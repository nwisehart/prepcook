import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowClockwiseIcon,
  PlayIcon,
  SpinnerIcon,
} from '@phosphor-icons/react';
import type { Manifest, ReproductionTarget, RunStep, Step } from 'shared/types';
import { Button } from '@prepcook/ui/components/Button';
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
    <div className="flex items-center gap-2">
      <PrimaryButton
        variant="secondary"
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
        <div className="flex items-center gap-2 py-1">
          <SpinnerIcon className="size-3.5 animate-spin text-low" />
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
    <div className="rounded-lg border border-error/30 bg-error-soft px-3 py-2 text-[12.5px] text-error whitespace-pre-wrap break-words">
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
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <p className="text-sm font-semibold text-high">
          {missing
            ? t('settings.repos.scripts.missingTitle')
            : t('settings.repos.scripts.invalidTitle')}
        </p>
        {error && <ErrorBox>{error}</ErrorBox>}
      </div>
      <p className="text-[12.5px] leading-[1.45] text-low">
        {t('settings.repos.scripts.hint')}
      </p>
      <pre className="overflow-x-auto rounded border border-border bg-surface-raised px-3 py-2 font-mono text-[12.5px] text-high">
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
    <div className="flex flex-col gap-5">
      {profile && (profile.name || profile.summary) && (
        <Subsection title={t('settings.repos.scripts.profile.title')}>
          {profile.name && (
            <p className="text-sm font-semibold text-high">{profile.name}</p>
          )}
          {profile.summary && (
            <p className="text-[12.5px] leading-[1.45] text-low">
              {profile.summary}
            </p>
          )}
        </Subsection>
      )}

      <Subsection title={t('settings.repos.scripts.target.title')}>
        <div className="flex items-center gap-2">
          <span className="inline-flex h-[22px] items-center rounded-full bg-panel px-2 text-xs font-medium text-normal">
            {targetName}
          </span>
          <span className="font-mono text-xs text-low">{target}</span>
        </div>
        {targetSummary && (
          <p className="text-[12.5px] leading-[1.45] text-low">
            {targetSummary}
          </p>
        )}
        <p className="text-[12.5px] leading-[1.45] text-low">
          {t('settings.repos.scripts.target.hint')}
        </p>
        {targetProblem && <ErrorBox>{targetProblem}</ErrorBox>}
      </Subsection>

      <Subsection title={t('settings.repos.scripts.steps.title')}>
        <div className="flex flex-col">
          {steps.map((row, index) => (
            <div
              key={row.step}
              className={cn(
                'grid grid-cols-[88px_minmax(0,1fr)_auto] items-center gap-3 py-2',
                index > 0 && 'border-t border-divider'
              )}
            >
              <span className="text-[13px] font-semibold text-high">
                {row.step}
              </span>
              <div className="flex min-w-0 flex-col gap-0.5">
                <code className="block whitespace-pre-wrap break-all font-mono text-[12.5px] text-high">
                  {row.command}
                </code>
                <span className="text-xs text-low">
                  {row.readyKind &&
                    `${
                      row.readyKind === 'url'
                        ? t('settings.repos.scripts.steps.readyUrl')
                        : t('settings.repos.scripts.steps.readyCommand')
                    } · `}
                  {t('settings.repos.scripts.steps.timeout', {
                    seconds: row.timeoutSecs,
                  })}
                </span>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => onRunStep(row.step)}
                disabled={runBusy}
              >
                {t('settings.repos.scripts.steps.testRun')}
              </Button>
            </div>
          ))}
        </div>
      </Subsection>

      {hooks.length > 0 && (
        <Subsection title={t('settings.repos.scripts.workspace.title')}>
          <div className="flex flex-col">
            {hooks.map(({ hook, command }, index) => (
              <div
                key={hook}
                className={cn(
                  'grid grid-cols-[88px_minmax(0,1fr)] items-center gap-3 py-2',
                  index > 0 && 'border-t border-divider'
                )}
              >
                <span className="text-[13px] font-semibold text-high">
                  {hook}
                </span>
                <code className="block whitespace-pre-wrap break-all font-mono text-[12.5px] text-high">
                  {command}
                </code>
              </div>
            ))}
          </div>
        </Subsection>
      )}

      {required.length > 0 && (
        <Subsection title={t('settings.repos.scripts.secrets.title')}>
          <div className="flex flex-wrap gap-2">
            {required.map((name) => {
              const isMissing = missing.has(name);
              return (
                <span
                  key={name}
                  className={cn(
                    'inline-flex h-[22px] items-center gap-1.5 rounded-full px-2 text-xs font-medium',
                    isMissing
                      ? 'bg-error-soft text-error'
                      : 'bg-panel text-normal'
                  )}
                >
                  <span
                    className={cn(
                      'size-2 rounded-full',
                      isMissing ? 'bg-error' : 'bg-success'
                    )}
                  />
                  <span className="font-mono">{name}</span>
                  <span className={isMissing ? undefined : 'text-low'}>
                    {isMissing
                      ? t('settings.repos.scripts.secrets.missing')
                      : t('settings.repos.scripts.secrets.set')}
                  </span>
                </span>
              );
            })}
          </div>
          {missing.size > 0 && (
            <p className="text-[12.5px] leading-[1.45] text-low">
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
    <div className="flex flex-col gap-1.5">
      <p className="text-[11px] font-bold uppercase tracking-[0.08em] text-low">
        {title}
      </p>
      {children}
    </div>
  );
}
