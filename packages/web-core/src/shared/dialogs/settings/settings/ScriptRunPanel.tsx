import { useTranslation } from 'react-i18next';
import { XIcon } from '@phosphor-icons/react';
import type {
  ReproductionTarget,
  RunStep,
  ScriptRunState,
  StandUpReport,
} from 'shared/types';
import { RunningDots } from '@prepcook/ui/components/RunningDots';
import { cn } from '@/shared/lib/utils';
import {
  type LogEntry,
  VirtualizedProcessLogs,
} from '@/shared/components/VirtualizedProcessLogs';

interface ScriptRunPanelProps {
  step: RunStep;
  /** The reproduction target the run is on. */
  target: ReproductionTarget;
  state: ScriptRunState | null;
  stateError: unknown;
  logs: LogEntry[];
  logsError: string | null;
  isLive: boolean;
  onClose: () => void;
}

/** The output of one Settings test run, then its step results. */
export function ScriptRunPanel({
  step,
  target,
  state,
  stateError,
  logs,
  logsError,
  isLive,
  onClose,
}: ScriptRunPanelProps) {
  const { t } = useTranslation('settings');
  const report = state?.report ?? null;

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-secondary">
      <div className="flex items-center justify-between gap-3 border-b border-border px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-semibold text-high">
            {t('settings.repos.scripts.run.title', { step, target })}
          </span>
          {isLive ? (
            <span className="flex items-center gap-1.5 text-xs font-medium text-brand-secondary">
              <RunningDots />
              {t('settings.repos.scripts.run.running')}
            </span>
          ) : report ? (
            <OutcomeBadge report={report} />
          ) : null}
        </div>
        {!isLive && (
          <button
            type="button"
            onClick={onClose}
            className="shrink-0 rounded p-1 text-low hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
            aria-label={t('settings.repos.scripts.run.close')}
          >
            <XIcon className="size-4" />
          </button>
        )}
      </div>

      <div className="h-64 bg-primary">
        <VirtualizedProcessLogs
          logs={logs}
          error={logsError}
          searchQuery=""
          matchIndices={[]}
          currentMatchIndex={-1}
        />
      </div>

      {!isLive && stateError != null && !report && (
        <p className="border-t border-border px-3 py-2 text-[12.5px] text-error">
          {t('settings.repos.scripts.run.reportUnavailable')}
        </p>
      )}

      {report && <ScriptRunReport report={report} />}
    </div>
  );
}

function OutcomeBadge({ report }: { report: StandUpReport }) {
  const ok = report.outcome.kind === 'ok';
  return (
    <span
      className={cn(
        'inline-flex h-[22px] items-center rounded-full px-2 font-mono text-xs font-medium',
        ok ? 'bg-success-soft text-success' : 'bg-error-soft text-error'
      )}
    >
      {report.outcome.kind}
    </span>
  );
}

function ScriptRunReport({ report }: { report: StandUpReport }) {
  const { t } = useTranslation('settings');
  const detail = 'detail' in report.outcome ? report.outcome.detail : null;

  return (
    <div className="flex flex-col gap-1.5 border-t border-border px-3 py-2">
      <p className="text-sm text-normal">
        {t('settings.repos.scripts.run.outcome')}{' '}
        <span className="font-mono">{report.outcome.kind}</span>
        {detail && <span className="text-low"> — {detail}</span>}
      </p>
      {report.message && (
        <p className="whitespace-pre-wrap text-[12.5px] text-low">
          {report.message}
        </p>
      )}
      {report.steps.length === 0 ? (
        <p className="text-[12.5px] text-low">
          {t('settings.repos.scripts.run.noSteps')}
        </p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-low">
              <th className="py-1 pr-3 font-semibold">
                {t('settings.repos.scripts.run.columns.step')}
              </th>
              <th className="py-1 pr-3 font-semibold">
                {t('settings.repos.scripts.run.columns.exit')}
              </th>
              <th className="py-1 pr-3 font-semibold">
                {t('settings.repos.scripts.run.columns.duration')}
              </th>
              <th className="py-1 font-semibold">
                {t('settings.repos.scripts.run.columns.timedOut')}
              </th>
            </tr>
          </thead>
          <tbody>
            {report.steps.map((stepReport, index) => {
              const failed =
                stepReport.timed_out ||
                (stepReport.exit !== null && stepReport.exit !== 0);
              return (
                <tr
                  key={`${stepReport.step}-${index}`}
                  className={cn(
                    'border-t border-divider',
                    failed ? 'text-error' : 'text-normal'
                  )}
                >
                  <td className="py-1 pr-3 font-mono text-[12.5px]">
                    {stepReport.step}
                  </td>
                  <td className="py-1 pr-3 font-mono text-[12.5px]">
                    {stepReport.exit === null
                      ? t('settings.repos.scripts.run.noExit')
                      : stepReport.exit}
                  </td>
                  <td className="py-1 pr-3">
                    {t('settings.repos.scripts.run.seconds', {
                      seconds: (stepReport.duration_ms / 1000).toFixed(1),
                    })}
                  </td>
                  <td className="py-1">
                    {stepReport.timed_out
                      ? t('settings.repos.scripts.run.yes')
                      : t('settings.repos.scripts.run.no')}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
