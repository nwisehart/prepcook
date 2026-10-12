import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { SpinnerIcon } from '@phosphor-icons/react';
import type { IntegrationsConfig } from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsSaveBar,
  SettingsTextarea,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';

const EMPTY_INTEGRATIONS: IntegrationsConfig = {
  reviewer_map: {},
  reviewer_exclude_emails: [],
  auto_assign_reviewers: true,
  bot_logins: [],
  sentry_access_token: null,
  sentry_host: null,
};

// One "email=username" pair per line; blank/invalid lines are dropped.
const mapToText = (map: IntegrationsConfig['reviewer_map']): string =>
  Object.entries(map ?? {})
    .map(([email, username]) => `${email}=${username ?? ''}`)
    .join('\n');

const textToMap = (text: string): IntegrationsConfig['reviewer_map'] => {
  const map: { [key: string]: string } = {};
  for (const line of text.split('\n')) {
    const [email, username] = line.split('=').map((part) => part.trim());
    if (email && username) map[email.toLowerCase()] = username;
  }
  return map;
};

const textToList = (text: string): string[] =>
  text
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);

// Trim to null so an emptied field clears the override (falling back to the
// runner environment) rather than persisting an empty string.
const orNull = (value: string): string | null => {
  const trimmed = value.trim();
  return trimmed === '' ? null : trimmed;
};

export function IntegrationsSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();

  const [draft, setDraft] = useState<IntegrationsConfig>(() =>
    config?.integrations
      ? cloneDeep(config.integrations)
      : cloneDeep(EMPTY_INTEGRATIONS)
  );
  // Raw text mirrors of the list/map fields, so half-typed lines survive
  // re-renders; parsed into the draft on every change.
  const [mapText, setMapText] = useState(() =>
    mapToText(config?.integrations?.reviewer_map ?? {})
  );
  const [excludeText, setExcludeText] = useState(() =>
    (config?.integrations?.reviewer_exclude_emails ?? []).join('\n')
  );
  const [botsText, setBotsText] = useState(() =>
    (config?.integrations?.bot_logins ?? []).join('\n')
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!config || dirty) return;
    const integrations = config.integrations
      ? cloneDeep(config.integrations)
      : cloneDeep(EMPTY_INTEGRATIONS);
    setDraft(integrations);
    setMapText(mapToText(integrations.reviewer_map ?? {}));
    setExcludeText((integrations.reviewer_exclude_emails ?? []).join('\n'));
    setBotsText((integrations.bot_logins ?? []).join('\n'));
  }, [config, dirty]);

  const hasUnsavedChanges = useMemo(() => {
    if (!config) return false;
    return !isEqual(draft, config.integrations ?? EMPTY_INTEGRATIONS);
  }, [draft, config]);

  useEffect(() => {
    setContextDirty('integrations', hasUnsavedChanges);
    return () => setContextDirty('integrations', false);
  }, [hasUnsavedChanges, setContextDirty]);

  const patch = (updates: Partial<IntegrationsConfig>) => {
    setDirty(true);
    setDraft((prev) => ({ ...prev, ...updates }));
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      await updateAndSaveConfig({ integrations: draft });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.integrations.save.error'));
      console.error('Error saving integrations config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    setDraft(
      config.integrations
        ? cloneDeep(config.integrations)
        : cloneDeep(EMPTY_INTEGRATIONS)
    );
    setDirty(false);
  };

  if (loading) {
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

  if (!config) {
    return (
      <div className="py-8">
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  return (
    <>
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {error}
        </div>
      )}
      {success && (
        <div className="bg-success/10 border border-success/50 rounded-sm p-4 text-success font-medium">
          {t('settings.integrations.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.integrations.reviewers.title')}
        description={t('settings.integrations.reviewers.description')}
      >
        <SettingsCheckbox
          id="auto-assign-reviewers"
          label={t('settings.integrations.reviewers.autoAssignLabel')}
          description={t('settings.integrations.reviewers.autoAssignHelper')}
          checked={draft.auto_assign_reviewers}
          onChange={(checked) => patch({ auto_assign_reviewers: checked })}
        />
        <SettingsField
          label={t('settings.integrations.reviewers.mapLabel')}
          description={t('settings.integrations.reviewers.mapHelper')}
        >
          <SettingsTextarea
            value={mapText}
            onChange={(value) => {
              setMapText(value);
              patch({ reviewer_map: textToMap(value) });
            }}
            placeholder={'ada@example.com=ada\nbob@example.com=bob.smith'}
            rows={5}
            monospace
          />
        </SettingsField>
        <SettingsField
          label={t('settings.integrations.reviewers.excludeLabel')}
          description={t('settings.integrations.reviewers.excludeHelper')}
        >
          <SettingsTextarea
            value={excludeText}
            onChange={(value) => {
              setExcludeText(value);
              patch({ reviewer_exclude_emails: textToList(value) });
            }}
            rows={2}
            monospace
          />
        </SettingsField>
        <SettingsField
          label={t('settings.integrations.reviewers.botsLabel')}
          description={t('settings.integrations.reviewers.botsHelper')}
        >
          <SettingsTextarea
            value={botsText}
            onChange={(value) => {
              setBotsText(value);
              patch({ bot_logins: textToList(value) });
            }}
            rows={2}
            monospace
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.integrations.sentry.title')}
        description={t('settings.integrations.sentry.description')}
      >
        <SettingsField
          label={t('settings.integrations.sentry.tokenLabel')}
          description={t('settings.integrations.sentry.tokenHelper')}
        >
          <SettingsInput
            type="password"
            value={draft.sentry_access_token ?? ''}
            onChange={(value) => patch({ sentry_access_token: orNull(value) })}
            placeholder={t('settings.integrations.fromRunnerEnv')}
          />
        </SettingsField>
        <SettingsField
          label={t('settings.integrations.sentry.hostLabel')}
          description={t('settings.integrations.sentry.hostHelper')}
        >
          <SettingsInput
            value={draft.sentry_host ?? ''}
            onChange={(value) => patch({ sentry_host: orNull(value) })}
            placeholder="sentry.io"
          />
        </SettingsField>
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        onSave={handleSave}
        onDiscard={handleDiscard}
      />
    </>
  );
}
