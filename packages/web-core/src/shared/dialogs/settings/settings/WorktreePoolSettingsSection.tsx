import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import type { WorktreePoolConfig } from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsLoading,
  SettingsSaveBar,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';

const DEFAULT_POOL_CONFIG: WorktreePoolConfig = {
  enabled: false,
  max_trees: 8,
  root: null,
};

export function WorktreePoolSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();

  const [draft, setDraft] = useState<WorktreePoolConfig>(() =>
    config?.worktree_pool
      ? cloneDeep(config.worktree_pool)
      : cloneDeep(DEFAULT_POOL_CONFIG)
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!config) return;
    if (!dirty) {
      setDraft(
        config.worktree_pool
          ? cloneDeep(config.worktree_pool)
          : DEFAULT_POOL_CONFIG
      );
    }
  }, [config, dirty]);

  const hasUnsavedChanges = useMemo(() => {
    if (!config) return false;
    return !isEqual(draft, config.worktree_pool ?? DEFAULT_POOL_CONFIG);
  }, [draft, config]);

  // Sync dirty state to context for unsaved changes confirmation
  useEffect(() => {
    setContextDirty('worktree-pool', hasUnsavedChanges);
    return () => setContextDirty('worktree-pool', false);
  }, [hasUnsavedChanges, setContextDirty]);

  const updatePool = useCallback((patch: Partial<WorktreePoolConfig>) => {
    setDirty(true);
    setDraft((prev) => ({ ...prev, ...patch }));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      await updateAndSaveConfig({ worktree_pool: draft });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.worktreePool.save.error'));
      console.error('Error saving worktree pool config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    setDraft(
      config.worktree_pool
        ? cloneDeep(config.worktree_pool)
        : DEFAULT_POOL_CONFIG
    );
    setDirty(false);
  };

  if (loading) {
    return <SettingsLoading label={t('settings.general.loading')} />;
  }

  if (!config) {
    return (
      <div className="py-8">
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  return (
    <>
      {/* Status messages */}
      {error && (
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {error}
        </div>
      )}

      {success && (
        <div className="rounded-lg border border-success/30 bg-success-soft px-4 py-3 text-sm font-medium text-success">
          {t('settings.worktreePool.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.worktreePool.pool.title')}
        description={t('settings.worktreePool.pool.description')}
      >
        <SettingsCheckbox
          id="worktree-pool-enabled"
          label={t('settings.worktreePool.enabled.label')}
          description={t('settings.worktreePool.enabled.helper')}
          checked={draft.enabled}
          onChange={(checked) => updatePool({ enabled: checked })}
        />

        <SettingsField
          label={t('settings.worktreePool.maxTrees.label')}
          description={t('settings.worktreePool.maxTrees.helper')}
        >
          <SettingsInput
            value={String(draft.max_trees)}
            onChange={(value) => {
              const parsed = parseInt(value, 10);
              updatePool({ max_trees: Number.isNaN(parsed) ? 0 : parsed });
            }}
            placeholder="8"
            error={draft.max_trees < 1 || draft.max_trees > 9}
            className="max-w-[120px]"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.worktreePool.root.label')}
          description={t('settings.worktreePool.root.helper')}
        >
          <SettingsInput
            value={draft.root ?? ''}
            onChange={(value) => updatePool({ root: value || null })}
            placeholder={t('settings.worktreePool.root.placeholder')}
            monospace
          />
        </SettingsField>
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        saveDisabled={draft.max_trees < 1 || draft.max_trees > 9}
        onSave={handleSave}
        onDiscard={handleDiscard}
      />
    </>
  );
}
