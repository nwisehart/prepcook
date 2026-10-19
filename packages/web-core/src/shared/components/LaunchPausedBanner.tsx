import { useCallback, useState } from 'react';
import { HourglassIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { configApi } from '@/shared/lib/api';

/**
 * Warning notice shown while the Claude seat's usage window is exhausted
 * (`usage_limits` → /api/info `launch_pause_minutes`). Agent launches are
 * paused until the window reopens; nothing is broken and no action is needed.
 *
 * The action is offered anyway because the pause can outlive the limit: it is
 * armed partly from text matched in agent output, so a healthy seat can sit
 * behind one. The button probes the seat and, if it answers, ends the pause and
 * releases the pacing on sessions waiting to be sent back in. Sessions still
 * resume one at a time — the window has one seat behind it, and resuming a
 * stalled board all at once spends it again immediately.
 *
 * Distinct from {@link SeatDegradedBanner}, which means the seat's login has
 * expired and cannot serve anything until someone re-authenticates. Both stall
 * the board, so the board has to say which one it is: without this notice an
 * exhausted window looks exactly like a crashed orchestrator, and the only way
 * to tell them apart was reading the runner's log.
 */
export function LaunchPausedBanner({ minutes }: { minutes: number }) {
  const { t } = useTranslation('common');
  const { reloadSystem } = useUserSystem();
  const [checking, setChecking] = useState(false);
  const [queued, setQueued] = useState<number | null>(null);

  const handleResume = useCallback(async () => {
    setChecking(true);
    try {
      const status = await configApi.recheckClaudeSeat();
      // The count crosses as a bigint (i64 in the API type); i18n counts and
      // renders numbers.
      setQueued(Number(status.stalled_sessions));
      await reloadSystem();
    } catch (err) {
      console.error('Error re-checking Claude seat:', err);
    } finally {
      setChecking(false);
    }
  }, [reloadSystem]);

  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded border border-attention-border bg-attention-soft px-3 py-2 text-sm text-high"
    >
      <HourglassIcon className="size-4 shrink-0 text-attention-foreground" />
      <span className="min-w-0 flex-1">
        {t('launchPaused.banner', {
          minutes,
          defaultValue:
            'Claude usage window exhausted — agent launches resume in about {{minutes}} min.',
        })}
        {queued !== null && queued > 0 && (
          <span className="ml-1.5 text-normal">
            {t('launchPaused.queued', {
              count: queued,
              defaultValue:
                '{{count}} stalled session(s) queued — they go back in one at a time.',
            })}
          </span>
        )}
      </span>
      <button
        type="button"
        onClick={handleResume}
        disabled={checking}
        className="h-7 shrink-0 rounded border bg-secondary px-2.5 text-xs font-semibold text-high transition-colors hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
      >
        {checking
          ? t('launchPaused.checking', { defaultValue: 'Checking the seat…' })
          : t('launchPaused.resume', {
              defaultValue: 'Check the seat and resume',
            })}
      </button>
    </div>
  );
}
