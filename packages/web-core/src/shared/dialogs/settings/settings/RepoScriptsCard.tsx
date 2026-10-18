import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowClockwiseIcon,
  PlayIcon,
  SpinnerIcon,
} from '@phosphor-icons/react';
import type { Manifest, ReproductionTarget, RunStep, Step } from 'shared/types';
import { PrimaryButton } from '@prepcook/ui/components/PrimaryButton';
import { cn } from '@/shared/lib/utils';
import { useRepoScripts } from '@/shared/hooks/useRepoScripts';
import { useRepoScriptRun } from '@/shared/hooks/useRepoScriptRun';
import { SettingsCard } from './SettingsComponents';
import { ScriptRunPanel } from './ScriptRunPanel';
import { useIntegrationDescriptors } from './IntegrationSection';
import { REPRODUCTION_AREA } from './ReproductionSettingsSection';

/** Shown under a missing or invalid manifest; mirrors docs/scripts.md. */
const MINIMAL_MANIFEST_EXAMPLE = `[app]
setup = "pnpm install"
start = "pnpm start --port $PREPCOOK_PORT"
test = "pnpm test"`;

interface StepRow {
  step: Step;
  command: string;
  /** For `ready`, whether `command` is a URL path or a shell command. */
  readyKind?: 'url' | 'command';
  timeoutSecs: number;
}

/**
 * The steps `target` runs that are configured, in the order a stand-up runs
 * them: `[app]` for the local target; `[app] setup` then `[app.remote]` for a
 * remote one, which builds where it deploys.
 */
function configuredSteps(
  manifest: Manifest,
  target: ReproductionTarget
): StepRow[] {
  const { app } = manifest;
  if (target === 'remote' && app.remote) {
    const remote = app.remote;
    const rows: StepRow[] = [];
    if (app.setup) {
      rows.push({
        step: 'setup',
        command: app.setup,
        timeoutSecs: app.timeouts.setup,
      });
    }
    rows.push({
      step: 'start',
      command: remote.start,
      timeoutSecs: remote.timeouts.start,
    });
    if (remote.ready) {
      const isUrl = remote.ready.url != null;
      rows.push({
        step: 'ready',
        command: (isUrl ? remote.ready.url : remote.ready.command) ?? '',
        readyKind: isUrl ? 'url' : 'command',
        timeoutSecs: remote.ready.timeout_secs,
      });
    }
    for (const step of ['seed', 'test', 'stop'] as const) {
      const command = remote[step];
      if (command) {
        rows.push({ step, command, timeoutSecs: remote.timeouts[step] });
      }
    }
    return rows;
  }
  const rows: StepRow[] = [];
  if (app.setup) {
    rows.push({
      step: 'setup',
      command: app.setup,
      timeoutSecs: app.timeouts.setup,
    });
  }
  if (app.build) {
    rows.push({
      step: 'build',
      command: app.build,
      timeoutSecs: app.timeouts.build,
    });
  }
  rows.push({
    step: 'start',
    command: app.start,
    timeoutSecs: app.timeouts.start,
  });
  if (app.ready) {
    const isUrl = app.ready.url != null;
    rows.push({
      step: 'ready',
      command: (isUrl ? app.ready.url : app.ready.command) ?? '',
      readyKind: isUrl ? 'url' : 'command',
      timeoutSecs: app.ready.timeout_secs,
    });
  }
  if (app.seed) {
    rows.push({
      step: 'seed',
      command: app.seed,
      timeoutSecs: app.timeouts.seed,
    });
  }
  if (app.test) {
    rows.push({
      step: 'test',
      command: app.test,
      timeoutSecs: app.timeouts.test,
    });
  }
  if (app.stop) {
    rows.push({
      step: 'stop',
      command: app.stop,
      timeoutSecs: app.timeouts.stop,
    });
  }
  return rows;
}

interface RepoScriptsCardProps {
  repoId: string;
}

/**
 * A read-only view of a repository's `.prepcook/scripts.toml`, with test runs of
 * each step and of the whole stand-up. Nothing here edits the manifest.
 */
export function RepoScriptsCard({ repoId }: RepoScriptsCardProps) {
  const { t } = useTranslation(['settings', 'common']);
  const scriptsQuery = useRepoScripts(repoId);
  const scriptRun = useRepoScriptRun(repoId);
  const descriptors = useIntegrationDescriptors();
  const response = scriptsQuery.data;
  const manifest = response?.manifest ?? null;
  const target = response?.target ?? 'local';
  const targetDescriptor = descriptors.data?.[REPRODUCTION_AREA]?.find(
    (descriptor) => descriptor.kind === target
  );
  const targetProblem = response?.target_problem ?? null;
  const runBusy =
    scriptRun.isLive || scriptRun.isStarting || targetProblem != null;

  const runStep = (step: RunStep) => scriptRun.start(step);

  const headerAction = (
    <div className="flex items-center gap-half">
      <PrimaryButton
        variant="tertiary"
        value={t('settings.repos.scripts.reload')}
        actionIcon={scriptsQuery.isFetching ? 'spinner' : ArrowClockwiseIcon}
        onClick={() => void scriptsQuery.refetch()}
        disabled={scriptsQuery.isFetching}
      />
      {manifest && (
        <PrimaryButton
          variant="default"
          value={t('settings.repos.scripts.standUp')}
          actionIcon={PlayIcon}
          onClick={() => runStep('stand-up')}
          disabled={runBusy}
        />
      )}
    </div>
  );

  return (
    <SettingsCard
      title={t('settings.repos.scripts.title')}
      description={t('settings.repos.scripts.description')}
      headerAction={headerAction}
    >
      {scriptsQuery.isLoading ? (
        <div className="flex items-center gap-half py-half">
          <SpinnerIcon
            className="size-icon-xs animate-spin text-low"
            weight="bold"
          />
          <span className="text-sm text-low">
            {t('settings.repos.scripts.loading')}
          </span>
        </div>
      ) : scriptsQuery.error ? (
        <ErrorBox>
          {scriptsQuery.error instanceof Error
            ? scriptsQuery.error.message
            : t('settings.repos.scripts.loadError')}
        </ErrorBox>
      ) : response && !manifest ? (
        <ManifestProblem missing={response.missing} error={response.error} />
      ) : manifest ? (
        <ManifestView
          manifest={manifest}
          target={target}
          targetName={targetDescriptor?.display_name ?? target}
          targetSummary={targetDescriptor?.summary ?? null}
          targetProblem={targetProblem}
          missingSecrets={response?.required_secrets_missing ?? []}
          runBusy={runBusy}
          onRunStep={runStep}
        />
      ) : null}

      {scriptRun.startError && (
        <ErrorBox>
          {t('settings.repos.scripts.run.startError', {
            message:
              scriptRun.startError instanceof Error
                ? scriptRun.startError.message
                : String(scriptRun.startError),
          })}
        </ErrorBox>
      )}

      {scriptRun.run && (
        <ScriptRunPanel
          step={scriptRun.run.step}
          target={scriptRun.state?.target ?? target}
          state={scriptRun.state}
          stateError={scriptRun.stateError}
          logs={scriptRun.logs}
          logsError={scriptRun.logsError}
          isLive={scriptRun.isLive}
          onClose={scriptRun.dismiss}
        />
      )}
    </SettingsCard>
  );
}

function ErrorBox({ children }: { children: ReactNode }) {
  return (
    <div className="bg-error/10 border border-error/50 rounded-sm p-base text-sm text-error whitespace-pre-wrap break-words">
      {children}
    </div>
  );
}

function ManifestProblem({
  missing,
  error,
}: {
  missing: boolean;
  error: string | null;
}) {
  const { t } = useTranslation('settings');
  return (
    <div className="space-y-base">
      <div className="space-y-half">
        <p className="text-sm font-medium text-normal">
          {missing
            ? t('settings.repos.scripts.missingTitle')
            : t('settings.repos.scripts.invalidTitle')}
        </p>
        {error && <ErrorBox>{error}</ErrorBox>}
      </div>
      <p className="text-sm text-low">{t('settings.repos.scripts.hint')}</p>
      <pre className="bg-secondary border border-border rounded-sm px-base py-half text-sm text-high font-ibm-plex-mono overflow-x-auto">
        {MINIMAL_MANIFEST_EXAMPLE}
      </pre>
    </div>
  );
}

function ManifestView({
  manifest,
  target,
  targetName,
  targetSummary,
  targetProblem,
  missingSecrets,
  runBusy,
  onRunStep,
}: {
  manifest: Manifest;
  target: ReproductionTarget;
  targetName: string;
  targetSummary: string | null;
  targetProblem: string | null;
  missingSecrets: string[];
  runBusy: boolean;
  onRunStep: (step: RunStep) => void;
}) {
  const { t } = useTranslation('settings');
  const steps = configuredSteps(manifest, target);
  const hooks = (['setup', 'cleanup', 'archive'] as const)
    .map((hook) => ({ hook, command: manifest.workspace[hook] }))
    .filter((entry): entry is { hook: typeof entry.hook; command: string } =>
      Boolean(entry.command)
    );
  const required = manifest.secrets.requires;
  const profile = manifest.profile;
  const missing = new Set(missingSecrets);

  return (
    <div className="space-y-double">
      {profile && (profile.name || profile.summary) && (
        <Subsection title={t('settings.repos.scripts.profile.title')}>
          {profile.name && (
            <p className="text-sm font-medium text-high">{profile.name}</p>
          )}
          {profile.summary && (
            <p className="text-sm text-low">{profile.summary}</p>
          )}
        </Subsection>
      )}

      <Subsection title={t('settings.repos.scripts.target.title')}>
        <div className="flex items-center gap-half">
          <span className="inline-flex items-center rounded-sm bg-secondary border border-border px-half py-0.5 text-sm text-normal">
            {targetName}
          </span>
          <span className="text-sm font-ibm-plex-mono text-low">{target}</span>
        </div>
        {targetSummary && <p className="text-sm text-low">{targetSummary}</p>}
        <p className="text-sm text-low">
          {t('settings.repos.scripts.target.hint')}
        </p>
        {targetProblem && <ErrorBox>{targetProblem}</ErrorBox>}
      </Subsection>

      <Subsection title={t('settings.repos.scripts.steps.title')}>
        {steps.map((row) => (
          <div
            key={row.step}
            className="flex items-start gap-base rounded-sm border border-border bg-secondary px-base py-half"
          >
            <div className="flex-1 min-w-0 space-y-0.5">
              <div className="flex items-center gap-half">
                <span className="text-sm font-medium text-normal font-ibm-plex-mono">
                  {row.step}
                </span>
                {row.readyKind && (
                  <span className="text-sm text-low">
                    {row.readyKind === 'url'
                      ? t('settings.repos.scripts.steps.readyUrl')
                      : t('settings.repos.scripts.steps.readyCommand')}
                  </span>
                )}
                <span className="text-sm text-low">
                  {t('settings.repos.scripts.steps.timeout', {
                    seconds: row.timeoutSecs,
                  })}
                </span>
              </div>
              <code className="block text-sm text-high font-ibm-plex-mono whitespace-pre-wrap break-all">
                {row.command}
              </code>
            </div>
            <PrimaryButton
              variant="tertiary"
              value={t('settings.repos.scripts.steps.testRun')}
              onClick={() => onRunStep(row.step)}
              disabled={runBusy}
              className="shrink-0"
            />
          </div>
        ))}
      </Subsection>

      {hooks.length > 0 && (
        <Subsection title={t('settings.repos.scripts.workspace.title')}>
          {hooks.map(({ hook, command }) => (
            <div
              key={hook}
              className="rounded-sm border border-border bg-secondary px-base py-half space-y-0.5"
            >
              <span className="text-sm font-medium text-normal font-ibm-plex-mono">
                {hook}
              </span>
              <code className="block text-sm text-high font-ibm-plex-mono whitespace-pre-wrap break-all">
                {command}
              </code>
            </div>
          ))}
        </Subsection>
      )}

      {required.length > 0 && (
        <Subsection title={t('settings.repos.scripts.secrets.title')}>
          <div className="flex flex-wrap gap-half">
            {required.map((name) => {
              const isMissing = missing.has(name);
              return (
                <span
                  key={name}
                  className="inline-flex items-center gap-half rounded-sm bg-secondary border border-border px-half py-0.5 text-sm"
                >
                  <span
                    className={cn(
                      'size-dot rounded-full',
                      isMissing ? 'bg-error' : 'bg-success'
                    )}
                  />
                  <span className="font-ibm-plex-mono text-normal">{name}</span>
                  <span className={isMissing ? 'text-error' : 'text-low'}>
                    {isMissing
                      ? t('settings.repos.scripts.secrets.missing')
                      : t('settings.repos.scripts.secrets.set')}
                  </span>
                </span>
              );
            })}
          </div>
          {missing.size > 0 && (
            <p className="text-sm text-low">
              {t('settings.repos.scripts.secrets.missingHint')}
            </p>
          )}
        </Subsection>
      )}
    </div>
  );
}

function Subsection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-half">
      <p className="text-sm font-medium text-normal">{title}</p>
      {children}
    </div>
  );
}
