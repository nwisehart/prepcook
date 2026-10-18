import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { SpinnerIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type {
  AppRunsConfig,
  Descriptor,
  ReproductionTarget,
  TestOutcome,
} from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { integrationsApi, remoteProjectsApi } from '@/shared/lib/api';
import {
  SettingsCard,
  SettingsField,
  SettingsSaveBar,
  SettingsSelect,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import { FieldInput, useIntegrationDescriptors } from './IntegrationSection';

export const REPRODUCTION_AREA = 'reproduction';

const EMPTY_APP_RUNS: AppRunsConfig = {
  port_range: '4200-4299',
  max_parallel: 2,
  default_target: 'local',
};

/** A board's choice: its own target, or `inherit` the default. */
type BoardChoice = ReproductionTarget | 'inherit';

interface Draft {
  appRuns: AppRunsConfig;
  /** Board id → its target; `null` inherits `app_runs.default_target`. */
  boards: Record<string, ReproductionTarget | null>;
}

/**
 * Where bug cards reproduce and verify, rendered from the reproduction
 * targets' descriptors: the `local` target's settings (the `app_runs`
 * section), the default target, and each board's choice.
 */
export function ReproductionSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();
  const descriptors = useIntegrationDescriptors();
  const targets: Descriptor[] = useMemo(
    () => descriptors.data?.[REPRODUCTION_AREA] ?? [],
    [descriptors.data]
  );

  const saved: Draft = useMemo(() => {
    const boards: Record<string, ReproductionTarget | null> = {};
    for (const [id, defaults] of Object.entries(
      config?.orchestrator?.project_defaults ?? {}
    )) {
      boards[id] = defaults?.reproduction?.target ?? null;
    }
    return {
      appRuns: { ...EMPTY_APP_RUNS, ...config?.app_runs },
      boards,
    };
  }, [config?.app_runs, config?.orchestrator?.project_defaults]);

  const [draft, setDraft] = useState<Draft>(() => cloneDeep(saved));
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [tests, setTests] = useState<
    Record<string, TestOutcome | 'running' | undefined>
  >({});

  useEffect(() => {
    if (!dirty) setDraft(cloneDeep(saved));
  }, [saved, dirty]);

  const hasUnsavedChanges = useMemo(
    () => !isEqual(draft, saved),
    [draft, saved]
  );

  useEffect(() => {
    setContextDirty(REPRODUCTION_AREA, hasUnsavedChanges);
    return () => setContextDirty(REPRODUCTION_AREA, false);
  }, [hasUnsavedChanges, setContextDirty]);

  // Board names instead of UUIDs, from every organization the user is in.
  const { data: organizationsData } = useUserOrganizations();
  const { data: remoteProjects } = useQuery<RemoteProject[]>({
    queryKey: [
      'reproduction-remote-projects',
      organizationsData?.organizations?.map((org) => org.id) ?? [],
    ],
    queryFn: async () => {
      const orgs = organizationsData?.organizations ?? [];
      const results = await Promise.all(
        orgs.map((org) =>
          remoteProjectsApi.listByOrganization(org.id).catch(() => [])
        )
      );
      return results.flat();
    },
    enabled: (organizationsData?.organizations?.length ?? 0) > 0,
  });
  const boardName = (id: string): string => {
    const label = config?.orchestrator?.project_defaults?.[id]?.label;
    return (
      remoteProjects?.find((project) => project.id === id)?.name ?? label ?? id
    );
  };

  const patchAppRuns = (updates: Partial<AppRunsConfig>) => {
    setDirty(true);
    setDraft((prev) => ({
      ...prev,
      appRuns: { ...prev.appRuns, ...updates },
    }));
    setTests({});
  };

  const setBoard = (id: string, choice: BoardChoice) => {
    setDirty(true);
    setDraft((prev) => ({
      ...prev,
      boards: { ...prev.boards, [id]: choice === 'inherit' ? null : choice },
    }));
  };

  const runTest = async (descriptor: Descriptor) => {
    setTests((current) => ({ ...current, [descriptor.kind]: 'running' }));
    const fields: Record<string, unknown> = {};
    for (const field of descriptor.fields) {
      fields[field.key] = (draft.appRuns as Record<string, unknown>)[field.key];
    }
    try {
      const outcome = await integrationsApi.test(
        REPRODUCTION_AREA,
        descriptor.kind,
        fields
      );
      setTests((current) => ({ ...current, [descriptor.kind]: outcome }));
    } catch (err) {
      setTests((current) => ({
        ...current,
        [descriptor.kind]: {
          ok: false,
          message: err instanceof Error ? err.message : String(err),
        },
      }));
    }
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      const projectDefaults = cloneDeep(config.orchestrator.project_defaults);
      for (const [id, target] of Object.entries(draft.boards)) {
        const board = projectDefaults[id];
        if (board) board.reproduction = { ...board.reproduction, target };
      }
      await updateAndSaveConfig({
        app_runs: draft.appRuns,
        orchestrator: {
          ...config.orchestrator,
          project_defaults: projectDefaults,
        },
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.reproduction.save.error'));
      console.error('Error saving reproduction settings:', err);
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

  const targetOptions = targets.map((descriptor) => ({
    value: descriptor.kind as ReproductionTarget,
    label: descriptor.display_name,
  }));
  const defaultName =
    targets.find((d) => d.kind === draft.appRuns.default_target)
      ?.display_name ?? draft.appRuns.default_target;
  const boardIds = Object.keys(draft.boards);

  return (
    <div className="space-y-6">
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {error}
        </div>
      )}
      {success && (
        <div className="bg-success/10 border border-success/50 rounded-sm p-4 text-success font-medium">
          {t('settings.reproduction.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.reproduction.targets.title')}
        description={t('settings.reproduction.targets.description')}
      >
        {targets.map((descriptor) => {
          const test = tests[descriptor.kind];
          return (
            <div
              key={descriptor.kind}
              className="space-y-4 rounded-sm border border-border p-base"
            >
              <div>
                <div className="text-sm font-medium text-high">
                  {descriptor.display_name}{' '}
                  <span className="font-ibm-plex-mono text-low">
                    {descriptor.kind}
                  </span>
                </div>
                <p className="text-sm text-low mt-1">{descriptor.summary}</p>
              </div>
              {descriptor.fields.map((field) => (
                <FieldInput
                  key={field.key}
                  id={`${REPRODUCTION_AREA}-${descriptor.kind}-${field.key}`}
                  field={field}
                  value={(draft.appRuns as Record<string, unknown>)[field.key]}
                  onChange={(value) => {
                    // The section's keys are never null: a cleared text
                    // input is empty, and a number keeps its last value.
                    if (value == null && field.kind.type === 'number') return;
                    patchAppRuns({
                      [field.key]: value ?? '',
                    } as Partial<AppRunsConfig>);
                  }}
                />
              ))}
              <div className="flex items-center gap-3">
                <PrimaryButton
                  variant="tertiary"
                  value={
                    test === 'running'
                      ? t('settings.integrations.section.testing')
                      : t('settings.integrations.section.test')
                  }
                  actionIcon={test === 'running' ? 'spinner' : undefined}
                  disabled={test === 'running'}
                  onClick={() => runTest(descriptor)}
                />
                {test && test !== 'running' && (
                  <span
                    className={
                      test.ok ? 'text-sm text-success' : 'text-sm text-error'
                    }
                  >
                    {test.message}
                  </span>
                )}
              </div>
            </div>
          );
        })}
        <SettingsField
          label={t('settings.reproduction.defaultTarget.label')}
          description={t('settings.reproduction.defaultTarget.helper')}
        >
          <SettingsSelect<ReproductionTarget>
            value={draft.appRuns.default_target}
            options={targetOptions}
            onChange={(target) => patchAppRuns({ default_target: target })}
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.reproduction.boards.title')}
        description={t('settings.reproduction.boards.description')}
      >
        {boardIds.length === 0 ? (
          <p className="text-sm text-low">
            {t('settings.reproduction.boards.empty')}
          </p>
        ) : (
          boardIds.map((id) => (
            <SettingsField key={id} label={boardName(id)}>
              <SettingsSelect<BoardChoice>
                value={draft.boards[id] ?? 'inherit'}
                options={[
                  {
                    value: 'inherit',
                    label: t('settings.reproduction.boards.inherit', {
                      name: defaultName,
                    }),
                  },
                  ...targetOptions,
                ]}
                onChange={(choice) => setBoard(id, choice)}
              />
            </SettingsField>
          ))
        )}
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        onSave={handleSave}
        onDiscard={() => {
          setDraft(cloneDeep(saved));
          setDirty(false);
          setTests({});
        }}
      />
    </div>
  );
}
