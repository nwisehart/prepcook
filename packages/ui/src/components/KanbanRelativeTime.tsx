/** The `t` from `useTranslation('common')`, structurally. */
export type KanbanTranslate = (
  key: string,
  options?: Record<string, unknown>
) => string;

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function parse(value: string | null | undefined): number | null {
  if (!value) return null;
  const time = Date.parse(value);
  return Number.isNaN(time) ? null : time;
}

/**
 * "just now", "9 min ago", "2 h ago", "yesterday", "3 d ago", then a short
 * date ("Oct 2") past a week. Empty when the timestamp does not parse.
 */
export function formatRelativeTime(
  t: KanbanTranslate,
  iso: string | null | undefined,
  now: number = Date.now()
): string {
  const time = parse(iso);
  if (time === null) return '';
  const diff = Math.max(0, now - time);
  if (diff < MINUTE) return t('kanban.time.justNow');
  if (diff < HOUR) {
    return t('kanban.time.minutesAgo', { n: Math.floor(diff / MINUTE) });
  }
  if (diff < DAY) {
    return t('kanban.time.hoursAgo', { n: Math.floor(diff / HOUR) });
  }
  const days = Math.floor(diff / DAY);
  if (days === 1) return t('kanban.time.yesterday');
  if (days < 7) return t('kanban.time.daysAgo', { n: days });
  return new Date(time).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
  });
}

/** A running duration: "1 min", "12 min", "2 h", "3 d". */
export function formatElapsed(t: KanbanTranslate, ms: number): string {
  if (ms < HOUR) {
    return t('kanban.duration.minutes', {
      n: Math.max(1, Math.floor(ms / MINUTE)),
    });
  }
  if (ms < DAY) {
    return t('kanban.duration.hours', { n: Math.floor(ms / HOUR) });
  }
  return t('kanban.duration.days', { n: Math.floor(ms / DAY) });
}
