import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/shared/lib/utils';
import { cloneDeep, isEqual } from 'lodash';
import { useQueries } from '@tanstack/react-query';
import type {
  AgentDescriptor,
  AgentsConfig,
  AvailabilityInfo,
  ExecutorProfileId,
  StageProfiles,
  TestOutcome,
} from 'shared/types';
import { configApi, integrationsApi } from '@/shared/lib/api';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import {
  SettingsCard,
  SettingsField,
  SettingsLoading,
  SettingsSaveBar,
  SettingsSelect,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import { useIntegrationDescriptors } from './IntegrationSection';
import {
  EMPTY_STAGE_PROFILES,
  ProfileSelect,
  StageProfileFields,
  externalDefault,
  parseProfileKey,
  profileKey,
  stageDefault,
  useProfileOptions,
} from './StageProfileFields';

export const AGENTS_AREA = 'agents';

/** The agents area's descriptor for the `agents` section's own settings. */
const AGENT_SETTINGS_KIND = 'settings';

/** The `agents` keys the settings descriptor names, beside `stages`. */
type AgentSettingKey = Exclude<keyof AgentsConfig, 'stages'>;

interface Draft {
  executor_profile: ExecutorProfileId;
  stages: StageProfiles;
  settings: Pick<AgentsConfig, AgentSettingKey>;
}

/** Every coding agent's own descriptor, from the `agents` area. */
export function useAgentDescriptors(): AgentDescriptor[] {
  const descriptorsQuery = useIntegrationDescriptors();
  return useMemo(
    () =>
      (descriptorsQuery.data?.[AGENTS_AREA] ?? []).flatMap((descriptor) =>
        descriptor.agent ? [descriptor.agent] : []
      ),
    [descriptorsQuery.data]
  );
}

/**
 * Which coding agent runs which stage, and every agent Prepcook can run: what
 * it needs, how it signs in, whether this machine has it, and a probe.
 */
export function CodingAgentsSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, profiles, updateAndSaveConfig } = useUserSystem();
  const descriptorsQuery = useIntegrationDescriptors();
  const agents = useAgentDescriptors();
  const settingsFields = useMemo(
    () =>
      descriptorsQuery.data?.[AGENTS_AREA]?.find(
        (descriptor) => descriptor.kind === AGENT_SETTINGS_KIND
      )?.fields ?? [],
    [descriptorsQuery.data]
  );
  const profileOptions = useProfileOptions(profiles);

  const saved: Draft | null = useMemo(
    () =>
      config
        ? {
            executor_profile: config.executor_profile,
            stages: { ...EMPTY_STAGE_PROFILES, ...config.agents?.stages },
            settings: { external: config.agents?.external ?? null },
          }
        : null,
    [config]
  );
  const [draft, setDraft] = useState<Draft | null>(() => cloneDeep(saved));
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
    setContextDirty('coding-agents', hasUnsavedChanges);
    return () => setContextDirty('coding-agents', false);
  }, [hasUnsavedChanges, setContextDirty]);

  // Read from each agent's sign-in files when this page opens, never at
  // startup: the check is the existing discovery, run on demand.
  const availability = useQueries({
    queries: agents.map((agent) => ({
      queryKey: ['agent-availability', agent.executor],
      queryFn: () => configApi.checkAgentAvailability(agent.executor),
      staleTime: 60_000,
    })),
  });

  const runTest = async (agent: AgentDescriptor) => {
    setTests((current) => ({ ...current, [agent.executor]: 'running' }));
    try {
      const outcome = await integrationsApi.test(AGENTS_AREA, agent.executor);
      setTests((current) => ({ ...current, [agent.executor]: outcome }));
    } catch (err) {
      setTests((current) => ({
        ...current,
        [agent.executor]: {
          ok: false,
          message: err instanceof Error ? err.message : String(err),
        },
      }));
    }
  };

  const handleSave = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      await updateAndSaveConfig({
        executor_profile: draft.executor_profile,
        agents: { ...config?.agents, ...draft.settings, stages: draft.stages },
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.codingAgents.save.error'));
      console.error('Error saving agent settings:', err);
    } finally {
      setSaving(false);
    }
  };

  if (loading || descriptorsQuery.isLoading) {
    return <SettingsLoading label={t('settings.general.loading')} />;
  }

  if (!config || !draft || descriptorsQuery.isError) {
    return (
      <div className="py-8">
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  const availabilityLabel = (info: AvailabilityInfo | undefined): string => {
    switch (info?.type) {
      case 'LOGIN_DETECTED':
        return t('settings.agents.availability.loginDetected');
      case 'INSTALLATION_FOUND':
        return t('settings.agents.availability.installationFound');
      case 'NOT_FOUND':
        return t('settings.agents.availability.notFoundSimple');
      default:
        return t('settings.agents.availability.checking');
    }
  };

  return (
    <>
      {error && (
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {error}
        </div>
      )}
      {success && (
        <div className="rounded-lg border border-success/30 bg-success-soft px-4 py-3 text-sm font-medium text-success">
          {t('settings.codingAgents.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.codingAgents.stagesCard.title')}
        description={t('settings.codingAgents.stagesCard.description')}
      >
        <SettingsField
          label={t('settings.codingAgents.defaultLabel')}
          description={t('settings.codingAgents.defaultHelper')}
        >
          <SettingsSelect<string>
            value={profileKey(draft.executor_profile)}
            options={profileOptions}
            onChange={(key) => {
              setDirty(true);
              setDraft((prev) =>
                prev
                  ? { ...prev, executor_profile: parseProfileKey(key) }
                  : prev
              );
            }}
          />
        </SettingsField>
        <StageProfileFields
          stages={draft.stages}
          fallback={(stage) =>
            stageDefault(stage, draft.executor_profile, agents)
          }
          options={profileOptions}
          onChange={(stages) => {
            setDirty(true);
            setDraft((prev) => (prev ? { ...prev, stages } : prev));
          }}
        />
        {settingsFields
          .filter((field) => field.kind.type === 'profile')
          .map((field) => {
            const key = field.key as AgentSettingKey;
            const planning =
              draft.stages.planning ??
              stageDefault('planning', draft.executor_profile, agents);
            return (
              <SettingsField
                key={field.key}
                label={field.label}
                description={field.help}
              >
                <ProfileSelect
                  value={draft.settings[key]}
                  inherited={externalDefault(planning, agents)}
                  inheritLabel={t('settings.codingAgents.stages.inherit')}
                  options={profileOptions}
                  onChange={(profile) => {
                    setDirty(true);
                    setDraft((prev) =>
                      prev
                        ? {
                            ...prev,
                            settings: { ...prev.settings, [key]: profile },
                          }
                        : prev
                    );
                  }}
                />
              </SettingsField>
            );
          })}
      </SettingsCard>

      <SettingsCard
        title={t('settings.codingAgents.agentsCard.title')}
        description={t('settings.codingAgents.agentsCard.description')}
      >
        {agents.map((agent, index) => {
          const info = availability[index]?.data;
          const test = tests[agent.executor];
          return (
            <div
              key={agent.executor}
              className="flex flex-col gap-3 rounded-lg border border-border px-4 py-3"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex min-w-0 flex-col gap-1">
                  <div className="text-sm font-semibold text-high">
                    {agent.display_name}
                  </div>
                  <p className="text-[12.5px] leading-[1.45] text-low">
                    {agent.summary}
                  </p>
                </div>
                <span
                  className={cn(
                    'inline-flex h-[22px] shrink-0 items-center whitespace-nowrap rounded-full px-2 text-xs font-medium',
                    info && info.type !== 'NOT_FOUND'
                      ? 'bg-success-soft text-success'
                      : 'bg-panel text-normal'
                  )}
                >
                  {availabilityLabel(info)}
                </span>
              </div>
              <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-[13px]">
                <dt className="text-low">
                  {t('settings.codingAgents.command')}
                </dt>
                <dd className="text-high font-mono text-[12.5px] break-all">
                  {agent.command || t('settings.codingAgents.builtIn')}
                </dd>
                <dt className="text-low">
                  {t('settings.codingAgents.signIn')}
                </dt>
                <dd className="text-normal">{agent.sign_in}</dd>
                <dt className="text-low">{t('settings.codingAgents.mcp')}</dt>
                <dd className="text-normal">
                  {t(`settings.codingAgents.mcpWiring.${agent.mcp}`)}
                </dd>
                {agent.variants.length > 0 && (
                  <>
                    <dt className="text-low">
                      {t('settings.codingAgents.variants')}
                    </dt>
                    <dd className="text-normal">
                      {agent.variants
                        .map((variant) =>
                          variant === agent.economy_variant
                            ? t('settings.codingAgents.economy', { variant })
                            : variant === agent.guarded_variant
                              ? t('settings.codingAgents.guarded', { variant })
                              : variant
                        )
                        .join(', ')}
                    </dd>
                  </>
                )}
              </dl>
              <div className="flex items-center gap-3 border-t border-divider pt-3">
                <PrimaryButton
                  variant="secondary"
                  value={
                    test === 'running'
                      ? t('settings.codingAgents.testing')
                      : t('settings.codingAgents.test')
                  }
                  actionIcon={test === 'running' ? 'spinner' : undefined}
                  disabled={test === 'running'}
                  onClick={() => runTest(agent)}
                />
                {test && test !== 'running' && (
                  <span
                    className={cn(
                      'inline-flex min-w-0 items-center gap-2 text-[12.5px] font-semibold',
                      test.ok ? 'text-success' : 'text-error'
                    )}
                  >
                    <span
                      className={cn(
                        'size-2 shrink-0 rounded-full',
                        test.ok ? 'bg-success' : 'bg-error'
                      )}
                    />
                    {test.message}
                  </span>
                )}
                <a
                  href={agent.docs_url}
                  target="_blank"
                  rel="noreferrer"
                  className="ml-auto shrink-0 text-[13px] font-semibold text-brand-secondary hover:underline"
                >
                  {t('settings.codingAgents.docs')}
                </a>
              </div>
            </div>
          );
        })}
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
    </>
  );
}
