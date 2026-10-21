import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual, pick } from 'lodash';
import { CaretRightIcon, TrashIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type {
  ApiConfig,
  DeployWatch,
  InAppGate,
  NotificationTarget,
  NotifierKind,
  OrchestratorConfig,
  ProjectAutomationDefaults,
  Repo,
} from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { remoteProjectsApi, repoApi } from '@/shared/lib/api';
import { cn } from '@/shared/lib/utils';
import { IconButton } from '@prepcook/ui/components/IconButton';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import {
  SettingsAlert,
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsLoading,
  SettingsSaveBar,
  SettingsSelect,
  SettingsTextarea,
  settingsHelperClassName,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import {
  FieldInput,
  IntegrationSection,
  useIntegrationDescriptors,
} from './IntegrationSection';
import {
  NOTIFICATIONS_AREA,
  NotificationEventsField,
} from './NotificationsSettingsSection';
import { useAgentDescriptors } from './CodingAgentsSettingsSection';
import {
  AGENT_STAGES,
  EMPTY_STAGE_PROFILES,
  ProfileSelect,
  StageProfileFields,
  stageDefault,
  useProfileOptions,
} from './StageProfileFields';

// `poll_interval_secs` is typed bigint (u64 via ts-rs) but is a plain JSON
// number at runtime; keep it a number so config serialization stays valid.
const DEFAULT_ORCHESTRATOR_CONFIG: OrchestratorConfig = {
  mode: 'off',
  poll_interval_secs: 15 as unknown as bigint,
  deployed_retention_days: null,
  cleanup_retention_days: null,
  pattern_threshold: 5,
  default_board: null,
  max_launches_per_tick: 3,
  workspace_retention_days: 30,
  autonomy: 'off',
  autonomy_review_iterations: 3,
  digest_at_local: null,
  eob_at_local: null,
  daily_timezone: null,
  project_defaults: {},
  seat_monitor: false,
};

// The global values this page edits. Reset to defaults touches only these:
// the file-only settings and the automated projects are kept.
const GLOBAL_FIELD_KEYS = [
  'mode',
  'poll_interval_secs',
  'deployed_retention_days',
  'cleanup_retention_days',
  'pattern_threshold',
  'max_launches_per_tick',
  'workspace_retention_days',
  'autonomy',
  'autonomy_review_iterations',
] as const;

const pickGlobalFields = (
  config: OrchestratorConfig = DEFAULT_ORCHESTRATOR_CONFIG
) => pick(config, GLOBAL_FIELD_KEYS);

/** The stable HTTP API's settings area, and its card route's descriptor. */
const API_AREA = 'api';
const CARD_API_KIND = 'cards';
const DEFAULT_API_CONFIG: ApiConfig = { cards: { daily_limit: 50 } };

const EMPTY_DEFAULTS: ProjectAutomationDefaults = {
  repo_ids: [],
  executor_profile: null,
  agents: { stages: EMPTY_STAGE_PROFILES, external: null },
  label: null,
  ticketing: {
    watcher_enabled: null,
    jql_filter: null,
    project_key: null,
    assigned_jql: null,
  },
  notifications: {
    enabled: null,
    skip_global: [],
    targets: [],
  },
  merge_requests: {
    reviewer_map: {},
    reviewer_exclude_emails: [],
    auto_assign_reviewers: null,
    bot_logins: [],
  },
  playbooks: {},
  autonomy: null,
  autonomy_review_iterations: null,
  deployed_retention: 'inherit',
  cleanup_retention: 'inherit',
  pattern_threshold: null,
  deploy_watch: null,
  reproduction: { target: null },
  evidence: { enabled: null, skip_global: [], sources: [] },
};

// Boards written before per-board overrides existed (or by an older client)
// deserialize without the override objects; fill them so the inputs below can
// treat them as always present.
const withOverrideShape = (
  defaults: ProjectAutomationDefaults
): ProjectAutomationDefaults => ({
  ...cloneDeep(EMPTY_DEFAULTS),
  ...defaults,
  ticketing: { ...EMPTY_DEFAULTS.ticketing, ...defaults.ticketing },
  notifications: { ...EMPTY_DEFAULTS.notifications, ...defaults.notifications },
  merge_requests: {
    ...EMPTY_DEFAULTS.merge_requests,
    ...defaults.merge_requests,
  },
  reproduction: { ...EMPTY_DEFAULTS.reproduction, ...defaults.reproduction },
  evidence: { ...EMPTY_DEFAULTS.evidence, ...defaults.evidence },
  agents: {
    ...EMPTY_DEFAULTS.agents,
    ...defaults.agents,
    stages: { ...EMPTY_STAGE_PROFILES, ...defaults.agents?.stages },
  },
});

// Overrides are tri-state in the config (null = inherit the global), which a
// checkbox cannot express.
type TriState = 'inherit' | 'on' | 'off';

const toTriState = (value: boolean | null | undefined): TriState =>
  value == null ? 'inherit' : value ? 'on' : 'off';

const fromTriState = (value: TriState): boolean | null =>
  value === 'inherit' ? null : value === 'on';

// Both fields blank turns the staging watch off; a half-filled pair is kept
// so it can be finished, and the server treats it as off until it is.
const toDeployWatch = (jobName: string, gitRef: string): DeployWatch | null =>
  jobName.trim() === '' && gitRef.trim() === ''
    ? null
    : { job_name: jobName, git_ref: gitRef };

// Blank input clears the override rather than persisting an empty string.
const orNull = (value: string): string | null =>
  value.trim() === '' ? null : value.trim();

// One "email=username" pair per line, matching Settings → Integrations.
const mapToText = (map: { [key in string]?: string }): string =>
  Object.entries(map ?? {})
    .map(([email, username]) => `${email}=${username ?? ''}`)
    .join('\n');

const textToMap = (text: string): { [key: string]: string } => {
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

/// How many settings this board overrides — surfaced on the collapsed
/// summary so a board that differs from the globals is visible at a glance.
const countOverrides = (defaults: ProjectAutomationDefaults): number => {
  const board = withOverrideShape(defaults);
  return [
    board.executor_profile != null,
    ...AGENT_STAGES.map((stage) => board.agents.stages[stage] != null),
    board.label != null,
    board.ticketing.watcher_enabled != null,
    board.ticketing.jql_filter != null,
    board.ticketing.project_key != null,
    board.ticketing.assigned_jql != null,
    board.notifications.enabled != null,
    board.notifications.skip_global.length > 0,
    board.notifications.targets.length > 0,
    board.merge_requests.auto_assign_reviewers != null,
    Object.keys(board.merge_requests.reviewer_map ?? {}).length > 0,
    (board.merge_requests.reviewer_exclude_emails ?? []).length > 0,
    (board.merge_requests.bot_logins ?? []).length > 0,
    Object.keys(board.playbooks ?? {}).length > 0,
    board.autonomy != null,
    board.autonomy_review_iterations != null,
    board.deployed_retention !== 'inherit',
    board.cleanup_retention !== 'inherit',
    board.pattern_threshold != null,
    board.deploy_watch != null,
  ].filter(Boolean).length;
};

export function OrchestratorSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig, profiles } = useUserSystem();
  const descriptorsQuery = useIntegrationDescriptors();
  const notificationDescriptors =
    descriptorsQuery.data?.[NOTIFICATIONS_AREA] ?? [];
  const cardApiFields =
    descriptorsQuery.data?.[API_AREA]?.find(
      (descriptor) => descriptor.kind === CARD_API_KIND
    )?.fields ?? [];
  const agentDescriptors = useAgentDescriptors();
  const profileOptions = useProfileOptions(profiles);

  const [draft, setDraft] = useState<OrchestratorConfig>(() =>
    config?.orchestrator
      ? cloneDeep(config.orchestrator)
      : cloneDeep(DEFAULT_ORCHESTRATOR_CONFIG)
  );
  const [inAppGateDraft, setInAppGateDraft] = useState<InAppGate>(
    () => config?.approvals?.in_app_gate ?? 'lavish'
  );
  const [apiDraft, setApiDraft] = useState<ApiConfig>(() =>
    cloneDeep(config?.api ?? DEFAULT_API_CONFIG)
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!config) return;
    if (!dirty) {
      setDraft(
        config.orchestrator
          ? cloneDeep(config.orchestrator)
          : cloneDeep(DEFAULT_ORCHESTRATOR_CONFIG)
      );
      setInAppGateDraft(config.approvals?.in_app_gate ?? 'lavish');
      setApiDraft(cloneDeep(config.api ?? DEFAULT_API_CONFIG));
    }
  }, [config, dirty]);

  const hasUnsavedChanges = useMemo(() => {
    if (!config) return false;
    return (
      !isEqual(draft, config.orchestrator ?? DEFAULT_ORCHESTRATOR_CONFIG) ||
      inAppGateDraft !== (config.approvals?.in_app_gate ?? 'lavish') ||
      !isEqual(apiDraft, config.api ?? DEFAULT_API_CONFIG)
    );
  }, [draft, inAppGateDraft, apiDraft, config]);

  useEffect(() => {
    setContextDirty('orchestrator', hasUnsavedChanges);
    return () => setContextDirty('orchestrator', false);
  }, [hasUnsavedChanges, setContextDirty]);

  // Remote projects across every organization the user belongs to, so the
  // project picker offers names instead of hand-copied UUIDs.
  const { data: organizationsData } = useUserOrganizations();
  const { data: remoteProjects } = useQuery<RemoteProject[]>({
    queryKey: [
      'orchestrator-remote-projects',
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

  const { data: repos } = useQuery<Repo[]>({
    queryKey: ['orchestrator-repos'],
    queryFn: () => repoApi.list(),
  });

  const projectNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const project of remoteProjects ?? []) {
      map.set(project.id, project.name);
    }
    return map;
  }, [remoteProjects]);

  const configuredProjectIds = Object.keys(draft.project_defaults);
  const availableProjects = (remoteProjects ?? []).filter(
    (project) => !configuredProjectIds.includes(project.id)
  );

  const markDirty = useCallback(() => setDirty(true), []);

  const triStateOptions = useMemo(
    () => [
      {
        value: 'inherit' as TriState,
        label: t('settings.orchestrator.board.inherit'),
      },
      { value: 'on' as TriState, label: t('settings.orchestrator.board.on') },
      { value: 'off' as TriState, label: t('settings.orchestrator.board.off') },
    ],
    [t]
  );

  const updateProjectDefaults = useCallback(
    (
      projectId: string,
      patch: (current: ProjectAutomationDefaults) => ProjectAutomationDefaults
    ) => {
      markDirty();
      setDraft((prev) => ({
        ...prev,
        project_defaults: {
          ...prev.project_defaults,
          [projectId]: patch(
            withOverrideShape(
              prev.project_defaults[projectId] ?? cloneDeep(EMPTY_DEFAULTS)
            )
          ),
        },
      }));
    },
    [markDirty]
  );

  const addProject = useCallback(
    (projectId: string) => {
      updateProjectDefaults(projectId, (current) => current);
    },
    [updateProjectDefaults]
  );

  const removeProject = useCallback(
    (projectId: string) => {
      markDirty();
      setDraft((prev) => {
        const next = { ...prev.project_defaults };
        delete next[projectId];
        return { ...prev, project_defaults: next };
      });
    },
    [markDirty]
  );

  const toggleRepo = useCallback(
    (projectId: string, repoId: string, checked: boolean) => {
      updateProjectDefaults(projectId, (current) => ({
        ...current,
        repo_ids: checked
          ? [...current.repo_ids, repoId]
          : current.repo_ids.filter((id) => id !== repoId),
      }));
    },
    [updateProjectDefaults]
  );

  const pollIntervalSecs = Number(draft.poll_interval_secs);
  const projectsMissingRepos = configuredProjectIds.some(
    (projectId) =>
      (draft.project_defaults[projectId]?.repo_ids.length ?? 0) === 0
  );

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      await updateAndSaveConfig({
        orchestrator: draft,
        approvals: { ...config?.approvals, in_app_gate: inAppGateDraft },
        api: apiDraft,
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.orchestrator.save.error'));
      console.error('Error saving orchestrator config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    setDraft(
      config.orchestrator
        ? cloneDeep(config.orchestrator)
        : cloneDeep(DEFAULT_ORCHESTRATOR_CONFIG)
    );
    setInAppGateDraft(config.approvals?.in_app_gate ?? 'lavish');
    setApiDraft(cloneDeep(config.api ?? DEFAULT_API_CONFIG));
    setDirty(false);
  };

  // Puts the shipped values of the global fields on this page in the draft.
  // File-only settings and the automated projects are left as they are.
  const resetGlobalsToDefaults = () => {
    markDirty();
    setDraft((prev) => ({ ...prev, ...pickGlobalFields() }));
    setInAppGateDraft('lavish');
    setApiDraft(cloneDeep(DEFAULT_API_CONFIG));
  };
  const globalsAtDefaults =
    inAppGateDraft === 'lavish' &&
    isEqual(apiDraft, DEFAULT_API_CONFIG) &&
    isEqual(pickGlobalFields(draft), pickGlobalFields());

  if (loading) {
    return <SettingsLoading label={t('settings.general.loading')} />;
  }

  if (!config) {
    return (
      <div className="py-8">
        <SettingsAlert variant="error">
          {t('settings.general.loadError')}
        </SettingsAlert>
      </div>
    );
  }

  const autonomyOptions = [
    {
      value: 'inherit' as TriState,
      label: t('settings.orchestrator.board.inherit'),
    },
    {
      value: 'off' as TriState,
      label: t('settings.orchestrator.modes.chef.title'),
    },
    {
      value: 'on' as TriState,
      label: t('settings.orchestrator.modes.prep.title'),
    },
  ];

  return (
    <>
      {error && <SettingsAlert variant="error">{error}</SettingsAlert>}

      {success && (
        <SettingsAlert variant="success">
          {t('settings.orchestrator.save.success')}
        </SettingsAlert>
      )}

      <SettingsCard>
        <div className="flex flex-col gap-1.5">
          <span
            id="orchestrator-autonomy-mode"
            className="text-sm font-semibold text-high"
          >
            {t('settings.orchestrator.autonomyMode.label')}
          </span>
          <p className={settingsHelperClassName}>
            {t('settings.orchestrator.autonomyMode.helper')}
          </p>
        </div>
        <div
          role="radiogroup"
          aria-labelledby="orchestrator-autonomy-mode"
          className="grid gap-3 sm:grid-cols-2"
        >
          <ModeCard
            selected={draft.autonomy !== 'full'}
            title={t('settings.orchestrator.modes.chef.title')}
            chip={t('settings.orchestrator.modes.chef.default')}
            description={t('settings.orchestrator.modes.chef.description')}
            onSelect={() => {
              markDirty();
              setDraft((prev) => ({ ...prev, autonomy: 'off' }));
            }}
          />
          <ModeCard
            selected={draft.autonomy === 'full'}
            title={t('settings.orchestrator.modes.prep.title')}
            description={t('settings.orchestrator.modes.prep.description')}
            onSelect={() => {
              markDirty();
              setDraft((prev) => ({ ...prev, autonomy: 'full' }));
            }}
          />
        </div>
      </SettingsCard>

      <SettingsCard>
        <SettingsField
          label={t('settings.orchestrator.approvals.title')}
          description={t('settings.orchestrator.approvals.description')}
        >
          <SettingsSelect<InAppGate>
            value={inAppGateDraft}
            options={[
              {
                value: 'lavish',
                label: t('settings.orchestrator.approvals.options.lavish'),
              },
              {
                value: 'none',
                label: t('settings.orchestrator.approvals.options.none'),
              },
            ]}
            onChange={(value) => {
              markDirty();
              setInAppGateDraft(value);
            }}
            className="max-w-[540px]"
          />
        </SettingsField>

        <div className="grid gap-4 sm:grid-cols-2">
          <SettingsField
            label={t('settings.orchestrator.reviewLoopCap.label')}
            description={t('settings.orchestrator.reviewLoopCap.helper')}
          >
            <SettingsInput
              value={String(draft.autonomy_review_iterations)}
              onChange={(value) => {
                markDirty();
                const parsed = parseInt(value, 10);
                setDraft((prev) => ({
                  ...prev,
                  autonomy_review_iterations: Number.isNaN(parsed) ? 0 : parsed,
                }));
              }}
              placeholder="3"
              error={draft.autonomy_review_iterations < 1}
              className="mt-auto max-w-[120px]"
            />
          </SettingsField>

          <SettingsField
            label={t('settings.orchestrator.maxLaunchesPerTick.label')}
            description={t('settings.orchestrator.maxLaunchesPerTick.helper')}
          >
            <SettingsInput
              value={String(draft.max_launches_per_tick)}
              onChange={(value) => {
                markDirty();
                const parsed = parseInt(value, 10);
                setDraft((prev) => ({
                  ...prev,
                  max_launches_per_tick: Number.isNaN(parsed) ? 0 : parsed,
                }));
              }}
              placeholder="3"
              error={draft.max_launches_per_tick < 1}
              className="mt-auto max-w-[120px]"
            />
          </SettingsField>
        </div>

        {cardApiFields.map((field) => (
          <FieldInput
            key={field.key}
            id={`${API_AREA}-${CARD_API_KIND}-${field.key}`}
            field={field}
            value={apiDraft.cards[field.key as keyof ApiConfig['cards']]}
            onChange={(value) => {
              if (typeof value !== 'number' || value < 0) return;
              markDirty();
              setApiDraft((prev) => ({
                ...prev,
                cards: { ...prev.cards, [field.key]: Math.floor(value) },
              }));
            }}
          />
        ))}
      </SettingsCard>

      <SettingsCard
        title={t('settings.orchestrator.launches.title')}
        description={t('settings.orchestrator.launches.description')}
      >
        <SettingsCheckbox
          id="orchestrator-auto"
          label={t('settings.orchestrator.mode.label')}
          description={t('settings.orchestrator.mode.helper')}
          checked={draft.mode === 'auto'}
          onChange={(checked) => {
            markDirty();
            setDraft((prev) => ({ ...prev, mode: checked ? 'auto' : 'off' }));
          }}
        />

        <div className="grid gap-4 sm:grid-cols-2">
          <SettingsField
            label={t('settings.orchestrator.pollInterval.label')}
            description={t('settings.orchestrator.pollInterval.helper')}
          >
            <SettingsInput
              value={String(pollIntervalSecs)}
              onChange={(value) => {
                const parsed = parseInt(value, 10);
                markDirty();
                setDraft((prev) => ({
                  ...prev,
                  poll_interval_secs: (Number.isNaN(parsed)
                    ? 0
                    : parsed) as unknown as bigint,
                }));
              }}
              placeholder="15"
              error={pollIntervalSecs < 1}
              className="mt-auto max-w-[120px]"
            />
          </SettingsField>

          <SettingsField
            label={t('settings.orchestrator.patternThreshold.label')}
            description={t('settings.orchestrator.patternThreshold.helper')}
          >
            <SettingsInput
              value={String(draft.pattern_threshold)}
              onChange={(value) => {
                markDirty();
                const parsed = parseInt(value, 10);
                setDraft((prev) => ({
                  ...prev,
                  pattern_threshold: Number.isNaN(parsed) ? 0 : parsed,
                }));
              }}
              placeholder="5"
              error={draft.pattern_threshold < 1}
              className="mt-auto max-w-[120px]"
            />
          </SettingsField>
        </div>
      </SettingsCard>

      <SettingsCard
        title={t('settings.orchestrator.retention.title')}
        description={t('settings.orchestrator.retention.description')}
      >
        <SettingsField
          label={t('settings.orchestrator.closeRetention.label')}
          description={t('settings.orchestrator.closeRetention.helper')}
        >
          <SettingsInput
            value={
              draft.deployed_retention_days == null
                ? ''
                : String(draft.deployed_retention_days)
            }
            onChange={(value) => {
              markDirty();
              const parsed = parseInt(value, 10);
              setDraft((prev) => ({
                ...prev,
                deployed_retention_days:
                  value.trim() === '' || Number.isNaN(parsed) || parsed < 1
                    ? null
                    : parsed,
              }));
            }}
            placeholder={t(
              'settings.orchestrator.deployedRetention.placeholder'
            )}
            className="max-w-[240px]"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.orchestrator.deleteRetention.label')}
          description={t('settings.orchestrator.deleteRetention.helper')}
        >
          <SettingsInput
            value={
              draft.cleanup_retention_days == null
                ? ''
                : String(draft.cleanup_retention_days)
            }
            onChange={(value) => {
              markDirty();
              const parsed = parseInt(value, 10);
              setDraft((prev) => ({
                ...prev,
                cleanup_retention_days:
                  value.trim() === '' || Number.isNaN(parsed) || parsed < 1
                    ? null
                    : parsed,
              }));
            }}
            placeholder={t(
              'settings.orchestrator.cleanupRetention.placeholder'
            )}
            className="max-w-[240px]"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.orchestrator.workspaceRetention.label')}
          description={t('settings.orchestrator.workspaceRetention.helper')}
        >
          <SettingsInput
            value={
              draft.workspace_retention_days == null
                ? ''
                : String(draft.workspace_retention_days)
            }
            onChange={(value) => {
              markDirty();
              const parsed = parseInt(value, 10);
              setDraft((prev) => ({
                ...prev,
                workspace_retention_days:
                  value.trim() === '' || Number.isNaN(parsed) || parsed < 1
                    ? null
                    : parsed,
              }));
            }}
            placeholder={t(
              'settings.orchestrator.workspaceRetention.placeholder'
            )}
            className="max-w-[240px]"
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.orchestrator.projects.title')}
        description={t('settings.orchestrator.projects.description')}
      >
        {configuredProjectIds.length === 0 && (
          <p className="text-sm text-low">
            {t('settings.orchestrator.projects.empty')}
          </p>
        )}

        {configuredProjectIds.map((projectId) => {
          const defaults = withOverrideShape(
            draft.project_defaults[projectId] ?? EMPTY_DEFAULTS
          );
          // A board whose id the remote no longer knows can never be scanned;
          // the orchestrator just logs a 404 every tick. Say so here, where the
          // entry can be removed. Only once the project list has loaded —
          // otherwise every card accuses itself while the query is in flight.
          const projectMissing =
            (remoteProjects ?? []).length > 0 &&
            !projectNameById.has(projectId);
          const overrideCount = countOverrides(defaults);
          const retention = defaults.deployed_retention;
          const retentionMode =
            retention === 'inherit' || retention === 'off' ? retention : 'days';
          const cleanupRetention = defaults.cleanup_retention;
          const cleanupRetentionMode =
            cleanupRetention === 'inherit' || cleanupRetention === 'off'
              ? cleanupRetention
              : 'days';
          return (
            <div
              key={projectId}
              className="flex flex-col gap-3 rounded-lg border border-border px-4 py-3"
            >
              <div className="flex items-center justify-between gap-2">
                <span
                  className="truncate text-sm font-semibold text-high"
                  title={projectId}
                >
                  {projectNameById.get(projectId) ?? projectId}
                </span>
                <IconButton
                  icon={TrashIcon}
                  onClick={() => removeProject(projectId)}
                  aria-label={t('settings.orchestrator.projects.remove')}
                  title={t('settings.orchestrator.projects.remove')}
                  className="hover:bg-error-soft hover:text-error"
                />
              </div>

              {projectMissing && (
                <p className="text-[12.5px] text-error">
                  {t('settings.orchestrator.projects.notFound')}
                </p>
              )}

              <SettingsField
                label={t('settings.orchestrator.projects.reposLabel')}
                description={t('settings.orchestrator.projects.reposHelper')}
              >
                <div className="flex flex-col gap-2">
                  {(repos ?? []).map((repo) => (
                    <SettingsCheckbox
                      key={repo.id}
                      variant="checkbox"
                      id={`orchestrator-${projectId}-${repo.id}`}
                      label={repo.name}
                      checked={defaults.repo_ids.includes(repo.id)}
                      onChange={(checked) =>
                        toggleRepo(projectId, repo.id, checked)
                      }
                    />
                  ))}
                  {(repos ?? []).length === 0 && (
                    <p className="text-sm text-low">
                      {t('settings.orchestrator.projects.noRepos')}
                    </p>
                  )}
                </div>
              </SettingsField>

              <details className="group rounded-lg border border-border">
                <summary className="flex cursor-pointer list-none items-center gap-2 rounded-lg px-3 py-2 text-sm font-semibold text-high hover:bg-panel [&::-webkit-details-marker]:hidden">
                  <CaretRightIcon className="size-3.5 shrink-0 text-low transition-transform group-open:rotate-90" />
                  {t('settings.orchestrator.board.overridesTitle')}
                  <span className="ml-auto inline-flex h-[22px] items-center rounded-full bg-panel px-2 text-xs font-medium text-normal">
                    {overrideCount === 0
                      ? t('settings.orchestrator.board.overridesNone')
                      : t('settings.orchestrator.board.overridesCount', {
                          total: overrideCount,
                        })}
                  </span>
                </summary>
                <div className="flex flex-col gap-4 border-t border-divider px-3 pb-3 pt-3">
                  <p className={settingsHelperClassName}>
                    {t('settings.orchestrator.board.overridesHelper')}
                  </p>

                  <SettingsField
                    label={t('settings.orchestrator.board.labelLabel')}
                    description={t('settings.orchestrator.board.labelHelper')}
                  >
                    <SettingsInput
                      value={defaults.label ?? ''}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          label: orNull(value),
                        }))
                      }
                      placeholder="extensions"
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.agentDefaultLabel')}
                    description={t(
                      'settings.orchestrator.board.agentDefaultHelper'
                    )}
                  >
                    <ProfileSelect
                      value={defaults.executor_profile}
                      inherited={config.executor_profile}
                      inheritLabel={t(
                        'settings.orchestrator.board.agentInherit'
                      )}
                      options={profileOptions}
                      onChange={(profile) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          executor_profile: profile,
                        }))
                      }
                    />
                  </SettingsField>
                  <StageProfileFields
                    stages={defaults.agents.stages}
                    fallback={(stage) =>
                      config.agents?.stages?.[stage] ??
                      stageDefault(
                        stage,
                        defaults.executor_profile ?? config.executor_profile,
                        agentDescriptors
                      )
                    }
                    options={profileOptions}
                    inheritLabel={t('settings.orchestrator.board.agentInherit')}
                    onChange={(stages) =>
                      updateProjectDefaults(projectId, (current) => ({
                        ...current,
                        agents: { ...current.agents, stages },
                      }))
                    }
                  />

                  <SettingsField
                    label={t('settings.orchestrator.board.watcherLabel')}
                    description={t('settings.orchestrator.board.watcherHelper')}
                  >
                    <SettingsSelect<TriState>
                      value={toTriState(defaults.ticketing.watcher_enabled)}
                      options={triStateOptions}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          ticketing: {
                            ...current.ticketing,
                            watcher_enabled: fromTriState(value),
                          },
                        }))
                      }
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.pickupLabel')}
                    description={t('settings.orchestrator.board.jqlHelper')}
                  >
                    <SettingsInput
                      value={defaults.ticketing.jql_filter ?? ''}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          ticketing: {
                            ...current.ticketing,
                            jql_filter: orNull(value),
                          },
                        }))
                      }
                      placeholder="project = {project_key} AND component = Extension"
                      monospace
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.assignedLabel')}
                    description={t(
                      'settings.orchestrator.board.assignedJqlHelper'
                    )}
                  >
                    <SettingsInput
                      value={defaults.ticketing.assigned_jql ?? ''}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          ticketing: {
                            ...current.ticketing,
                            assigned_jql: orNull(value),
                          },
                        }))
                      }
                      placeholder={t(
                        'settings.orchestrator.board.inheritPlaceholder'
                      )}
                      monospace
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.trackerKeyLabel')}
                    description={t(
                      'settings.orchestrator.board.trackerKeyHelper'
                    )}
                  >
                    <SettingsInput
                      value={defaults.ticketing.project_key ?? ''}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          ticketing: {
                            ...current.ticketing,
                            project_key: orNull(value),
                          },
                        }))
                      }
                      placeholder={t(
                        'settings.orchestrator.board.inheritPlaceholder'
                      )}
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.notificationsLabel')}
                    description={t(
                      'settings.orchestrator.board.notificationsHelper'
                    )}
                  >
                    <SettingsSelect<TriState>
                      value={toTriState(defaults.notifications.enabled)}
                      options={triStateOptions}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          notifications: {
                            ...current.notifications,
                            enabled: fromTriState(value),
                          },
                        }))
                      }
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.skipGlobalLabel')}
                    description={t(
                      'settings.orchestrator.board.skipGlobalHelper'
                    )}
                  >
                    <div className="flex flex-wrap gap-x-5 gap-y-2">
                      {notificationDescriptors.map((descriptor) => {
                        const kind = descriptor.kind as NotifierKind;
                        return (
                          <SettingsCheckbox
                            key={kind}
                            variant="checkbox"
                            id={`skip-global-${projectId}-${kind}`}
                            label={descriptor.display_name}
                            checked={defaults.notifications.skip_global.includes(
                              kind
                            )}
                            onChange={(checked) =>
                              updateProjectDefaults(projectId, (current) => ({
                                ...current,
                                notifications: {
                                  ...current.notifications,
                                  skip_global: checked
                                    ? [
                                        ...current.notifications.skip_global,
                                        kind,
                                      ]
                                    : current.notifications.skip_global.filter(
                                        (skipped) => skipped !== kind
                                      ),
                                },
                              }))
                            }
                          />
                        );
                      })}
                    </div>
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.boardTargetsLabel')}
                    description={t(
                      'settings.orchestrator.board.boardTargetsHelper'
                    )}
                  >
                    <IntegrationSection<NotificationTarget>
                      area={NOTIFICATIONS_AREA}
                      descriptors={notificationDescriptors}
                      entries={defaults.notifications.targets}
                      multiple
                      onChange={(targets) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          notifications: {
                            ...current.notifications,
                            targets,
                          },
                        }))
                      }
                      renderExtra={(target, update) => (
                        <NotificationEventsField
                          target={target}
                          onChange={update}
                        />
                      )}
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.autoAssignLabel')}
                    description={t(
                      'settings.orchestrator.board.autoAssignHelper'
                    )}
                  >
                    <SettingsSelect<TriState>
                      value={toTriState(
                        defaults.merge_requests.auto_assign_reviewers
                      )}
                      options={triStateOptions}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          merge_requests: {
                            ...current.merge_requests,
                            auto_assign_reviewers: fromTriState(value),
                          },
                        }))
                      }
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.reviewerMapLabel')}
                    description={t(
                      'settings.orchestrator.board.reviewerMapHelper'
                    )}
                  >
                    <SettingsTextarea
                      value={mapToText(defaults.merge_requests.reviewer_map)}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          merge_requests: {
                            ...current.merge_requests,
                            reviewer_map: textToMap(value),
                          },
                        }))
                      }
                      rows={3}
                      placeholder={'dev@example.com=dev-login'}
                      monospace
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.excludeLabel')}
                    description={t('settings.orchestrator.board.excludeHelper')}
                  >
                    <SettingsTextarea
                      value={(
                        defaults.merge_requests.reviewer_exclude_emails ?? []
                      ).join('\n')}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          merge_requests: {
                            ...current.merge_requests,
                            reviewer_exclude_emails: textToList(value),
                          },
                        }))
                      }
                      rows={2}
                      placeholder="release-bot@example.com"
                      monospace
                    />
                  </SettingsField>

                  <SettingsField
                    label={t('settings.orchestrator.board.botLoginsLabel')}
                    description={t(
                      'settings.orchestrator.board.botLoginsHelper'
                    )}
                  >
                    <SettingsTextarea
                      value={(defaults.merge_requests.bot_logins ?? []).join(
                        '\n'
                      )}
                      onChange={(value) =>
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          merge_requests: {
                            ...current.merge_requests,
                            bot_logins: textToList(value),
                          },
                        }))
                      }
                      rows={2}
                      placeholder="ci-bot"
                      monospace
                    />
                  </SettingsField>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <SettingsField
                      label={t(
                        'settings.orchestrator.board.deployWatchJobLabel'
                      )}
                      description={t(
                        'settings.orchestrator.board.deployWatchJobHelper'
                      )}
                    >
                      <SettingsInput
                        value={defaults.deploy_watch?.job_name ?? ''}
                        onChange={(value) =>
                          updateProjectDefaults(projectId, (current) => ({
                            ...current,
                            deploy_watch: toDeployWatch(
                              value,
                              current.deploy_watch?.git_ref ?? ''
                            ),
                          }))
                        }
                        placeholder="deploy_staging"
                        monospace
                        className="mt-auto"
                      />
                    </SettingsField>

                    <SettingsField
                      label={t(
                        'settings.orchestrator.board.deployWatchRefLabel'
                      )}
                      description={t(
                        'settings.orchestrator.board.deployWatchRefHelper'
                      )}
                    >
                      <SettingsInput
                        value={defaults.deploy_watch?.git_ref ?? ''}
                        onChange={(value) =>
                          updateProjectDefaults(projectId, (current) => ({
                            ...current,
                            deploy_watch: toDeployWatch(
                              current.deploy_watch?.job_name ?? '',
                              value
                            ),
                          }))
                        }
                        placeholder="main"
                        monospace
                        className="mt-auto"
                      />
                    </SettingsField>
                  </div>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <SettingsField
                      label={t('settings.orchestrator.autonomyMode.label')}
                      description={t(
                        'settings.orchestrator.board.autonomyHelper'
                      )}
                    >
                      <div className="mt-auto">
                        <SettingsSelect<TriState>
                          value={toTriState(
                            defaults.autonomy == null
                              ? null
                              : defaults.autonomy === 'full'
                          )}
                          options={autonomyOptions}
                          onChange={(value) => {
                            const resolved = fromTriState(value);
                            updateProjectDefaults(projectId, (current) => ({
                              ...current,
                              autonomy:
                                resolved == null
                                  ? null
                                  : resolved
                                    ? 'full'
                                    : 'off',
                            }));
                          }}
                        />
                      </div>
                    </SettingsField>

                    <SettingsField
                      label={t('settings.orchestrator.reviewLoopCap.label')}
                      description={t(
                        'settings.orchestrator.board.blankInheritsHelper'
                      )}
                    >
                      <SettingsInput
                        value={
                          defaults.autonomy_review_iterations == null
                            ? ''
                            : String(defaults.autonomy_review_iterations)
                        }
                        onChange={(value) => {
                          const parsed = parseInt(value, 10);
                          updateProjectDefaults(projectId, (current) => ({
                            ...current,
                            autonomy_review_iterations:
                              value.trim() === '' ||
                              Number.isNaN(parsed) ||
                              parsed < 1
                                ? null
                                : parsed,
                          }));
                        }}
                        placeholder={t(
                          'settings.orchestrator.board.inheritPlaceholder'
                        )}
                        className="mt-auto"
                      />
                    </SettingsField>
                  </div>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <SettingsField
                      label={t('settings.orchestrator.closeRetention.label')}
                      description={t(
                        'settings.orchestrator.board.closeRetentionHelper'
                      )}
                    >
                      <div className="mt-auto flex flex-col gap-1.5">
                        <SettingsSelect<'inherit' | 'off' | 'days'>
                          value={retentionMode}
                          options={[
                            {
                              value: 'inherit',
                              label: t('settings.orchestrator.board.inherit'),
                            },
                            {
                              value: 'off',
                              label: t('settings.orchestrator.board.off'),
                            },
                            {
                              value: 'days',
                              label: t(
                                'settings.orchestrator.board.retentionDays'
                              ),
                            },
                          ]}
                          onChange={(mode) =>
                            updateProjectDefaults(projectId, (current) => ({
                              ...current,
                              deployed_retention:
                                mode === 'days' ? { days: 7 } : mode,
                            }))
                          }
                        />
                        {retentionMode === 'days' && (
                          <SettingsInput
                            value={String(
                              typeof retention === 'object' ? retention.days : 7
                            )}
                            onChange={(value) => {
                              const parsed = parseInt(value, 10);
                              updateProjectDefaults(projectId, (current) => ({
                                ...current,
                                deployed_retention: {
                                  days:
                                    Number.isNaN(parsed) || parsed < 1
                                      ? 1
                                      : parsed,
                                },
                              }));
                            }}
                            placeholder="7"
                          />
                        )}
                      </div>
                    </SettingsField>

                    <SettingsField
                      label={t('settings.orchestrator.deleteRetention.label')}
                      description={t(
                        'settings.orchestrator.board.deleteRetentionHelper'
                      )}
                    >
                      <div className="mt-auto flex flex-col gap-1.5">
                        <SettingsSelect<'inherit' | 'off' | 'days'>
                          value={cleanupRetentionMode}
                          options={[
                            {
                              value: 'inherit',
                              label: t('settings.orchestrator.board.inherit'),
                            },
                            {
                              value: 'off',
                              label: t('settings.orchestrator.board.off'),
                            },
                            {
                              value: 'days',
                              label: t(
                                'settings.orchestrator.board.retentionDays'
                              ),
                            },
                          ]}
                          onChange={(mode) =>
                            updateProjectDefaults(projectId, (current) => ({
                              ...current,
                              cleanup_retention:
                                mode === 'days' ? { days: 14 } : mode,
                            }))
                          }
                        />
                        {cleanupRetentionMode === 'days' && (
                          <SettingsInput
                            value={String(
                              typeof cleanupRetention === 'object'
                                ? cleanupRetention.days
                                : 14
                            )}
                            onChange={(value) => {
                              const parsed = parseInt(value, 10);
                              updateProjectDefaults(projectId, (current) => ({
                                ...current,
                                cleanup_retention: {
                                  days:
                                    Number.isNaN(parsed) || parsed < 1
                                      ? 1
                                      : parsed,
                                },
                              }));
                            }}
                            placeholder="14"
                          />
                        )}
                      </div>
                    </SettingsField>
                  </div>

                  <SettingsField
                    label={t('settings.orchestrator.patternThreshold.label')}
                    description={t(
                      'settings.orchestrator.board.blankInheritsHelper'
                    )}
                  >
                    <SettingsInput
                      value={
                        defaults.pattern_threshold == null
                          ? ''
                          : String(defaults.pattern_threshold)
                      }
                      onChange={(value) => {
                        const parsed = parseInt(value, 10);
                        updateProjectDefaults(projectId, (current) => ({
                          ...current,
                          pattern_threshold:
                            value.trim() === '' ||
                            Number.isNaN(parsed) ||
                            parsed < 1
                              ? null
                              : parsed,
                        }));
                      }}
                      placeholder={t(
                        'settings.orchestrator.board.inheritPlaceholder'
                      )}
                      className="max-w-[240px]"
                    />
                  </SettingsField>

                  <p className={settingsHelperClassName}>
                    {t('settings.orchestrator.board.playbooksHint')}
                  </p>
                </div>
              </details>
            </div>
          );
        })}

        {availableProjects.length > 0 && (
          <SettingsField
            label={t('settings.orchestrator.projects.addLabel')}
            description={t('settings.orchestrator.projects.addHelper')}
          >
            <SettingsSelect
              value={undefined}
              options={availableProjects.map((project) => ({
                value: project.id,
                label: project.name,
              }))}
              onChange={(projectId) => addProject(projectId)}
              placeholder={t('settings.orchestrator.projects.addPlaceholder')}
              className="max-w-[440px]"
            />
          </SettingsField>
        )}

        {availableProjects.length === 0 &&
          configuredProjectIds.length === 0 && (
            <PrimaryButton
              value={t('settings.orchestrator.projects.boardHint')}
              disabled
              variant="default"
              className="self-start"
            />
          )}
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        saveDisabled={
          pollIntervalSecs < 1 ||
          projectsMissingRepos ||
          draft.autonomy_review_iterations < 1
        }
        onSave={handleSave}
        onDiscard={handleDiscard}
        onReset={resetGlobalsToDefaults}
        resetDisabled={globalsAtDefaults}
      />
    </>
  );
}

/** One autonomy mode as a selectable card; the pair forms a radio group. */
function ModeCard({
  selected,
  title,
  chip,
  description,
  onSelect,
}: {
  selected: boolean;
  title: string;
  chip?: string;
  description: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn(
        'flex items-start gap-3 rounded-lg border bg-secondary px-3.5 py-3 text-left transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        selected
          ? 'border-brand ring-1 ring-brand'
          : 'border-border hover:bg-panel'
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          'mt-0.5 size-4 shrink-0 rounded-full bg-secondary',
          selected ? 'border-[5px] border-brand' : 'border-2 border-low/50'
        )}
      />
      <span className="flex min-w-0 flex-col gap-1">
        <span className="flex items-center gap-1.5 text-sm font-bold text-high">
          {title}
          {chip && (
            <span className="inline-flex h-[22px] items-center rounded-full bg-panel px-2 text-xs font-medium text-normal">
              {chip}
            </span>
          )}
        </span>
        <span className={settingsHelperClassName}>{description}</span>
      </span>
    </button>
  );
}
