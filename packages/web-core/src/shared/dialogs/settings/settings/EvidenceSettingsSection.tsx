import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { cloneDeep, isEqual } from 'lodash';
import { SpinnerIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type {
  EvidenceKind,
  EvidenceSourceConfig,
  ProjectEvidenceOverride,
} from 'shared/types';
import type { Project as RemoteProject } from 'shared/remote-types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { remoteProjectsApi } from '@/shared/lib/api';
import {
  SettingsCard,
  SettingsCheckbox,
  SettingsField,
  SettingsSaveBar,
  SettingsSelect,
} from './SettingsComponents';
import { useSettingsDirty } from './SettingsDirtyContext';
import {
  IntegrationSection,
  useIntegrationDescriptors,
} from './IntegrationSection';

export const EVIDENCE_AREA = 'evidence';

const EMPTY_OVERRIDE: ProjectEvidenceOverride = {
  enabled: null,
  skip_global: [],
  sources: [],
};

/** A board's switch: inherit the global sources, or turn all of them off. */
type BoardSwitch = 'inherit' | 'off';

interface Draft {
  sources: EvidenceSourceConfig[];
  /** Board id → its evidence settings. */
  boards: Record<string, ProjectEvidenceOverride>;
}

/**
 * Where failures leave traces outside the reproduction, rendered from the
 * evidence providers' descriptors: the sources every board uses, and each
 * board's own (on top of, or instead of, the global ones).
 */
export function EvidenceSettingsSection() {
  const { t } = useTranslation(['settings', 'common']);
  const { setDirty: setContextDirty } = useSettingsDirty();
  const { config, loading, updateAndSaveConfig } = useUserSystem();
  const descriptorsQuery = useIntegrationDescriptors();
  const descriptors = useMemo(
    () => descriptorsQuery.data?.[EVIDENCE_AREA] ?? [],
    [descriptorsQuery.data]
  );

  const saved: Draft = useMemo(() => {
    const boards: Record<string, ProjectEvidenceOverride> = {};
    for (const [id, defaults] of Object.entries(
      config?.orchestrator?.project_defaults ?? {}
    )) {
      boards[id] = { ...EMPTY_OVERRIDE, ...defaults?.evidence };
    }
    return { sources: config?.evidence?.sources ?? [], boards };
  }, [config?.evidence, config?.orchestrator?.project_defaults]);

  const [draft, setDraft] = useState<Draft>(() => cloneDeep(saved));
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
    setContextDirty(EVIDENCE_AREA, hasUnsavedChanges);
    return () => setContextDirty(EVIDENCE_AREA, false);
  }, [hasUnsavedChanges, setContextDirty]);

  // Board names instead of UUIDs, from every organization the user is in.
  const { data: organizationsData } = useUserOrganizations();
  const { data: remoteProjects } = useQuery<RemoteProject[]>({
    queryKey: [
      'evidence-remote-projects',
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

  const patchBoard = (
    id: string,
    update: (board: ProjectEvidenceOverride) => ProjectEvidenceOverride
  ) => {
    setDirty(true);
    setDraft((prev) => ({
      ...prev,
      boards: {
        ...prev.boards,
        [id]: update(prev.boards[id] ?? EMPTY_OVERRIDE),
      },
    }));
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    setSuccess(false);
    try {
      const projectDefaults = cloneDeep(config.orchestrator.project_defaults);
      for (const [id, evidence] of Object.entries(draft.boards)) {
        const board = projectDefaults[id];
        if (board) board.evidence = evidence;
      }
      await updateAndSaveConfig({
        evidence: { sources: draft.sources },
        orchestrator: {
          ...config.orchestrator,
          project_defaults: projectDefaults,
        },
      });
      setDirty(false);
      setSuccess(true);
      setTimeout(() => setSuccess(false), 3000);
    } catch (err) {
      setError(t('settings.evidence.save.error'));
      console.error('Error saving evidence sources:', err);
    } finally {
      setSaving(false);
    }
  };

  if (loading || descriptorsQuery.isLoading) {
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

  if (!config || descriptorsQuery.isError) {
    return (
      <div className="py-8">
        <div className="bg-error/10 border border-error/50 rounded-sm p-4 text-error">
          {t('settings.general.loadError')}
        </div>
      </div>
    );
  }

  const switchOptions = [
    {
      value: 'inherit' as BoardSwitch,
      label: t('settings.evidence.boards.on'),
    },
    { value: 'off' as BoardSwitch, label: t('settings.evidence.boards.off') },
  ];
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
          {t('settings.evidence.save.success')}
        </div>
      )}
      <SettingsCard
        title={t('settings.evidence.sources.title')}
        description={t('settings.evidence.sources.description')}
      >
        <IntegrationSection<EvidenceSourceConfig>
          area={EVIDENCE_AREA}
          descriptors={descriptors}
          entries={draft.sources}
          multiple
          onChange={(sources) => {
            setDirty(true);
            setDraft((prev) => ({ ...prev, sources }));
          }}
        />
      </SettingsCard>

      <SettingsCard
        title={t('settings.evidence.boards.title')}
        description={t('settings.evidence.boards.description')}
      >
        {boardIds.length === 0 ? (
          <p className="text-sm text-low">
            {t('settings.evidence.boards.empty')}
          </p>
        ) : (
          boardIds.map((id) => {
            const board = draft.boards[id] ?? EMPTY_OVERRIDE;
            const off = board.enabled === false;
            return (
              <div
                key={id}
                className="space-y-4 rounded-sm border border-border p-base"
              >
                <div className="text-sm font-medium text-high">
                  {boardName(id)}
                </div>
                <SettingsField
                  label={t('settings.evidence.boards.enabledLabel')}
                  description={t('settings.evidence.boards.enabledHelper')}
                >
                  <SettingsSelect<BoardSwitch>
                    value={off ? 'off' : 'inherit'}
                    options={switchOptions}
                    onChange={(value) =>
                      patchBoard(id, (current) => ({
                        ...current,
                        enabled: value === 'off' ? false : null,
                      }))
                    }
                  />
                </SettingsField>
                {!off && (
                  <>
                    <SettingsField
                      label={t('settings.evidence.boards.skipGlobalLabel')}
                      description={t(
                        'settings.evidence.boards.skipGlobalHelper'
                      )}
                    >
                      <div className="flex flex-wrap gap-4">
                        {descriptors.map((descriptor) => {
                          const kind = descriptor.kind as EvidenceKind;
                          return (
                            <SettingsCheckbox
                              key={kind}
                              id={`evidence-skip-${id}-${kind}`}
                              label={descriptor.display_name}
                              checked={board.skip_global.includes(kind)}
                              onChange={(checked) =>
                                patchBoard(id, (current) => ({
                                  ...current,
                                  skip_global: checked
                                    ? [...current.skip_global, kind]
                                    : current.skip_global.filter(
                                        (skipped) => skipped !== kind
                                      ),
                                }))
                              }
                            />
                          );
                        })}
                      </div>
                    </SettingsField>
                    <SettingsField
                      label={t('settings.evidence.boards.ownLabel')}
                      description={t('settings.evidence.boards.ownHelper')}
                    >
                      <IntegrationSection<EvidenceSourceConfig>
                        area={EVIDENCE_AREA}
                        descriptors={descriptors}
                        entries={board.sources}
                        multiple
                        onChange={(sources) =>
                          patchBoard(id, (current) => ({
                            ...current,
                            sources,
                          }))
                        }
                      />
                    </SettingsField>
                  </>
                )}
              </div>
            );
          })
        )}
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
