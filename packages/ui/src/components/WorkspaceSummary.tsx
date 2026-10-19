import { PushPinIcon, PlayIcon, DotsThreeIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';

export type WorkspaceSummaryStageTone = 'brand' | 'success' | 'neutral';

export type WorkspaceSummaryStatusTone =
  | 'attention'
  | 'working'
  | 'error'
  | 'success'
  | 'muted';

export interface WorkspaceSummaryProps {
  name: string;
  workspaceId?: string;
  /** Card key (e.g. "PAN-49"), or the branch when the session has no card */
  cardKey?: string;
  /** Column the card sits in, e.g. "Plan" */
  stageLabel?: string;
  stageTone?: WorkspaceSummaryStageTone;
  /** One-line status under the title */
  statusLabel?: string;
  statusTone?: WorkspaceSummaryStatusTone;
  isActive?: boolean;
  isPinned?: boolean;
  hasRunningDevServer?: boolean;
  /** Finished sessions read quieter: title in normal text */
  isMuted?: boolean;
  onClick?: () => void;
  className?: string;
  /** Whether this is a draft session (shows "Draft" as its status) */
  isDraft?: boolean;
  onOpenWorkspaceActions?: (workspaceId: string) => void;
}

const stageToneClass: Record<WorkspaceSummaryStageTone, string> = {
  brand: 'bg-brand-soft text-brand-soft-foreground',
  success: 'bg-success-soft text-success',
  neutral: 'bg-panel text-normal',
};

const statusTextClass: Record<WorkspaceSummaryStatusTone, string> = {
  attention: 'text-attention-foreground font-semibold',
  working: 'text-brand-secondary font-medium',
  error: 'text-error font-semibold',
  success: 'text-success font-semibold',
  muted: 'text-low',
};

const statusDotClass: Record<WorkspaceSummaryStatusTone, string | null> = {
  attention: 'bg-attention',
  working: 'bg-brand',
  error: 'bg-error',
  success: 'bg-success',
  muted: null,
};

/**
 * One session in the Sessions list: key and stage chip, the title on one
 * line, and a status line (dot + text) in the session's semantic colour.
 */
export function WorkspaceSummary({
  name,
  workspaceId,
  cardKey,
  stageLabel,
  stageTone = 'brand',
  statusLabel,
  statusTone = 'muted',
  isActive = false,
  isPinned = false,
  hasRunningDevServer = false,
  isMuted = false,
  onClick,
  className,
  isDraft = false,
  onOpenWorkspaceActions,
}: WorkspaceSummaryProps) {
  const { t } = useTranslation('common');
  const resolvedStatus = isDraft ? t('workspaces.draft') : statusLabel;
  const dotClass = isDraft ? null : statusDotClass[statusTone];
  const showKeyRow = Boolean(
    cardKey || stageLabel || isPinned || hasRunningDevServer
  );

  const handleOpenCommandBar = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!workspaceId || !onOpenWorkspaceActions) return;
    onOpenWorkspaceActions(workspaceId);
  };

  return (
    <div
      className={cn(
        'group relative rounded border transition-colors duration-100',
        isActive
          ? 'border-border bg-secondary'
          : 'border-transparent hover:bg-panel',
        className
      )}
    >
      <button
        type="button"
        onClick={onClick}
        aria-current={isActive ? 'true' : undefined}
        className={cn(
          'flex w-full min-w-0 cursor-pointer flex-col gap-[3px] rounded px-3 py-2.5 text-left',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
        )}
      >
        {showKeyRow && (
          <span className="flex min-w-0 items-center gap-1.5 pr-6">
            {cardKey && (
              <span className="min-w-0 truncate font-mono text-xs text-low">
                {cardKey}
              </span>
            )}
            {stageLabel && (
              <span
                className={cn(
                  'inline-flex h-[22px] shrink-0 items-center rounded-full px-2 text-xs font-medium leading-none',
                  stageToneClass[stageTone]
                )}
              >
                {stageLabel}
              </span>
            )}
            {hasRunningDevServer && (
              <PlayIcon
                className="size-3.5 shrink-0 text-brand"
                weight="fill"
                aria-hidden
              />
            )}
            {isPinned && (
              <PushPinIcon
                className="size-3.5 shrink-0 text-low"
                weight="fill"
                aria-hidden
              />
            )}
          </span>
        )}
        <span
          className={cn(
            'min-w-0 truncate text-sm font-semibold',
            !showKeyRow && 'pr-6',
            isMuted ? 'text-normal' : 'text-high'
          )}
        >
          {name}
        </span>
        {resolvedStatus && (
          <span
            className={cn(
              'flex min-w-0 items-center gap-1.5 text-xs',
              isDraft ? 'text-low' : statusTextClass[statusTone]
            )}
          >
            {dotClass && (
              <span
                className={cn('size-2 shrink-0 rounded-full', dotClass)}
                aria-hidden
              />
            )}
            <span className="min-w-0 truncate">{resolvedStatus}</span>
          </span>
        )}
      </button>

      {workspaceId && onOpenWorkspaceActions && (
        <div className="absolute right-1.5 top-1.5 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
          <button
            type="button"
            onClick={handleOpenCommandBar}
            onPointerDown={(e) => e.stopPropagation()}
            className={cn(
              'flex size-6 items-center justify-center rounded-sm text-low hover:bg-panel hover:text-high',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
            )}
            title={t('workspaces.more')}
            aria-label={t('workspaces.more')}
          >
            <DotsThreeIcon className="size-4" weight="bold" />
          </button>
        </div>
      )}
    </div>
  );
}
