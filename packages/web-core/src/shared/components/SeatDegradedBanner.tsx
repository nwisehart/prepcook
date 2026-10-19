import { WarningIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';

/**
 * Error notice shown when the periodic Claude seat probe last failed
 * (seat_monitor → /api/info `claude_seat_degraded`). The subscription login
 * has expired, so agents can't run until someone re-authenticates on the
 * runner (`docker exec -it prepcook claude /login`). Slack is
 * also alerted; this is the in-app counterpart so it's visible on the board.
 */
export function SeatDegradedBanner() {
  const { t } = useTranslation('common');
  return (
    <div
      role="alert"
      className="flex items-center gap-2 rounded border border-error/20 bg-error-soft px-3 py-2 text-sm text-high"
    >
      <WarningIcon className="size-4 shrink-0 text-error" />
      <span>
        {t('seatDegraded.banner', {
          defaultValue:
            "Claude seat expired — agents are paused until it's re-authenticated on the runner.",
        })}
      </span>
    </div>
  );
}
