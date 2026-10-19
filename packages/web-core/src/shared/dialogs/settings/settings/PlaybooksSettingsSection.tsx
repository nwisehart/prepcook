import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { isEqual } from 'lodash';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import type { ProjectAutomationDefaults } from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { DEFAULT_PLAYBOOKS } from '@/shared/lib/stagePlaybooks';
import {
  SettingsCard,
  SettingsField,
  SettingsLoading,
  SettingsSaveBar,
  SettingsSelect,
  SettingsTextarea,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';

type PlaybookMap = Record<string, string>;

/// Which playbook set is being edited: the global texts every board inherits,
/// or one board's overrides (`orchestrator.project_defaults[id].playbooks`).
type Scope = { kind: 'global' } | { kind: 'board'; projectId: string };

// Annotated rather than inferred: `project_defaults ?? {}` would widen the
// index signature and lose the value type.
type BoardDefaults = { [key in string]?: ProjectAutomationDefaults };

function toPlaybookMap(playbooks?: { [key in string]?: string }): PlaybookMap {
  const map: PlaybookMap = {};
  for (const [key, value] of Object.entries(playbooks ?? {})) {
    if (typeof value === 'string') {
      map[key] = value;
    }
  }
  return map;
}

/// Board playbook overrides as saved in the config, in the same shape the
/// editor drafts them.
function savedBoardPlaybooks(
  defaults?: BoardDefaults
): Record<string, PlaybookMap> {
  const saved: Record<string, PlaybookMap> = {};
  for (const [projectId, board] of Object.entries(defaults ?? {})) {
    saved[projectId] = toPlaybookMap(board?.playbooks);
  }
  return saved;
}

export function PlaybooksSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();

  const [draft, setDraft] = useState<PlaybookMap>(() =>
    toPlaybookMap(config?.playbooks)
  );
  const [scope, setScope] = useState<Scope>({ kind: 'global' });
  // Board overrides, keyed by remote project id. Only keys the user actually
  // edited are held here, so an untouched stage keeps inheriting.
  const [boardDrafts, setBoardDrafts] = useState<Record<string, PlaybookMap>>(
    () => savedBoardPlaybooks(config?.orchestrator?.project_defaults)
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!config) return;
    if (!dirty) {
      setDraft(toPlaybookMap(config.playbooks));
      setBoardDrafts(
        savedBoardPlaybooks(config.orchestrator?.project_defaults)
      );
    }
  }, [config, dirty]);

  const hasUnsavedChanges = useMemo(() => {
    if (!config) return false;
    return (
      !isEqual(draft, toPlaybookMap(config.playbooks)) ||
      !isEqual(
        boardDrafts,
        savedBoardPlaybooks(config.orchestrator?.project_defaults)
      )
    );
  }, [draft, boardDrafts, config]);

  // Sync dirty state to context for unsaved changes confirmation
  useEffect(() => {
    setContextDirty('playbooks', hasUnsavedChanges);
    return () => setContextDirty('playbooks', false);
  }, [hasUnsavedChanges, setContextDirty]);

  const playbookKeys = useMemo(
    () =>
      Array.from(
        new Set([...Object.keys(draft), ...Object.keys(DEFAULT_PLAYBOOKS)])
      ).sort(),
    [draft]
  );

  // Boards available as scopes: whatever the orchestrator is configured for.
  const boardOptions = useMemo(() => {
    const defaults: BoardDefaults =
      config?.orchestrator?.project_defaults ?? {};
    return Object.entries(defaults).map(([projectId, board]) => ({
      value: projectId,
      label: board?.label?.trim() || `${projectId.slice(0, 8)}…`,
    }));
  }, [config]);

  const boardDraft =
    scope.kind === 'board' ? (boardDrafts[scope.projectId] ?? {}) : {};

  const updatePlaybook = useCallback(
    (key: string, value: string) => {
      setDirty(true);
      if (scope.kind === 'global') {
        setDraft((prev) => ({ ...prev, [key]: value }));
        return;
      }
      const { projectId } = scope;
      setBoardDrafts((prev) => ({
        ...prev,
        [projectId]: { ...(prev[projectId] ?? {}), [key]: value },
      }));
    },
    [scope]
  );

  /// Drops a board's override so the stage inherits the global text again.
  const clearOverride = useCallback(
    (key: string) => {
      if (scope.kind !== 'board') return;
      const { projectId } = scope;
      setDirty(true);
      setBoardDrafts((prev) => {
        const next = { ...(prev[projectId] ?? {}) };
        delete next[key];
        return { ...prev, [projectId]: next };
      });
    },
    [scope]
  );

  // Puts the shipped text back for every stage that has one. Global scope
  // only; a board's overrides are cleared one stage at a time.
  const resetToShippedTexts = useCallback(() => {
    setDirty(true);
    setDraft((prev) => ({ ...prev, ...DEFAULT_PLAYBOOKS }));
  }, []);
  const atShippedTexts = Object.entries(DEFAULT_PLAYBOOKS).every(
    ([key, text]) => draft[key] === text
  );

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      // Board overrides live inside the orchestrator config; rebuild it from
      // the saved value so this tab only changes playbook text.
      const savedDefaults: BoardDefaults =
        config?.orchestrator?.project_defaults ?? {};
      const project_defaults: BoardDefaults = {};
      for (const [projectId, board] of Object.entries(savedDefaults)) {
        if (!board) continue;
        project_defaults[projectId] = {
          ...board,
          playbooks: boardDrafts[projectId] ?? {},
        };
      }
      await updateAndSaveConfig({
        playbooks: draft,
        ...(config?.orchestrator
          ? { orchestrator: { ...config.orchestrator, project_defaults } }
          : {}),
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.playbooks.save.error'));
      console.error('Error saving playbooks config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    setDraft(toPlaybookMap(config.playbooks));
    setBoardDrafts(savedBoardPlaybooks(config.orchestrator?.project_defaults));
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
          {t('settings.playbooks.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.playbooks.section.title')}
        description={t('settings.playbooks.section.description')}
      >
        {boardOptions.length > 0 && (
          <SettingsField
            label={t('settings.playbooks.scope.label')}
            description={t('settings.playbooks.scope.helper')}
          >
            <SettingsSelect<string>
              value={scope.kind === 'global' ? 'global' : scope.projectId}
              options={[
                {
                  value: 'global',
                  label: t('settings.playbooks.scope.global'),
                },
                ...boardOptions,
              ]}
              onChange={(value) =>
                setScope(
                  value === 'global'
                    ? { kind: 'global' }
                    : { kind: 'board', projectId: value }
                )
              }
              className="max-w-[360px]"
            />
          </SettingsField>
        )}

        {playbookKeys.map((key) => {
          const globalValue = draft[key] ?? '';
          const overridden =
            scope.kind === 'board' && boardDraft[key] !== undefined;
          const value =
            scope.kind === 'board'
              ? (boardDraft[key] ?? globalValue)
              : globalValue;
          const defaultValue = DEFAULT_PLAYBOOKS[key];
          const canReset =
            scope.kind === 'global' &&
            defaultValue !== undefined &&
            globalValue !== defaultValue;
          return (
            <SettingsField
              key={key}
              label={key}
              className="[&>label]:font-mono [&>label]:text-[13px]"
              description={
                scope.kind === 'global'
                  ? t('settings.playbooks.entry.helper')
                  : overridden
                    ? t('settings.playbooks.entry.overridden')
                    : t('settings.playbooks.entry.usingGlobal')
              }
            >
              <SettingsTextarea
                value={value}
                onChange={(next) => updatePlaybook(key, next)}
                rows={6}
                monospace
              />
              {canReset && (
                <div className="flex justify-end">
                  <PrimaryButton
                    variant="secondary"
                    value={t('settings.playbooks.reset')}
                    onClick={() => updatePlaybook(key, defaultValue)}
                  />
                </div>
              )}
              {overridden && (
                <div className="flex justify-end">
                  <PrimaryButton
                    variant="secondary"
                    value={t('settings.playbooks.entry.useGlobal')}
                    onClick={() => clearOverride(key)}
                  />
                </div>
              )}
            </SettingsField>
          );
        })}
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        onSave={handleSave}
        onDiscard={handleDiscard}
        onReset={scope.kind === 'global' ? resetToShippedTexts : undefined}
        resetDisabled={atShippedTexts}
      />
    </>
  );
}
