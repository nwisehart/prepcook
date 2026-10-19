import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { useQuery } from '@tanstack/react-query';
import type {
  Descriptor,
  StatusMapping,
  TicketingMode,
  TrackerConfig,
  TrackerKind,
} from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { remoteProjectsApi, trackerApi } from '@/shared/lib/api';
import { cn } from '@/shared/lib/utils';
import { Button } from '@prepcook/ui/components/Button';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsInput,
  SettingsLoading,
  SettingsSaveBar,
  SettingsSelect,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import {
  IntegrationSection,
  useIntegrationDescriptors,
  type IntegrationEntry,
} from './IntegrationSection';

export const TICKETING_AREA = 'ticketing';

// `poll_secs` is typed bigint (u64 via ts-rs) but is a plain JSON number at
// runtime; keep it a number so config serialization stays valid.
const EMPTY_TICKETING_CONFIG: TrackerConfig = {
  mode: 'tickets',
  kind: 'jira',
  site: null,
  email: null,
  api_token: null,
  project_key: null,
  repo: null,
  pagerduty_api_token: null,
  service_ids: null,
  user_id: null,
  linear_api_key: null,
  team_key: null,
  incidents: { resolve_on_triage: false },
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

const MODES: TicketingMode[] = ['tickets', 'incidents'];

/** The incident tools; every other kind is a ticket tracker. */
const INCIDENT_KINDS: TrackerKind[] = ['pagerduty'];

const modeOf = (kind: string): TicketingMode =>
  INCIDENT_KINDS.includes(kind as TrackerKind) ? 'incidents' : 'tickets';

type StageStatusKey = Exclude<keyof StatusMapping, 'shipped'>;

const STAGE_STATUS_KEYS: StageStatusKey[] = [
  'planning',
  'executing',
  'reviewing',
  'pushed_mr',
  'triaged',
  'done',
];

const orNull = (value: string) => (value.trim() === '' ? null : value);

const toCommaList = (list: string[]) => list.join(', ');

const fromCommaList = (text: string) =>
  text
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item !== '');

type TicketingEntry = IntegrationEntry;

/** The section's fields one provider's descriptor names. */
const fieldKeys = (descriptor: Descriptor | undefined): string[] =>
  descriptor?.fields.map((field) => field.key) ?? [];

/** Whether any field the provider reads is set. */
export const hasProviderSettings = (
  draft: TrackerConfig,
  descriptor: Descriptor | undefined
) =>
  fieldKeys(descriptor).some((key) => {
    const value = (draft as unknown as Record<string, unknown>)[key];
    return value != null && value !== '';
  });

export function TicketingSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();
  const descriptors = useIntegrationDescriptors();
  const allDescriptors = useMemo(
    () => descriptors.data?.[TICKETING_AREA] ?? [],
    [descriptors.data]
  );

  const saved = useMemo(
    () =>
      config?.ticketing
        ? cloneDeep(config.ticketing)
        : cloneDeep(EMPTY_TICKETING_CONFIG),
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
  // Whether the provider block is shown. Removing it clears the provider's
  // own fields, which is what leaves no tracker configured.
  const [removed, setRemoved] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const descriptorFor = useCallback(
    (kind: string) => allDescriptors.find((d) => d.kind === kind),
    [allDescriptors]
  );

  const resetTo = useCallback(
    (next: TrackerConfig) => {
      setDraft(cloneDeep(next));
      setShippedText(toCommaList(next.status_mapping.shipped));
      setBlockerText(toCommaList(next.blocker_priorities));
      setRemoved(!hasProviderSettings(next, descriptorFor(next.kind)));
    },
    [descriptorFor]
  );

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
    setContextDirty(TICKETING_AREA, hasUnsavedChanges);
    return () => setContextDirty(TICKETING_AREA, false);
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

  const modeDescriptors = useMemo(
    () => allDescriptors.filter((d) => modeOf(d.kind) === draft.mode),
    [allDescriptors, draft.mode]
  );

  // The configured provider as the generic section edits it: its kind and
  // the fields its descriptor names, read from (and written back to) the
  // flat ticketing section.
  const entries: TicketingEntry[] = useMemo(() => {
    if (removed || modeOf(draft.kind) !== draft.mode) return [];
    const record = draft as unknown as Record<string, unknown>;
    const entry: TicketingEntry = { kind: draft.kind };
    for (const key of fieldKeys(descriptorFor(draft.kind))) {
      entry[key] = record[key] ?? null;
    }
    return [entry];
  }, [draft, removed, descriptorFor]);

  const onEntriesChange = (next: TicketingEntry[]) => {
    setDirty(true);
    const [entry] = next;
    if (!entry) {
      // Removed: clear what that provider read, so nothing is configured.
      const cleared: Record<string, null> = {};
      for (const key of fieldKeys(descriptorFor(draft.kind))) {
        cleared[key] = null;
      }
      setDraft((prev) => ({ ...prev, ...cleared }));
      setRemoved(true);
      return;
    }
    const { kind, ...fields } = entry;
    setDraft((prev) => ({
      ...prev,
      ...(fields as Partial<TrackerConfig>),
      kind: kind as TrackerKind,
    }));
    setRemoved(false);
  };

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
      setUsersError(t('settings.ticketing.approvers.error'));
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
      await updateAndSaveConfig({ ticketing: draft });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.ticketing.save.error'));
      console.error('Error saving ticketing config:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleDiscard = () => {
    if (!config) return;
    resetTo(saved);
    setDirty(false);
  };

  if (loading || descriptors.isLoading) {
    return <SettingsLoading label={t('settings.general.loading')} />;
  }

  if (!config || descriptors.isError) {
    return (
      <div className="py-8">
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  const pollSecs = Number(draft.poll_secs);
  const incidents = draft.mode === 'incidents';

  return (
    <>
      {error && (
        <div className="rounded-lg border border-error/30 bg-error-soft px-4 py-3 text-sm text-error">
          {error}
        </div>
      )}

      {success && (
        <div className="rounded-lg border border-success/30 bg-success-soft px-4 py-3 text-sm font-medium text-success">
          {t('settings.ticketing.save.success')}
        </div>
      )}

      <SettingsCard
        title={t('settings.ticketing.source.title')}
        description={t('settings.ticketing.source.description')}
      >
        <div className="grid gap-3 sm:grid-cols-2" role="radiogroup">
          {MODES.map((mode) => {
            const selected = draft.mode === mode;
            return (
              <button
                key={mode}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => update({ mode })}
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
                    selected
                      ? 'border-[5px] border-brand'
                      : 'border-2 border-low/50'
                  )}
                />
                <span className="flex min-w-0 flex-col gap-1">
                  <span className="text-sm font-bold text-high">
                    {t(`settings.ticketing.mode.${mode}.label`)}
                  </span>
                  <span className="text-[12.5px] leading-[1.45] text-low">
                    {t(`settings.ticketing.mode.${mode}.helper`)}
                  </span>
                </span>
              </button>
            );
          })}
        </div>

        <IntegrationSection<TicketingEntry>
          area={TICKETING_AREA}
          descriptors={modeDescriptors}
          entries={entries}
          onChange={onEntriesChange}
          renderExtra={(entry) =>
            modeOf(entry.kind) === 'incidents' ? (
              <SettingsCheckbox
                id="ticketing-resolve-on-triage"
                label={t('settings.ticketing.incidents.resolveLabel')}
                description={t('settings.ticketing.incidents.resolveHelper')}
                checked={draft.incidents.resolve_on_triage}
                onChange={(checked) =>
                  update({ incidents: { resolve_on_triage: checked } })
                }
              />
            ) : null
          }
        />
      </SettingsCard>

      <SettingsCard
        title={t('settings.ticketing.watcher.title')}
        description={t('settings.ticketing.watcher.description')}
      >
        <SettingsCheckbox
          id="tracker-watcher-enabled"
          label={t('settings.ticketing.watcher.enabledLabel')}
          description={t('settings.ticketing.watcher.enabledHelper')}
          checked={draft.enabled}
          onChange={(checked) => update({ enabled: checked })}
        />

        <SettingsField
          label={t('settings.ticketing.watcher.projectLabel')}
          description={t('settings.ticketing.watcher.projectHelper')}
        >
          <SettingsSelect
            value={draft.project_id ?? undefined}
            options={(remoteProjects ?? []).map((project) => ({
              value: project.id,
              label: project.name,
            }))}
            onChange={(projectId) => update({ project_id: projectId })}
            placeholder={t('settings.ticketing.watcher.projectPlaceholder')}
          />
        </SettingsField>

        <SettingsField
          label={t('settings.ticketing.watcher.pollLabel')}
          description={t('settings.ticketing.watcher.pollHelper')}
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
            className="max-w-[120px]"
          />
        </SettingsField>

        <p className="text-[12.5px] leading-[1.45] text-low">
          {t('settings.ticketing.watcher.boardsHint')}
        </p>
      </SettingsCard>

      <SettingsCard
        title={t('settings.ticketing.workflow.title')}
        description={
          incidents
            ? t('settings.ticketing.workflow.incidentsDescription')
            : t('settings.ticketing.workflow.description')
        }
      >
        {!incidents && (
          <>
            <SettingsField
              label={t('settings.ticketing.workflow.statusLabel')}
              description={t('settings.ticketing.workflow.statusHelper')}
            >
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-left text-xs text-low">
                    <th className="pb-1.5 pr-3 font-semibold">
                      {t('settings.ticketing.workflow.stageColumn')}
                    </th>
                    <th className="pb-1.5 font-semibold">
                      {t('settings.ticketing.workflow.statusColumn')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {STAGE_STATUS_KEYS.map((key) => (
                    <tr key={key}>
                      <td className="whitespace-nowrap py-1 pr-3 font-medium text-high">
                        {t(`settings.ticketing.workflow.stages.${key}`)}
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
                              : t('settings.ticketing.workflow.unmapped')
                          }
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </SettingsField>

            <SettingsField
              label={t('settings.ticketing.workflow.shippedLabel')}
              description={t('settings.ticketing.workflow.shippedHelper')}
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
          </>
        )}

        <SettingsField
          label={t('settings.ticketing.workflow.blockerLabel')}
          description={t('settings.ticketing.workflow.blockerHelper')}
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
        title={t('settings.ticketing.approvers.title')}
        description={t('settings.ticketing.approvers.description')}
      >
        <SettingsField
          label={t('settings.ticketing.approvers.label')}
          description={t('settings.ticketing.approvers.helper')}
        >
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-3">
              <Button
                type="button"
                variant="outline"
                onClick={loadAssignable}
                disabled={loadingUsers}
              >
                {loadingUsers
                  ? t('settings.ticketing.approvers.loading')
                  : assignable
                    ? t('settings.ticketing.approvers.refresh')
                    : t('settings.ticketing.approvers.import')}
              </Button>
              <span className="text-[12.5px] text-low">
                {t('settings.ticketing.approvers.selected', {
                  count: selectedCount,
                })}
              </span>
            </div>

            {usersError && (
              <p className="text-[12.5px] text-error">{usersError}</p>
            )}

            {approverRows.length === 0 ? (
              <p className="text-sm text-low">
                {t('settings.ticketing.approvers.empty')}
              </p>
            ) : (
              <div className="flex max-h-72 flex-col gap-2 overflow-y-auto rounded-lg border border-border p-3">
                {approverRows.map(([accountId, name]) => (
                  <SettingsCheckbox
                    key={accountId}
                    variant="checkbox"
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
