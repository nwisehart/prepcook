import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { SpinnerIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type { StatusMapping, TrackerConfig, TrackerKind } from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { remoteProjectsApi, trackerApi } from '@/shared/lib/api';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsSaveBar,
  SettingsSelect,
  SettingsTextarea,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';

// `poll_secs` is typed bigint (u64 via ts-rs) but is a plain JSON number at
// runtime; keep it a number so config serialization stays valid.
const EMPTY_TRACKER_CONFIG: TrackerConfig = {
  kind: 'jira',
  site: null,
  email: null,
  api_token: null,
  project_key: null,
  import_filter: null,
  pickup_filter: null,
  assigned_filter: null,
  poll_secs: 45 as unknown as bigint,
  enabled: false,
  project_id: null,
  type_mapping: {},
  blocker_priorities: ['P0', 'P1'],
  status_mapping: {
    planning: null,
    executing: null,
    reviewing: null,
    pushed_mr: null,
    triaged: null,
    done: null,
    shipped: ['Merged', 'Deployed to Production', 'Done'],
  },
  approvers: {},
  bot_identity: null,
};

const TRACKER_KINDS: { kind: TrackerKind; supported: boolean }[] = [
  { kind: 'jira', supported: true },
  { kind: 'github_issues', supported: false },
  { kind: 'linear', supported: false },
];

type StageStatusKey = Exclude<keyof StatusMapping, 'shipped'>;

const STAGE_STATUS_KEYS: StageStatusKey[] = [
  'planning',
  'executing',
  'reviewing',
  'pushed_mr',
  'triaged',
  'done',
];

const JIRA_API_TOKENS_URL =
  'https://id.atlassian.com/manage-profile/security/api-tokens';

const orNull = (value: string) => (value.trim() === '' ? null : value);

const toCommaList = (list: string[]) => list.join(', ');

const fromCommaList = (text: string) =>
  text
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item !== '');

export function TrackerSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();

  const saved = useMemo(
    () =>
      config?.tracker
        ? cloneDeep(config.tracker)
        : cloneDeep(EMPTY_TRACKER_CONFIG),
    [config]
  );

  const [draft, setDraft] = useState<TrackerConfig>(saved);
  // The comma lists are edited as text so a trailing comma survives typing;
  // the parsed list is what lands in the draft.
  const [shippedText, setShippedText] = useState(() =>
    toCommaList(saved.status_mapping.shipped)
  );
  const [blockerText, setBlockerText] = useState(() =>
    toCommaList(saved.blocker_priorities)
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const resetTo = useCallback((next: TrackerConfig) => {
    setDraft(cloneDeep(next));
    setShippedText(toCommaList(next.status_mapping.shipped));
    setBlockerText(toCommaList(next.blocker_priorities));
  }, []);

  useEffect(() => {
    if (!config) return;
    if (!dirty) {
      resetTo(saved);
    }
  }, [config, dirty, saved, resetTo]);

  const hasUnsavedChanges = useMemo(() => {
    if (!config) return false;
    return !isEqual(draft, saved);
  }, [draft, saved, config]);

  // Sync dirty state to context for unsaved changes confirmation
  useEffect(() => {
    setContextDirty('tracker', hasUnsavedChanges);
    return () => setContextDirty('tracker', false);
  }, [hasUnsavedChanges, setContextDirty]);

  const update = useCallback((patch: Partial<TrackerConfig>) => {
    setDirty(true);
    setDraft((prev) => ({ ...prev, ...patch }));
  }, []);

  const updateStatus = useCallback((patch: Partial<StatusMapping>) => {
    setDirty(true);
    setDraft((prev) => ({
      ...prev,
      status_mapping: { ...prev.status_mapping, ...patch },
    }));
  }, []);

  // Remote projects across every organization the user belongs to, so the
  // board picker offers names instead of hand-copied UUIDs.
  const { data: organizationsData } = useUserOrganizations();
  const { data: remoteProjects } = useQuery<RemoteProject[]>({
    queryKey: [
      'tracker-remote-projects',
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

  const [assignable, setAssignable] = useState<Record<string, string> | null>(
    null
  );
  const [loadingUsers, setLoadingUsers] = useState(false);
  const [usersError, setUsersError] = useState<string | null>(null);

  // Saved approvers show without an import, so an instance that cannot reach
  // the tracker still displays who was chosen — and can still untick them,
  // which is the operation you most want working on a bad day.
  const approverRows = useMemo(() => {
    const merged: Record<string, string> = {};
    for (const [id, name] of Object.entries(draft.approvers)) {
      if (name) merged[id] = name;
    }
    for (const [id, name] of Object.entries(assignable ?? {})) {
      merged[id] = name;
    }
    return Object.entries(merged).sort(([, a], [, b]) => a.localeCompare(b));
  }, [assignable, draft.approvers]);

  const selectedCount = useMemo(
    () => Object.values(draft.approvers).filter(Boolean).length,
    [draft.approvers]
  );

  const loadAssignable = useCallback(async () => {
    setLoadingUsers(true);
    setUsersError(null);
    try {
      setAssignable(await trackerApi.assignableUsers());
    } catch (err) {
      setUsersError(t('settings.tracker.approvers.error'));
      console.error('Error loading assignable tracker users:', err);
    } finally {
      setLoadingUsers(false);
    }
  }, [t]);

  const toggleApprover = useCallback(
    (accountId: string, displayName: string, checked: boolean) => {
      setDirty(true);
      setDraft((prev) => {
        const next = { ...prev.approvers };
        if (checked) {
          next[accountId] = displayName;
        } else {
          delete next[accountId];
        }
        return { ...prev, approvers: next };
      });
    },
    []
  );

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(false);

    try {
      await updateAndSaveConfig({ tracker: draft });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.tracker.save.error'));
      console.error('Error saving tracker config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    resetTo(saved);
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

  const pollSecs = Number(draft.poll_secs);

  return (
    <>
      {error && (
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {error}
        </div>
      )}

      {success && (
        <div className="bg-success/10 border border-success/50 rounded-sm p-4 text-success font-medium">
          {t('settings.tracker.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.tracker.kind.title')}
        description={t('settings.tracker.kind.description')}
      >
        <div className="flex flex-wrap gap-2" role="radiogroup">
          {TRACKER_KINDS.map(({ kind, supported }) => {
            const selected = draft.kind === kind;
            return (
              <button
                key={kind}
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={!supported}
                onClick={() => update({ kind })}
                className={`text-sm px-3 py-1.5 rounded-sm border text-left disabled:opacity-50 disabled:cursor-not-allowed ${
                  selected
                    ? 'border-brand bg-brand/10'
                    : 'border-border hover:bg-secondary'
                }`}
              >
                <span className="block">
                  {t(`settings.tracker.kind.options.${kind}`)}
                </span>
                {!supported && (
                  <span className="block text-xs text-low">
                    {t('settings.tracker.kind.notSupported')}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </SettingsCard>

      <SettingsCard
        title={t('settings.tracker.connection.title')}
        description={t('settings.tracker.connection.description')}
      >
        <SettingsField
          label={t('settings.tracker.site.label')}
          description={t('settings.tracker.site.helper')}
        >
          <SettingsInput
            value={draft.site ?? ''}
            onChange={(value) => update({ site: orNull(value) })}
            placeholder="example.atlassian.net"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.email.label')}
          description={t('settings.tracker.email.helper')}
        >
          <SettingsInput
            value={draft.email ?? ''}
            onChange={(value) => update({ email: orNull(value) })}
            placeholder={t('settings.tracker.email.placeholder')}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.apiToken.label')}
          description={
            <>
              {t('settings.tracker.apiToken.helper')}{' '}
              <a
                href={JIRA_API_TOKENS_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="text-brand hover:underline"
              >
                {t('settings.tracker.apiToken.helpLink')}
              </a>
            </>
          }
        >
          <SettingsInput
            type="password"
            value={draft.api_token ?? ''}
            onChange={(value) => update({ api_token: orNull(value) })}
            placeholder={t('settings.tracker.apiToken.placeholder')}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.botIdentity.label')}
          description={t('settings.tracker.botIdentity.helper')}
        >
          <SettingsInput
            value={draft.bot_identity ?? ''}
            onChange={(value) => update({ bot_identity: orNull(value) })}
            placeholder={t('settings.tracker.botIdentity.placeholder')}
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.tracker.import.title')}
        description={t('settings.tracker.import.description')}
      >
        <SettingsField
          label={t('settings.tracker.projectKey.label')}
          description={t('settings.tracker.projectKey.helper')}
        >
          <SettingsInput
            value={draft.project_key ?? ''}
            onChange={(value) => update({ project_key: orNull(value) })}
            placeholder={t('settings.tracker.projectKey.placeholder')}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.importFilter.label')}
          description={t('settings.tracker.importFilter.helper')}
        >
          <SettingsTextarea
            value={draft.import_filter ?? ''}
            onChange={(value) => update({ import_filter: orNull(value) })}
            placeholder={t('settings.tracker.importFilter.placeholder')}
            rows={3}
            monospace
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.tracker.watcher.title')}
        description={t('settings.tracker.watcher.description')}
      >
        <SettingsCheckbox
          id="tracker-watcher-enabled"
          label={t('settings.tracker.watcher.enabledLabel')}
          description={t('settings.tracker.watcher.enabledHelper')}
          checked={draft.enabled}
          onChange={(checked) => update({ enabled: checked })}
        />

        <SettingsField
          label={t('settings.tracker.watcher.projectLabel')}
          description={t('settings.tracker.watcher.projectHelper')}
        >
          <SettingsSelect
            value={draft.project_id ?? undefined}
            options={(remoteProjects ?? []).map((project) => ({
              value: project.id,
              label: project.name,
            }))}
            onChange={(projectId) => update({ project_id: projectId })}
            placeholder={t('settings.tracker.watcher.projectPlaceholder')}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.watcher.pickupLabel')}
          description={t('settings.tracker.watcher.pickupHelper')}
        >
          <SettingsInput
            value={draft.pickup_filter ?? ''}
            onChange={(value) => update({ pickup_filter: orNull(value) })}
            placeholder="project = {project_key} AND priority in (P0, P1) AND issuetype = Bug"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.watcher.assignedLabel')}
          description={t('settings.tracker.watcher.assignedHelper')}
        >
          <SettingsInput
            value={draft.assigned_filter ?? ''}
            onChange={(value) => update({ assigned_filter: orNull(value) })}
            placeholder="project = {project_key} AND assignee = currentUser()"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.watcher.pollLabel')}
          description={t('settings.tracker.watcher.pollHelper')}
        >
          <SettingsInput
            value={String(pollSecs)}
            onChange={(value) => {
              const parsed = parseInt(value, 10);
              update({
                poll_secs: (Number.isNaN(parsed)
                  ? 0
                  : parsed) as unknown as bigint,
              });
            }}
            placeholder="45"
            error={pollSecs < 1}
          />
        </SettingsField>

        <p className="text-sm text-low">
          {t('settings.tracker.watcher.boardsHint')}
        </p>
      </SettingsCard>

      <SettingsCard
        title={t('settings.tracker.workflow.title')}
        description={t('settings.tracker.workflow.description')}
      >
        <SettingsField
          label={t('settings.tracker.workflow.statusLabel')}
          description={t('settings.tracker.workflow.statusHelper')}
        >
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-low">
                <th className="font-normal pb-2 pr-3">
                  {t('settings.tracker.workflow.stageColumn')}
                </th>
                <th className="font-normal pb-2">
                  {t('settings.tracker.workflow.statusColumn')}
                </th>
              </tr>
            </thead>
            <tbody>
              {STAGE_STATUS_KEYS.map((key) => (
                <tr key={key}>
                  <td className="pr-3 py-1 whitespace-nowrap">
                    {t(`settings.tracker.workflow.stages.${key}`)}
                  </td>
                  <td className="py-1">
                    <SettingsInput
                      value={draft.status_mapping[key] ?? ''}
                      onChange={(value) =>
                        updateStatus({ [key]: orNull(value) })
                      }
                      placeholder={
                        key === 'planning'
                          ? 'In Progress'
                          : t('settings.tracker.workflow.unmapped')
                      }
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.workflow.shippedLabel')}
          description={t('settings.tracker.workflow.shippedHelper')}
        >
          <SettingsInput
            value={shippedText}
            onChange={(value) => {
              setShippedText(value);
              updateStatus({ shipped: fromCommaList(value) });
            }}
            placeholder="Merged, Deployed to Production, Done"
          />
        </SettingsField>

        <SettingsField
          label={t('settings.tracker.workflow.blockerLabel')}
          description={t('settings.tracker.workflow.blockerHelper')}
        >
          <SettingsInput
            value={blockerText}
            onChange={(value) => {
              setBlockerText(value);
              update({ blocker_priorities: fromCommaList(value) });
            }}
            placeholder="P0, P1"
          />
        </SettingsField>
      </SettingsCard>

      <SettingsCard
        title={t('settings.tracker.approvers.title')}
        description={t('settings.tracker.approvers.description')}
      >
        <SettingsField
          label={t('settings.tracker.approvers.label')}
          description={t('settings.tracker.approvers.helper')}
        >
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={loadAssignable}
                disabled={loadingUsers}
                className="text-sm px-3 py-1.5 rounded-sm border border-border hover:bg-secondary disabled:opacity-50"
              >
                {loadingUsers
                  ? t('settings.tracker.approvers.loading')
                  : assignable
                    ? t('settings.tracker.approvers.refresh')
                    : t('settings.tracker.approvers.import')}
              </button>
              <span className="text-sm text-low">
                {t('settings.tracker.approvers.selected', {
                  count: selectedCount,
                })}
              </span>
            </div>

            {usersError && <p className="text-sm text-error">{usersError}</p>}

            {approverRows.length === 0 ? (
              <p className="text-sm text-low">
                {t('settings.tracker.approvers.empty')}
              </p>
            ) : (
              <div className="space-y-2 max-h-72 overflow-y-auto rounded-sm border border-border p-3">
                {approverRows.map(([accountId, name]) => (
                  <SettingsCheckbox
                    key={accountId}
                    id={`approver-${accountId}`}
                    label={name}
                    checked={Boolean(draft.approvers[accountId])}
                    onChange={(checked) =>
                      toggleApprover(accountId, name, checked)
                    }
                  />
                ))}
              </div>
            )}
          </div>
        </SettingsField>
      </SettingsCard>

      <SettingsSaveBar
        show={hasUnsavedChanges}
        saving={saving}
        saveDisabled={pollSecs < 1}
        onSave={handleSave}
        onDiscard={handleDiscard}
      />
    </>
  );
}
