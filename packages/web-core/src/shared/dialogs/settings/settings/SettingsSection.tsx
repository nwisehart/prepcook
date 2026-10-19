import { useTranslation } from 'react-i18next';
import { XIcon } from '@phosphor-icons/react';
import {
  renderSettingsSection,
  type SettingsSectionInitialState,
  type SettingsSectionType,
} from './settingsRegistry';

export type { SettingsSectionInitialState, SettingsSectionType };

interface SettingsSectionProps {
  type: SettingsSectionType;
  onClose?: () => void;
  initialState?: SettingsSectionInitialState[SettingsSectionType];
}

// Sections whose lead sentence says more than their nav description.
const LEAD_KEYS: Partial<Record<SettingsSectionType, string>> = {
  orchestrator: 'settings.layout.lead.orchestrator',
};

/**
 * One settings page: the section title and lead sentence, then the section's
 * cards in a column that grows to the bottom of the dialog so the section's
 * action row (SettingsSaveBar) sits at the bottom even when the page is short.
 */
export function SettingsSection({
  type,
  onClose,
  initialState,
}: SettingsSectionProps) {
  const { t } = useTranslation('settings');
  const leadKey = LEAD_KEYS[type] ?? `settings.layout.nav.${type}Desc`;

  return (
    <div className="flex min-h-full flex-col px-5 pt-[22px] md:px-8">
      <div className="flex w-full max-w-[760px] flex-1 flex-col gap-[18px]">
        <header className="flex items-start justify-between gap-4">
          <div className="flex min-w-0 flex-col gap-1">
            <h2 className="text-[22px] font-bold leading-tight tracking-[-0.3px] text-high">
              {t(`settings.layout.nav.${type}`)}
            </h2>
            <p className="text-[12.5px] leading-[1.45] text-low">
              {t(leadKey)}
            </p>
          </div>
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              className="hidden size-8 shrink-0 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand md:flex"
            >
              <XIcon className="size-4" />
              <span className="sr-only">
                {t('buttons.close', { ns: 'common' })}
              </span>
            </button>
          )}
        </header>

        <div className="flex flex-1 flex-col gap-[18px] pb-6">
          {renderSettingsSection(type, initialState)}
        </div>
      </div>
    </div>
  );
}
