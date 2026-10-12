import { WarningIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';

/**
 * Warning strip shown when the periodic Claude seat probe last failed
 * (seat_monitor → /api/info `claude_seat_degraded`). The subscription login
 * has expired, so agents can't run until someone re-authenticates on the
 * runner (`docker exec -it prepcook claude /login`). Slack is
 * also alerted; this is the in-app counterpart so it's visible on the board.
 */
export function SeatDegradedBanner() {
  const { t } = useTranslation('common');
  return (
    <div className="flex items-center justify-center gap-2 bg-warning/15 text-warning px-base py-1 text-sm font-medium border-b border-warning/30">
      <WarningIcon className="size-icon-xs shrink-0" weight="bold" />
      <span>
        {t('seatDegraded.banner', {
          defaultValue:
            "Claude seat expired — agents are paused until it's re-authenticated on the runner.",
        })}
      </span>
    </div>
  );
}
