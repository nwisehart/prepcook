import { useTranslation } from 'react-i18next';
import { XIcon } from '@phosphor-icons/react';
import type { RunStep, ScriptRunState, StandUpReport } from 'shared/types';
import { RunningDots } from '@prepcook/ui/components/RunningDots';
import { cn } from '@/shared/lib/utils';
import {
  type LogEntry,
  VirtualizedProcessLogs,
} from '@/shared/components/VirtualizedProcessLogs';

interface ScriptRunPanelProps {
  step: RunStep;
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
    <div className="rounded-sm border border-border bg-secondary">
      <div className="flex items-center justify-between gap-base px-base py-half border-b border-border">
        <div className="flex items-center gap-half min-w-0">
          <span className="text-sm font-medium text-normal truncate">
            {t('settings.repos.scripts.run.title', { step })}
          </span>
          {isLive ? (
            <span className="flex items-center gap-half text-sm text-low">
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
            className="shrink-0 text-low hover:text-normal"
            aria-label={t('settings.repos.scripts.run.close')}
          >
            <XIcon className="size-icon-sm" />
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
        <p className="px-base py-half text-sm text-error border-t border-border">
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
        'inline-flex items-center gap-half rounded-sm px-half text-sm font-ibm-plex-mono',
        ok ? 'bg-success/10 text-success' : 'bg-error/10 text-error'
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
    <div className="border-t border-border px-base py-half space-y-half">
      <p className="text-sm text-normal">
        {t('settings.repos.scripts.run.outcome')}{' '}
        <span className="font-ibm-plex-mono">{report.outcome.kind}</span>
        {detail && <span className="text-low"> — {detail}</span>}
      </p>
      {report.message && (
        <p className="text-sm text-low whitespace-pre-wrap">{report.message}</p>
      )}
      {report.steps.length === 0 ? (
        <p className="text-sm text-low">
          {t('settings.repos.scripts.run.noSteps')}
        </p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-low">
              <th className="font-medium py-0.5 pr-base">
                {t('settings.repos.scripts.run.columns.step')}
              </th>
              <th className="font-medium py-0.5 pr-base">
                {t('settings.repos.scripts.run.columns.exit')}
              </th>
              <th className="font-medium py-0.5 pr-base">
                {t('settings.repos.scripts.run.columns.duration')}
              </th>
              <th className="font-medium py-0.5">
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
                  className={cn(failed ? 'text-error' : 'text-normal')}
                >
                  <td className="py-0.5 pr-base font-ibm-plex-mono">
                    {stepReport.step}
                  </td>
                  <td className="py-0.5 pr-base font-ibm-plex-mono">
                    {stepReport.exit === null
                      ? t('settings.repos.scripts.run.noExit')
                      : stepReport.exit}
                  </td>
                  <td className="py-0.5 pr-base">
                    {t('settings.repos.scripts.run.seconds', {
                      seconds: (stepReport.duration_ms / 1000).toFixed(1),
                    })}
                  </td>
                  <td className="py-0.5">
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
