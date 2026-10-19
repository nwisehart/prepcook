import { WarningCircleIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';

interface BoardNoticeProps {
  /** `no_secret`: a board URL but no secret; `no_board`: no board URL. */
  reason: 'no_secret' | 'no_board';
}

/**
 * Notice above the board/projects area while the server cannot authenticate
 * to a board. Nothing else waits on it; the fix is outside the app.
 */
export function BoardNotice({ reason }: BoardNoticeProps) {
  const { t } = useTranslation('common');
  // Text between backticks is a command or a variable and renders as code.
  const segments = (
    reason === 'no_board' ? t('boardNotice.noBoard') : t('boardNotice.noSecret')
  ).split('`');

  return (
    <div
      role="status"
      className="flex items-center gap-2 rounded border border-attention-border bg-attention-soft px-3 py-2 text-sm text-high"
    >
      <WarningCircleIcon className="size-4 shrink-0 text-attention-foreground" />
      <span>
        {segments.map((segment, index) =>
          index % 2 === 1 ? (
            <code key={index} className="font-mono text-xs font-medium">
              {segment}
            </code>
          ) : (
            <span key={index}>{segment}</span>
          )
        )}
      </span>
    </div>
  );
}
