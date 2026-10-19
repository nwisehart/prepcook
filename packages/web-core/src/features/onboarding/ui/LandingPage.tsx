import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { Trans, useTranslation } from 'react-i18next';
import {
  BirdIcon,
  CheckIcon,
  CowIcon,
  DeviceMobileIcon,
  MusicNoteIcon,
  MusicNotesIcon,
  SpeakerHighIcon,
  SpeakerXIcon,
  WarningIcon,
  WaveformIcon,
  type Icon,
} from '@phosphor-icons/react';
import { usePostHog } from 'posthog-js/react';
import {
  BaseCodingAgent,
  EditorType,
  SoundFile,
  type BoardStatus,
  type EditorConfig,
} from 'shared/types';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { AgentIcon, getAgentName } from '@/shared/components/AgentIcon';
import { IdeIcon } from '@/shared/components/IdeIcon';
import { getIdeName } from '@/shared/lib/ideName';
import { cn, playSound } from '@/shared/lib/utils';
import { isTauriApp } from '@/shared/lib/platform';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { Button } from '@prepcook/ui/components/Button';
import { PrepCookMark } from '@prepcook/ui/components/PrepCookMark';

type SoundOption = {
  value: SoundFile;
  label: string;
  icon: Icon;
};

const SOUND_OPTIONS: SoundOption[] = [
  {
    value: SoundFile.ABSTRACT_SOUND1,
    label: 'Abstract Sound 1',
    icon: WaveformIcon,
  },
  {
    value: SoundFile.ABSTRACT_SOUND2,
    label: 'Abstract Sound 2',
    icon: MusicNoteIcon,
  },
  {
    value: SoundFile.ABSTRACT_SOUND3,
    label: 'Abstract Sound 3',
    icon: MusicNotesIcon,
  },
  {
    value: SoundFile.ABSTRACT_SOUND4,
    label: 'Abstract Sound 4',
    icon: SpeakerHighIcon,
  },
  {
    value: SoundFile.COW_MOOING,
    label: 'Cow Mooing',
    icon: CowIcon,
  },
  {
    value: SoundFile.PHONE_VIBRATION,
    label: 'Phone Vibration',
    icon: DeviceMobileIcon,
  },
  {
    value: SoundFile.ROOSTER,
    label: 'Rooster',
    icon: BirdIcon,
  },
];

const AGENT_PRIORITY: BaseCodingAgent[] = [
  BaseCodingAgent.CLAUDE_CODE,
  BaseCodingAgent.CODEX,
  BaseCodingAgent.OPENCODE,
  BaseCodingAgent.GEMINI,
];

const REPO_URL = 'https://github.com/nwisehart/prepcook';
const DOCS_URL = `${REPO_URL}/tree/main/docs`;

const REMOTE_ONBOARDING_EVENTS = {
  STAGE_VIEWED: 'remote_onboarding_ui_stage_viewed',
  STAGE_SUBMITTED: 'remote_onboarding_ui_stage_submitted',
  STAGE_COMPLETED: 'remote_onboarding_ui_stage_completed',
  STAGE_FAILED: 'remote_onboarding_ui_stage_failed',
} as const;

/** The first-run steps, in order: each one is one of the saved choices. */
const STEPS = ['agent', 'editor', 'sound'] as const;
type Step = (typeof STEPS)[number];

/** The board's columns, left to right, with the dot each column shows. */
const STAGES: { id: string; dot: string }[] = [
  { id: 'todo', dot: 'border-2 border-low/60' },
  { id: 'plan', dot: 'bg-brand' },
  { id: 'execute', dot: 'bg-brand' },
  { id: 'review', dot: 'bg-brand' },
  { id: 'pushMr', dot: 'bg-brand' },
  { id: 'close', dot: 'bg-success' },
  { id: 'cleanUp', dot: 'bg-low/50' },
];

function randomDefaultSoundFile(): SoundFile {
  const randomIndex = Math.floor(Math.random() * SOUND_OPTIONS.length);
  return SOUND_OPTIONS[randomIndex]?.value ?? SoundFile.COW_MOOING;
}

export function LandingPage() {
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const {
    config,
    profiles,
    updateAndSaveConfig,
    loading,
    appVersion,
    boardStatus,
  } = useUserSystem();
  const posthog = usePostHog();

  const [initialized, setInitialized] = useState(false);
  const [saving, setSaving] = useState(false);
  const [step, setStep] = useState<Step>('agent');
  const [selectedAgent, setSelectedAgent] = useState<BaseCodingAgent>(
    BaseCodingAgent.CLAUDE_CODE
  );
  const [editorType, setEditorType] = useState<EditorType>(EditorType.VS_CODE);
  const [customCommand, setCustomCommand] = useState('');
  const [soundEnabled, setSoundEnabled] = useState(true);
  const [soundFile, setSoundFile] = useState<SoundFile>(randomDefaultSoundFile);
  const hasTrackedStageViewRef = useRef(false);
  const hasRedirectedToRootRef = useRef(false);

  const trackRemoteOnboardingEvent = useCallback(
    (eventName: string, properties: Record<string, unknown> = {}) => {
      posthog?.capture(eventName, {
        ...properties,
        flow: 'remote_onboarding_ui',
        source: 'frontend',
      });
    },
    [posthog]
  );

  useEffect(() => {
    if (!config || initialized) return;

    setSelectedAgent(config.executor_profile.executor);
    setEditorType(config.editor.editor_type);
    setCustomCommand(config.editor.custom_command || '');
    setInitialized(true);
  }, [config, initialized]);

  useEffect(() => {
    if (!config || !initialized || hasTrackedStageViewRef.current) return;

    trackRemoteOnboardingEvent(REMOTE_ONBOARDING_EVENTS.STAGE_VIEWED, {
      stage: 'landing',
    });
    hasTrackedStageViewRef.current = true;
  }, [config, initialized, trackRemoteOnboardingEvent]);

  useEffect(() => {
    if (
      !config?.remote_onboarding_acknowledged ||
      hasRedirectedToRootRef.current
    ) {
      return;
    }

    hasRedirectedToRootRef.current = true;
    appNavigation.goToRoot({ replace: true });
  }, [appNavigation, config?.remote_onboarding_acknowledged]);

  const executorOptions = useMemo(() => {
    const compareAgents = (a: BaseCodingAgent, b: BaseCodingAgent) => {
      const priorityA = AGENT_PRIORITY.indexOf(a);
      const priorityB = AGENT_PRIORITY.indexOf(b);
      const hasPriorityA = priorityA !== -1;
      const hasPriorityB = priorityB !== -1;

      if (hasPriorityA && hasPriorityB) {
        return priorityA - priorityB;
      }
      if (hasPriorityA) return -1;
      if (hasPriorityB) return 1;

      return getAgentName(a).localeCompare(getAgentName(b));
    };

    if (profiles) {
      return (Object.keys(profiles) as BaseCodingAgent[]).sort(compareAgents);
    }
    return [...Object.values(BaseCodingAgent)].sort(compareAgents);
  }, [profiles]);

  const editorOptions = useMemo(() => Object.values(EditorType), []);

  const previewSound = async (value: SoundFile) => {
    try {
      await playSound(`/api/sounds/${value}`);
    } catch (err) {
      console.error('Failed to play sound:', err);
    }
  };

  const handleSoundSelect = (value: SoundFile) => {
    setSoundEnabled(true);
    setSoundFile(value);
    void previewSound(value);
  };

  const isCustomEditorValid =
    editorType !== EditorType.CUSTOM || customCommand.trim() !== '';
  const canContinue = !saving && isCustomEditorValid;

  const stepIndex = STEPS.indexOf(step);
  const isLastStep = stepIndex === STEPS.length - 1;
  // The editor step needs its custom command before moving on; the last
  // step saves every choice at once.
  const canAdvance =
    step === 'editor' ? isCustomEditorValid : isLastStep ? canContinue : true;

  const handleContinue = async () => {
    if (!config || !canContinue) return;

    const editorConfig: EditorConfig = {
      editor_type: editorType,
      custom_command:
        editorType === EditorType.CUSTOM ? customCommand.trim() : null,
      remote_ssh_host: null,
      remote_ssh_user: null,
    };

    trackRemoteOnboardingEvent(REMOTE_ONBOARDING_EVENTS.STAGE_SUBMITTED, {
      stage: 'landing',
      method: 'continue',
      selected_agent: selectedAgent,
      editor_type: editorType,
      custom_editor_command_set:
        editorType === EditorType.CUSTOM && customCommand.trim() !== '',
      sound_enabled: soundEnabled,
      sound_file: soundEnabled ? soundFile : null,
    });

    setSaving(true);
    // `prepcook up` signs the server in to the board itself, so these
    // preferences are the whole of onboarding.
    const success = await updateAndSaveConfig({
      remote_onboarding_acknowledged: true,
      onboarding_acknowledged: true,
      disclaimer_acknowledged: true,
      executor_profile: {
        executor: selectedAgent,
        variant: null,
      },
      editor: editorConfig,
      notifications: {
        ...config.notifications,
        sound_enabled: soundEnabled,
        sound_file: soundFile,
      },
    });
    setSaving(false);

    if (success) {
      trackRemoteOnboardingEvent(REMOTE_ONBOARDING_EVENTS.STAGE_COMPLETED, {
        stage: 'landing',
        destination: '/',
      });
      hasRedirectedToRootRef.current = true;
      appNavigation.goToRoot({ replace: true });
      return;
    }

    trackRemoteOnboardingEvent(REMOTE_ONBOARDING_EVENTS.STAGE_FAILED, {
      stage: 'landing',
      reason: 'config_save_failed',
    });
  };

  const handleNext = () => {
    if (!canAdvance) return;
    if (isLastStep) {
      void handleContinue();
      return;
    }
    setStep(STEPS[stepIndex + 1]);
  };

  if (loading || !config || !initialized) {
    return (
      <div className="h-screen bg-primary flex items-center justify-center">
        <p className="text-sm text-low">{t('states.loading')}</p>
      </div>
    );
  }

  if (config.remote_onboarding_acknowledged) {
    return null;
  }

  return (
    <div className="flex h-screen flex-col overflow-y-auto bg-primary text-sm text-normal">
      {isTauriApp() && (
        <div
          data-tauri-drag-region
          className="fixed inset-x-0 top-0 h-10 z-10"
        />
      )}

      <header className="flex flex-wrap items-center gap-2.5 border-b border-border px-7 py-[18px]">
        <PrepCookMark className="!size-[26px] text-brand" />
        <span className="text-[17px] font-bold tracking-[-0.2px] text-high">
          {t('onboarding.productName')}
        </span>
        <span className="flex-1" />
        <RuntimeStatus appVersion={appVersion} boardStatus={boardStatus} />
        <a
          href={DOCS_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="text-[13px] font-semibold text-brand-secondary hover:underline"
        >
          {t('onboarding.docs')}
        </a>
        <a
          href={REPO_URL}
          target="_blank"
          rel="noopener noreferrer"
          className="text-[13px] font-semibold text-brand-secondary hover:underline"
        >
          {t('onboarding.github')}
        </a>
      </header>

      <main className="flex flex-1 justify-center px-7 pb-12 pt-10">
        <div className="flex w-full max-w-[1180px] flex-wrap items-start gap-10">
          <section className="flex min-w-0 flex-[999_1_520px] flex-col gap-[22px]">
            <div className="flex flex-col gap-2">
              <h1 className="text-[30px] font-bold leading-tight tracking-[-0.5px] text-high">
                {t('onboarding.title')}
              </h1>
              <p className="max-w-[620px] text-[15px] leading-[1.45] text-normal">
                {t('onboarding.lead')}
              </p>
            </div>

            <StepIndicator
              current={step}
              onSelect={(next) => {
                // Steps after the editor need a valid editor choice first.
                if (
                  STEPS.indexOf(next) > STEPS.indexOf('editor') &&
                  !isCustomEditorValid
                ) {
                  return;
                }
                setStep(next);
              }}
            />

            <section className="flex flex-col gap-[18px] rounded-lg border border-border bg-secondary px-[22px] py-5">
              <div className="flex flex-col gap-1.5">
                <span id={`onboarding-${step}`} className="font-bold text-high">
                  {t(`onboarding.steps.${step}.question`)}
                </span>
                <span className="text-[12.5px] leading-[1.45] text-low">
                  {t(`onboarding.steps.${step}.helper`)}
                </span>
              </div>

              {step === 'agent' && (
                <ChoiceGrid labelledBy="onboarding-agent">
                  {executorOptions.map((agent) => (
                    <ChoiceRow
                      key={agent}
                      selected={selectedAgent === agent}
                      onSelect={() => setSelectedAgent(agent)}
                      icon={
                        <AgentIcon agent={agent} className="size-5 shrink-0" />
                      }
                      label={getAgentName(agent)}
                    />
                  ))}
                </ChoiceGrid>
              )}

              {step === 'editor' && (
                <>
                  <ChoiceGrid labelledBy="onboarding-editor">
                    {editorOptions.map((editor) => (
                      <ChoiceRow
                        key={editor}
                        selected={editorType === editor}
                        onSelect={() => setEditorType(editor)}
                        icon={
                          <IdeIcon
                            editorType={editor}
                            className="size-4 shrink-0"
                          />
                        }
                        label={getIdeName(editor)}
                      />
                    ))}
                  </ChoiceGrid>

                  {editorType === EditorType.CUSTOM && (
                    <div className="flex flex-col gap-1.5">
                      <label
                        htmlFor="onboarding-custom-command"
                        className="font-semibold text-high"
                      >
                        {t('onboarding.steps.editor.customCommand')}
                      </label>
                      <input
                        id="onboarding-custom-command"
                        type="text"
                        value={customCommand}
                        onChange={(e) => setCustomCommand(e.target.value)}
                        placeholder="code --wait"
                        className={cn(
                          'h-9 w-full rounded border bg-secondary px-3 font-mono text-[13px] text-high',
                          'placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
                          customCommand.trim() === ''
                            ? 'border-error'
                            : 'border-border'
                        )}
                      />
                    </div>
                  )}
                </>
              )}

              {step === 'sound' && (
                <ChoiceGrid labelledBy="onboarding-sound">
                  {SOUND_OPTIONS.map((option) => {
                    const OptionIcon = option.icon;
                    const selected = soundEnabled && soundFile === option.value;
                    return (
                      <ChoiceRow
                        key={option.value}
                        selected={selected}
                        onSelect={() => handleSoundSelect(option.value)}
                        icon={
                          <OptionIcon
                            className={cn(
                              'size-4 shrink-0',
                              selected ? 'text-brand' : 'text-low'
                            )}
                          />
                        }
                        label={option.label}
                      />
                    );
                  })}
                  <ChoiceRow
                    selected={!soundEnabled}
                    onSelect={() => setSoundEnabled(false)}
                    icon={
                      <SpeakerXIcon
                        className={cn(
                          'size-4 shrink-0',
                          !soundEnabled ? 'text-brand' : 'text-low'
                        )}
                      />
                    }
                    label={t('onboarding.steps.sound.none')}
                  />
                </ChoiceGrid>
              )}

              <div className="flex flex-wrap items-center gap-2.5 border-t border-divider pt-4">
                {stepIndex > 0 && (
                  <Button
                    variant="outline"
                    size="lg"
                    onClick={() => setStep(STEPS[stepIndex - 1])}
                    disabled={saving}
                  >
                    {t('onboarding.back')}
                  </Button>
                )}
                <span className="text-[12.5px] text-low">
                  {t('onboarding.stepCount', {
                    current: stepIndex + 1,
                    total: STEPS.length,
                  })}
                </span>
                <span className="flex-1" />
                <Button size="lg" onClick={handleNext} disabled={!canAdvance}>
                  {saving ? t('onboarding.saving') : t('buttons.continue')}
                </Button>
              </div>
            </section>

            <p className="flex items-start gap-2 rounded-lg border border-border bg-surface-raised px-3.5 py-2.5 text-[12.5px] leading-[1.45] text-normal">
              <WarningIcon
                className="mt-0.5 size-3.5 shrink-0 text-attention-foreground"
                weight="fill"
              />
              <span>
                <Trans
                  t={t}
                  i18nKey="onboarding.permissionsNote"
                  components={{
                    code: <code className="font-mono text-xs text-high" />,
                  }}
                />
              </span>
            </p>
          </section>

          <BugJourney />
        </div>
      </main>
    </div>
  );
}

/** Version, where the server runs, and whether the board answers. */
function RuntimeStatus({
  appVersion,
  boardStatus,
}: {
  appVersion: string | null;
  boardStatus: BoardStatus | null;
}) {
  const { t } = useTranslation('common');
  const board =
    boardStatus?.status === 'connected'
      ? { dot: 'bg-success', label: t('onboarding.status.boardUp') }
      : boardStatus?.status === 'not_answering'
        ? { dot: 'bg-attention', label: t('onboarding.status.boardDown') }
        : boardStatus
          ? { dot: 'bg-low/50', label: t('onboarding.status.noBoard') }
          : null;
  return (
    <span className="mr-2 hidden items-center gap-1.5 text-[12.5px] text-low sm:inline-flex">
      {appVersion && (
        <>
          <span className="font-mono">v{appVersion}</span>
          <span aria-hidden="true">·</span>
        </>
      )}
      {t('onboarding.status.local')}
      {board && (
        <>
          <span aria-hidden="true">·</span>
          <span className="inline-flex items-center gap-1.5">
            <span className={cn('size-2 rounded-full', board.dot)} />
            {board.label}
          </span>
        </>
      )}
    </span>
  );
}

function StepIndicator({
  current,
  onSelect,
}: {
  current: Step;
  onSelect: (step: Step) => void;
}) {
  const { t } = useTranslation('common');
  const currentIndex = STEPS.indexOf(current);
  return (
    <ol className="m-0 flex list-none flex-wrap items-center gap-2.5 p-0">
      {STEPS.map((step, index) => {
        const isCurrent = index === currentIndex;
        const isDone = index < currentIndex;
        return (
          <li key={step} className="flex items-center gap-2.5">
            {index > 0 && (
              <span aria-hidden="true" className="h-px w-7 bg-border" />
            )}
            <button
              type="button"
              onClick={() => onSelect(step)}
              aria-current={isCurrent ? 'step' : undefined}
              className={cn(
                'flex items-center gap-2 rounded text-sm transition-colors',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                isCurrent
                  ? 'font-semibold text-high'
                  : 'font-medium text-low hover:text-high'
              )}
            >
              <span
                className={cn(
                  'inline-flex size-[26px] shrink-0 items-center justify-center rounded-full text-[12.5px] font-bold',
                  isCurrent
                    ? 'bg-brand text-on-brand'
                    : isDone
                      ? 'bg-brand-soft text-brand-soft-foreground'
                      : 'bg-panel text-low'
                )}
              >
                {isDone ? <CheckIcon className="size-3.5" /> : index + 1}
              </span>
              {t(`onboarding.steps.${step}.name`)}
            </button>
          </li>
        );
      })}
    </ol>
  );
}

function ChoiceGrid({
  labelledBy,
  children,
}: {
  labelledBy: string;
  children: ReactNode;
}) {
  return (
    <div
      role="radiogroup"
      aria-labelledby={labelledBy}
      className="grid grid-cols-1 gap-2 sm:grid-cols-2"
    >
      {children}
    </div>
  );
}

/** One selectable option: icon, name and a radio dot. */
function ChoiceRow({
  selected,
  onSelect,
  icon,
  label,
}: {
  selected: boolean;
  onSelect: () => void;
  icon: ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn(
        'flex h-11 items-center gap-3 rounded-lg border bg-secondary px-3 text-left transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        selected
          ? 'border-brand ring-1 ring-brand'
          : 'border-border hover:bg-panel'
      )}
    >
      {icon}
      <span className="flex-1 truncate text-sm font-medium text-high">
        {label}
      </span>
      <span
        aria-hidden="true"
        className={cn(
          'size-4 shrink-0 rounded-full bg-secondary',
          selected ? 'border-[5px] border-brand' : 'border-2 border-low/50'
        )}
      />
    </button>
  );
}

/** The board's stages, so a first-time user knows what the cards will do. */
function BugJourney() {
  const { t } = useTranslation('common');
  return (
    <aside className="flex max-w-[420px] flex-[1_1_360px] flex-col gap-4 rounded-lg border border-border bg-secondary px-6 py-[22px]">
      <div className="flex flex-col gap-1">
        <h2 className="text-[15px] font-bold text-high">
          {t('onboarding.journey.title')}
        </h2>
        <p className="text-[12.5px] leading-[1.45] text-low">
          {t('onboarding.journey.helper')}
        </p>
      </div>
      <ol className="m-0 flex list-none flex-col gap-3 p-0">
        {STAGES.map((stage) => (
          <li key={stage.id} className="flex items-start gap-3">
            <span
              aria-hidden="true"
              className={cn('mt-1.5 size-2.5 shrink-0 rounded-full', stage.dot)}
            />
            <span className="min-w-[70px] font-bold text-high">
              {t(`onboarding.stages.${stage.id}.name`)}
            </span>
            <span className="text-[12.5px] leading-[1.45] text-low">
              {t(`onboarding.stages.${stage.id}.description`)}
            </span>
          </li>
        ))}
      </ol>
      <div className="flex flex-col gap-1 border-t border-divider pt-3">
        <span className="font-semibold text-high">
          {t('onboarding.noTracker.title')}
        </span>
        <span className="text-[12.5px] leading-[1.45] text-low">
          {t('onboarding.noTracker.description')}
        </span>
      </div>
    </aside>
  );
}
