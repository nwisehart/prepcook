import { cn } from '../lib/cn';
import { useTranslation } from 'react-i18next';
import {
  DotsThreeIcon,
  LinkBreakIcon,
  TrashIcon,
  PlayIcon,
  PlusIcon,
  StampIcon,
} from '@phosphor-icons/react';
import { UserAvatar, type UserAvatarUser } from './UserAvatar';
import { formatRelativeTime } from './KanbanRelativeTime';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './DropdownMenu';

export interface WorkspacePr {
  number: number;
  url: string;
  status: 'open' | 'merged' | 'closed';
}

export interface WorkspaceWithStats {
  id: string;
  localWorkspaceId: string | null;
  name: string | null;
  archived: boolean;
  filesChanged: number;
  linesAdded: number;
  linesRemoved: number;
  prs: WorkspacePr[];
  owner: UserAvatarUser | null;
  updatedAt: string;
  isOwnedByCurrentUser: boolean;
  isRunning?: boolean;
  hasPendingApproval?: boolean;
  hasRunningDevServer?: boolean;
  hasUnseenActivity?: boolean;
  latestProcessCompletedAt?: string;
  latestProcessStatus?: 'running' | 'completed' | 'failed' | 'killed';
  /** The stage the session works for ("Plan", "Review"), as a chip. */
  stageLabel?: string | null;
}

export interface IssueWorkspaceCardProps {
  workspace: WorkspaceWithStats;
  onClick?: () => void;
  onUnlink?: () => void;
  onDelete?: () => void;
  showOwner?: boolean;
  showStatusBadge?: boolean;
  showNoPrText?: boolean;
  /** Live lavish-axi review session URL — renders an Open Review chip. */
  reviewUrl?: string | null;
  /**
   * What "agent turn done" means for this ticket's column: 'review' (work
   * executed, ready to move to Review) vs generic 'input' (idle until a
   * person replies).
   */
  readyVariant?: 'input' | 'review';
  className?: string;
}

export interface IssueWorkspaceCreateCardProps {
  onClick?: () => void;
  className?: string;
  shouldAnimateCreateButton?: boolean;
}

type SessionState = 'archived' | 'waiting' | 'running' | 'failed' | 'ready';

const STATE_STYLES: Record<SessionState, { dot: string; text: string }> = {
  archived: { dot: 'bg-low/50', text: 'text-low font-medium' },
  waiting: {
    dot: 'bg-attention',
    text: 'text-attention-foreground font-semibold',
  },
  running: { dot: 'bg-brand', text: 'text-brand-secondary font-medium' },
  failed: { dot: 'bg-error', text: 'text-error font-medium' },
  ready: { dot: 'bg-low/60', text: 'text-low font-medium' },
};

/**
 * One session on a card, as a row: stage chip, the session's name over its
 * meta line (when it last moved, its diff, its pull requests), and its
 * state on the right.
 */
export function IssueWorkspaceCard({
  workspace,
  onClick,
  onUnlink,
  onDelete,
  showOwner = true,
  showStatusBadge = true,
  showNoPrText = true,
  reviewUrl,
  readyVariant = 'input',
  className,
}: IssueWorkspaceCardProps) {
  const { t } = useTranslation('common');
  const timeAgo = formatRelativeTime(
    t,
    workspace.latestProcessCompletedAt ?? workspace.updatedAt
  );
  const isRunning = workspace.isRunning ?? false;
  const isFailed =
    workspace.latestProcessStatus === 'failed' ||
    workspace.latestProcessStatus === 'killed';

  const state: SessionState = workspace.archived
    ? 'archived'
    : isRunning && workspace.hasPendingApproval
      ? 'waiting'
      : isRunning
        ? 'running'
        : isFailed
          ? 'failed'
          : 'ready';
  const stateLabel =
    state === 'archived'
      ? t('workspaces.archived')
      : state === 'waiting'
        ? t('kanban.sessions.waiting')
        : state === 'running'
          ? t('workspaces.running')
          : state === 'failed'
            ? t('kanban.sessions.failed')
            : readyVariant === 'review' &&
                workspace.latestProcessStatus === 'completed'
              ? t('workspaces.readyForReview')
              : t('workspaces.idle');
  // A waiting session always says so: it is the signal to act.
  const showState = showStatusBadge || state === 'waiting';

  return (
    <div
      className={cn(
        'flex items-center gap-2.5 rounded border border-border bg-secondary px-3 py-2.5 transition-colors',
        onClick && 'cursor-pointer hover:bg-panel/60',
        className
      )}
      onClick={
        onClick
          ? (e) => {
              e.stopPropagation();
              onClick();
            }
          : undefined
      }
      role={onClick ? 'button' : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                e.stopPropagation();
                onClick();
              }
            }
          : undefined
      }
    >
      {workspace.stageLabel && (
        <span className="inline-flex h-[22px] shrink-0 items-center rounded-full bg-brand-soft px-2 text-xs font-medium text-brand-soft-foreground">
          {workspace.stageLabel}
        </span>
      )}

      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[13px] font-semibold text-high">
          {workspace.name ?? t('kanban.sessions.untitled')}
        </span>
        <span className="flex min-w-0 flex-wrap items-center gap-x-1.5 text-xs text-low">
          {workspace.hasRunningDevServer && (
            <PlayIcon
              className="size-3 shrink-0 text-brand"
              weight="fill"
              aria-label={t('kanban.sessions.devServer')}
            />
          )}
          <span className="whitespace-nowrap">{timeAgo}</span>
          {workspace.filesChanged > 0 && (
            <>
              <span aria-hidden="true">·</span>
              <span className="inline-flex items-center gap-1.5 whitespace-nowrap font-mono">
                <span>
                  {t('kanban.card.files', { count: workspace.filesChanged })}
                </span>
                <span className="text-success">+{workspace.linesAdded}</span>
                <span className="text-error">−{workspace.linesRemoved}</span>
              </span>
            </>
          )}
          {workspace.prs.map((pr) => (
            <span key={pr.number} className="flex items-center gap-1.5">
              <span aria-hidden="true">·</span>
              <a
                href={pr.url}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
                className={cn(
                  'whitespace-nowrap hover:underline',
                  pr.status === 'merged'
                    ? 'text-success'
                    : pr.status === 'closed'
                      ? 'text-low'
                      : 'text-brand-secondary'
                )}
              >
                {t('kanban.card.pullRequest', { number: pr.number })}
              </a>
            </span>
          ))}
          {workspace.prs.length === 0 && showNoPrText && (
            <>
              <span aria-hidden="true">·</span>
              <span className="whitespace-nowrap">
                {t('kanban.noPrCreated')}
              </span>
            </>
          )}
        </span>
      </span>

      {reviewUrl && (
        <a
          href={reviewUrl}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.stopPropagation()}
          className="inline-flex h-[22px] shrink-0 items-center gap-1 rounded-full bg-brand-soft px-2 text-xs font-medium text-brand-soft-foreground hover:opacity-80"
          title={t('workspaces.openSpecReview')}
        >
          <StampIcon className="size-3" weight="bold" />
          {t('workspaces.review')}
        </a>
      )}

      {showState && (
        <span
          className={cn(
            'inline-flex shrink-0 items-center gap-1.5 text-xs',
            STATE_STYLES[state].text
          )}
        >
          <span
            aria-hidden="true"
            className={cn(
              'size-2 rounded-full',
              STATE_STYLES[state].dot,
              state === 'running' && 'animate-pulse'
            )}
          />
          {stateLabel}
        </span>
      )}

      {showOwner && workspace.owner && (
        <UserAvatar user={workspace.owner} className="size-5 text-[10px]" />
      )}

      {(onUnlink || onDelete) && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              onClick={(e) => e.stopPropagation()}
              className="flex size-6 shrink-0 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high"
              aria-label={t('workspaces.more')}
            >
              <DotsThreeIcon className="size-icon-xs" weight="bold" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {onUnlink && (
              <DropdownMenuItem
                onClick={(e) => {
                  e.stopPropagation();
                  onUnlink();
                }}
              >
                <LinkBreakIcon className="size-icon-xs" />
                {t('workspaces.unlinkFromIssue')}
              </DropdownMenuItem>
            )}
            {onDelete && (
              <DropdownMenuItem
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete();
                }}
                className="text-error focus:text-error"
              >
                <TrashIcon className="size-icon-xs" />
                {t('workspaces.deleteWorkspace')}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

/** The empty state of the sessions list: start the card's first session. */
export function IssueWorkspaceCreateCard({
  onClick,
  className,
  shouldAnimateCreateButton = false,
}: IssueWorkspaceCreateCardProps) {
  const { t } = useTranslation('common');

  return (
    <div
      className={cn(
        'flex items-center justify-between gap-3 rounded border border-dashed border-border px-3 py-2.5',
        className
      )}
    >
      <span className="truncate text-[13px] text-low">
        {t('kanban.sessions.none')}
      </span>
      <button
        type="button"
        onClick={onClick}
        disabled={!onClick}
        className={cn(
          'inline-flex h-7 shrink-0 items-center gap-1.5 rounded border border-border bg-secondary px-2.5 text-[13px] font-semibold text-high transition-colors hover:bg-panel disabled:cursor-not-allowed disabled:opacity-50',
          shouldAnimateCreateButton && 'create-issue-attention'
        )}
      >
        <PlusIcon className="size-3.5" weight="bold" />
        {t('kanban.sessions.newSession')}
      </button>
    </div>
  );
}
