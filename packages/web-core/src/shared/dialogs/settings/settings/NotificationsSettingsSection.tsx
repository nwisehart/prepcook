import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { SpinnerIcon } from '@phosphor-icons/react';
import type { NotificationTarget } from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import {
  SettingsCard,
  SettingsField,
  SettingsInput,
  SettingsSaveBar,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import {
  IntegrationSection,
  useIntegrationDescriptors,
} from './IntegrationSection';

export const NOTIFICATIONS_AREA = 'notifications';

/** `events` as typed: comma-separated kinds; blank means every event. */
const eventsToText = (events: string[] | undefined): string =>
  (events ?? []).join(', ');

const textToEvents = (text: string): string[] | undefined => {
  const events = text
    .split(',')
    .map((event) => event.trim())
    .filter(Boolean);
  return events.length === 0 ? undefined : events;
};

/** The `events` input every notification target carries. */
export function NotificationEventsField({
  target,
  onChange,
}: {
  target: NotificationTarget;
  onChange: (target: NotificationTarget) => void;
}) {
  const { t } = useTranslation('settings');
  // Kept as typed so a half-finished list survives re-renders; replaced when
  // the target's list changes underneath it (an entry above was removed).
  const [text, setText] = useState(() => eventsToText(target.events));
  useEffect(() => {
    setText((current) =>
      isEqual(textToEvents(current), target.events)
        ? current
        : eventsToText(target.events)
    );
  }, [target.events]);
  return (
    <SettingsField
      label={t('settings.notifications.eventsLabel')}
      description={t('settings.notifications.eventsHelper')}
    >
      <SettingsInput
        value={text}
        onChange={(value) => {
          setText(value);
          const { events: _previous, ...rest } = target;
          const events = textToEvents(value);
          onChange((events ? { ...rest, events } : rest) as NotificationTarget);
        }}
        placeholder="*"
      />
    </SettingsField>
  );
}

export function NotificationsSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();
  const descriptors = useIntegrationDescriptors();

  const saved = useMemo(
    () => config?.notifications?.targets ?? [],
    [config?.notifications?.targets]
  );
  const [draft, setDraft] = useState<NotificationTarget[]>(() =>
    cloneDeep(saved)
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!dirty) setDraft(cloneDeep(saved));
  }, [saved, dirty]);

  const hasUnsavedChanges = useMemo(
    () => !isEqual(draft, saved),
    [draft, saved]
  );

  useEffect(() => {
    setContextDirty(NOTIFICATIONS_AREA, hasUnsavedChanges);
    return () => setContextDirty(NOTIFICATIONS_AREA, false);
  }, [hasUnsavedChanges, setContextDirty]);

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      await updateAndSaveConfig({
        notifications: { ...config.notifications, targets: draft },
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.notifications.save.error'));
      console.error('Error saving notification targets:', err);
    } finally {
      setSaving(false);
    }
  };

  if (loading || descriptors.isLoading) {
    return (
      <div className="flex items-center justify-center py-8 gap-2">
        <SpinnerIcon
          className="size-icon-lg animate-spin text-brand"
          weight="bold"
        />
        <span className="text-normal">{t('settings.general.loading')}</span>
      </div>
    );
  }

  if (!config || descriptors.isError) {
    return (
      <div className="py-8">
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {error}
        </div>
      )}
      {success && (
        <div className="bg-success/10 border border-success/50 rounded-sm p-4 text-success font-medium">
          {t('settings.notifications.save.success')}
        </div>
      )}
      <SettingsCard
        title={t('settings.notifications.title')}
        description={t('settings.notifications.description')}
      >
        <IntegrationSection<NotificationTarget>
          area={NOTIFICATIONS_AREA}
          descriptors={descriptors.data?.[NOTIFICATIONS_AREA] ?? []}
          entries={draft}
          multiple
          onChange={(targets) => {
            setDirty(true);
            setDraft(targets);
          }}
          renderExtra={(target, update) => (
            <NotificationEventsField target={target} onChange={update} />
          )}
        />
      </SettingsCard>
      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        onSave={handleSave}
        onDiscard={() => {
          setDraft(cloneDeep(saved));
          setDirty(false);
        }}
      />
    </div>
  );
}
