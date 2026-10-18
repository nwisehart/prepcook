import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  AgentDescriptor,
  BaseCodingAgent,
  ExecutorProfile,
  ExecutorProfileId,
  StageProfiles,
} from 'shared/types';
import { toPrettyCase } from '@/shared/lib/string';
import { getSortedExecutorVariantKeys } from '@/shared/lib/executor';
import { SettingsField, SettingsSelect } from './SettingsComponents';

/** The stages that start an agent session of their own, in pipeline order. */
export const AGENT_STAGES: (keyof StageProfiles)[] = [
  'planning',
  'executing',
  'reviewing',
  'comment_reply',
  'pattern',
];

export const EMPTY_STAGE_PROFILES: StageProfiles = {
  planning: null,
  executing: null,
  reviewing: null,
  comment_reply: null,
  pattern: null,
};

/** The select value standing for "no override". */
const INHERIT = '__inherit__';

export const profileKey = (profile: ExecutorProfileId): string =>
  profile.variant ? `${profile.executor}:${profile.variant}` : profile.executor;

export const parseProfileKey = (key: string): ExecutorProfileId => {
  const [executor, variant] = key.split(':');
  return { executor: executor as BaseCodingAgent, variant: variant ?? null };
};

export const profileLabel = (profile: ExecutorProfileId): string =>
  profile.variant
    ? `${toPrettyCase(profile.executor)} · ${toPrettyCase(profile.variant)}`
    : toPrettyCase(profile.executor);

/**
 * What a stage runs when nothing names a profile for it: the default profile
 * for Planning; every other stage takes the agent's economy variant when the
 * default names none. Mirrors `AgentStage::default_profile` on the server.
 */
export const stageDefault = (
  stage: keyof StageProfiles,
  defaultProfile: ExecutorProfileId,
  agents: AgentDescriptor[]
): ExecutorProfileId => {
  if (stage === 'planning' || defaultProfile.variant) return defaultProfile;
  const economy = agents.find(
    (agent) => agent.executor === defaultProfile.executor
  )?.economy_variant;
  return economy ? { ...defaultProfile, variant: economy } : defaultProfile;
};

/** Every discovered profile: each agent, then each of its variants. */
export function useProfileOptions(
  profiles: Record<string, ExecutorProfile> | null
): { value: string; label: string }[] {
  return useMemo(() => {
    const options: { value: string; label: string }[] = [];
    for (const executor of Object.keys(profiles ?? {}).sort()) {
      const id = { executor: executor as BaseCodingAgent, variant: null };
      options.push({ value: profileKey(id), label: profileLabel(id) });
      for (const variant of getSortedExecutorVariantKeys(
        profiles?.[executor]
      )) {
        if (variant === 'DEFAULT') continue;
        const withVariant = { ...id, variant };
        options.push({
          value: profileKey(withVariant),
          label: profileLabel(withVariant),
        });
      }
    }
    return options;
  }, [profiles]);
}

/**
 * One profile select with an "inherit" entry that names what inheriting
 * resolves to.
 */
export function ProfileSelect({
  value,
  inherited,
  inheritLabel,
  options,
  onChange,
}: {
  value: ExecutorProfileId | null;
  inherited: ExecutorProfileId;
  /** Shown before the inherited profile, e.g. "Default". */
  inheritLabel: string;
  options: { value: string; label: string }[];
  onChange: (value: ExecutorProfileId | null) => void;
}) {
  const all = [
    {
      value: INHERIT,
      label: `${inheritLabel} (${profileLabel(inherited)})`,
    },
    ...options,
  ];
  return (
    <SettingsSelect<string>
      value={value ? profileKey(value) : INHERIT}
      options={all}
      onChange={(next) =>
        onChange(next === INHERIT ? null : parseProfileKey(next))
      }
    />
  );
}

/**
 * The five per-stage overrides. `fallback` is what each unset stage resolves
 * to: the stage default from the default profile, or, on a board, the global
 * override first.
 */
export function StageProfileFields({
  stages,
  fallback,
  options,
  onChange,
  inheritLabel,
}: {
  stages: StageProfiles;
  fallback: (stage: keyof StageProfiles) => ExecutorProfileId;
  options: { value: string; label: string }[];
  onChange: (stages: StageProfiles) => void;
  /** The unset entry's label; "Default" when not given. */
  inheritLabel?: string;
}) {
  const { t } = useTranslation('settings');
  return (
    <>
      {AGENT_STAGES.map((stage) => (
        <SettingsField
          key={stage}
          label={t(`settings.codingAgents.stages.${stage}.label`)}
          description={t(`settings.codingAgents.stages.${stage}.helper`)}
        >
          <ProfileSelect
            value={stages[stage]}
            inherited={fallback(stage)}
            inheritLabel={
              inheritLabel ?? t('settings.codingAgents.stages.inherit')
            }
            options={options}
            onChange={(profile) => onChange({ ...stages, [stage]: profile })}
          />
        </SettingsField>
      ))}
    </>
  );
}
