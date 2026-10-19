import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { CaretLeftIcon, XIcon } from '@phosphor-icons/react';
import { create, useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/shared/lib/modals';

import { cn } from '@/shared/lib/utils';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { SettingsSection } from './settings/SettingsSection';
import type {
  SettingsSectionType,
  SettingsSectionInitialState,
} from './settings/SettingsSection';
import {
  SETTINGS_SECTION_DEFINITIONS,
  isHostSpecificSettingsSection,
  type SettingsSectionGroup,
} from './settings/settingsRegistry';
import {
  SettingsDirtyProvider,
  useSettingsDirty,
} from './settings/SettingsDirtyContext';
import { SettingsMachineUserSystemProvider } from './settings/SettingsMachineUserSystemProvider';
import { useIntegrationDescriptors } from './settings/IntegrationSection';
import {
  TICKETING_AREA,
  hasProviderSettings,
} from './settings/TicketingSettingsSection';
import { ConfirmDialog } from '@prepcook/ui/components/ConfirmDialog';

export interface SettingsDialogProps {
  initialSection?: SettingsSectionType;
  initialState?: SettingsSectionInitialState[SettingsSectionType];
}

interface SettingsDialogContentProps {
  initialSection?: SettingsSectionType;
  initialState?: SettingsSectionInitialState[SettingsSectionType];
  onClose: () => void;
}

const NAV_GROUPS: { group: SettingsSectionGroup; labelKey: string }[] = [
  { group: 'host', labelKey: 'settings.layout.nav.thisMachine' },
  { group: 'integrations', labelKey: 'settings.layout.nav.integrationsGroup' },
  { group: 'universal', labelKey: 'settings.layout.nav.boardGroup' },
];

/**
 * The connected tracker's name beside the Ticketing row, read from the saved
 * config: shown once any field its provider reads is set.
 */
function TrackerNavChip() {
  const { config } = useUserSystem();
  const descriptors = useIntegrationDescriptors();
  const ticketing = config?.ticketing;
  const descriptor = descriptors.data?.[TICKETING_AREA]?.find(
    (candidate) => candidate.kind === ticketing?.kind
  );
  if (!ticketing || !descriptor || !hasProviderSettings(ticketing, descriptor))
    return null;
  return (
    <span className="ml-auto inline-flex h-[22px] shrink-0 items-center rounded-full bg-success-soft px-2 text-xs font-medium text-success">
      {descriptor.display_name}
    </span>
  );
}

function SettingsDialogNavigation({
  activeSection,
  onSectionSelect,
}: {
  activeSection: SettingsSectionType;
  onSectionSelect: (sectionId: SettingsSectionType) => void;
}) {
  const { t } = useTranslation('settings');

  return (
    <nav
      aria-label={t('settings.layout.nav.title')}
      className="flex flex-1 flex-col gap-0.5 overflow-y-auto px-3 pb-6"
    >
      {NAV_GROUPS.map(({ group, labelKey }, groupIndex) => (
        <div key={group} className="flex flex-col gap-0.5">
          <div
            className={cn(
              'px-2.5 pb-1 text-[11px] font-bold uppercase tracking-[0.08em] text-low',
              groupIndex === 0 ? 'pt-1.5' : 'pt-3.5'
            )}
          >
            {t(labelKey)}
          </div>
          {SETTINGS_SECTION_DEFINITIONS.filter(
            (section) => section.group === group
          ).map((section) => {
            const isActive = activeSection === section.id;
            return (
              <button
                key={section.id}
                type="button"
                onClick={() => onSectionSelect(section.id)}
                aria-current={isActive ? 'page' : undefined}
                className={cn(
                  'flex w-full items-center gap-2.5 rounded px-2.5 py-2 text-left text-sm text-high transition-colors',
                  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
                  isActive
                    ? 'bg-panel font-semibold'
                    : 'font-medium hover:bg-panel'
                )}
              >
                <span className="truncate">
                  {t(`settings.layout.nav.${section.id}`)}
                </span>
                {section.id === 'ticketing' && <TrackerNavChip />}
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}

function SettingsDialogContent({
  initialSection,
  initialState,
  onClose,
}: SettingsDialogContentProps) {
  const { t } = useTranslation('settings');
  const { isDirty } = useSettingsDirty();

  const resolvedInitialSection = useMemo<SettingsSectionType>(() => {
    if (
      initialSection &&
      SETTINGS_SECTION_DEFINITIONS.some(
        (section) => section.id === initialSection
      )
    ) {
      return initialSection;
    }

    return 'general';
  }, [initialSection]);

  const [activeSection, setActiveSection] = useState<SettingsSectionType>(
    resolvedInitialSection
  );
  // On mobile, false shows the nav menu and true shows the active section
  const [mobileShowContent, setMobileShowContent] = useState<boolean>(
    initialSection === resolvedInitialSection
  );
  const isConfirmingRef = useRef(false);

  const handleCloseWithConfirmation = useCallback(async () => {
    if (isConfirmingRef.current) return;

    if (isDirty) {
      isConfirmingRef.current = true;
      try {
        const result = await ConfirmDialog.show({
          title: t('settings.unsavedChanges.title'),
          message: t('settings.unsavedChanges.message'),
          confirmText: t('settings.unsavedChanges.discard'),
          cancelText: t('settings.unsavedChanges.cancel'),
          variant: 'destructive',
        });
        if (result === 'confirmed') {
          onClose();
        }
      } finally {
        isConfirmingRef.current = false;
      }
    } else {
      onClose();
    }
  }, [isDirty, onClose, t]);

  const handleSectionSelect = (sectionId: SettingsSectionType) => {
    setActiveSection(sectionId);
    setMobileShowContent(true);
  };

  const handleMobileBack = () => {
    setMobileShowContent(false);
  };

  // Handle ESC key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        handleCloseWithConfirmation();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleCloseWithConfirmation]);

  return (
    <>
      {/* Overlay */}
      <div
        data-tauri-drag-region
        className="fixed inset-0 z-[9998] bg-scrim/45 animate-in fade-in-0 duration-200"
        onClick={handleCloseWithConfirmation}
      />
      {/* Positioning: full screen on mobile; full height and at most 1180px
          wide on desktop. Clicks outside the dialog fall through to the
          overlay. */}
      <div className="pointer-events-none fixed inset-0 z-[9999] flex justify-center md:p-6">
        <div
          role="dialog"
          aria-modal="true"
          aria-label={t('settings.layout.nav.title')}
          className={cn(
            'pointer-events-auto flex h-full w-full overflow-hidden bg-secondary text-normal',
            'animate-in fade-in-0 slide-in-from-bottom-4 duration-200',
            'md:max-w-[1180px] md:rounded-xl md:border md:border-border md:shadow-overlay'
          )}
        >
          {/* Section nav - hidden on mobile when showing content */}
          <div
            className={cn(
              'flex w-full flex-col bg-secondary md:w-[228px] md:shrink-0 md:border-r md:border-border',
              mobileShowContent ? 'hidden md:flex' : 'flex'
            )}
          >
            <div className="flex items-center justify-between px-5 pb-2.5 pt-[22px]">
              <h2 className="text-lg font-bold text-high">
                {t('settings.layout.nav.title')}
              </h2>
              {/* Close button - mobile only */}
              <button
                type="button"
                onClick={handleCloseWithConfirmation}
                className="rounded p-1 text-low hover:bg-panel hover:text-high md:hidden"
              >
                <XIcon className="size-4" />
                <span className="sr-only">
                  {t('buttons.close', { ns: 'common' })}
                </span>
              </button>
            </div>
            <SettingsDialogNavigation
              activeSection={activeSection}
              onSectionSelect={handleSectionSelect}
            />
          </div>
          {/* Content - hidden on mobile when showing nav */}
          <div
            className={cn(
              'relative min-w-0 flex-1 flex-col overflow-hidden bg-primary',
              mobileShowContent ? 'flex' : 'hidden md:flex'
            )}
          >
            {/* Mobile header with back button */}
            <div className="flex items-center gap-2 border-b border-border bg-secondary p-3 md:hidden">
              <button
                type="button"
                onClick={handleMobileBack}
                className="rounded p-1 text-low hover:bg-panel hover:text-high"
              >
                <CaretLeftIcon className="size-4" />
              </button>
              <span className="text-sm font-semibold text-high">
                {t(`settings.layout.nav.${activeSection}`)}
              </span>
              <button
                type="button"
                onClick={handleCloseWithConfirmation}
                className="ml-auto rounded p-1 text-low hover:bg-panel hover:text-high"
              >
                <XIcon className="size-4" />
                <span className="sr-only">
                  {t('buttons.close', { ns: 'common' })}
                </span>
              </button>
            </div>
            {/* Section content; keyed so each section opens scrolled to the top */}
            <div key={activeSection} className="flex-1 overflow-y-auto">
              {isHostSpecificSettingsSection(activeSection) ? (
                <SettingsMachineUserSystemProvider>
                  <SettingsSection
                    type={activeSection}
                    onClose={handleCloseWithConfirmation}
                    initialState={initialState}
                  />
                </SettingsMachineUserSystemProvider>
              ) : (
                <SettingsSection
                  type={activeSection}
                  onClose={handleCloseWithConfirmation}
                  initialState={initialState}
                />
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

const SettingsDialogImpl = create<SettingsDialogProps>(
  ({ initialSection, initialState }) => {
    const modal = useModal();
    const handleClose = useCallback(() => {
      modal.hide();
      modal.resolve();
      modal.remove();
    }, [modal]);

    return createPortal(
      <SettingsDirtyProvider>
        <SettingsDialogContent
          initialSection={initialSection}
          initialState={initialState}
          onClose={handleClose}
        />
      </SettingsDirtyProvider>,
      document.body
    );
  }
);

export const SettingsDialog = defineModal<SettingsDialogProps | void, void>(
  SettingsDialogImpl
);
