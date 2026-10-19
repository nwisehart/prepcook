'use client';

import type {
  MouseEvent,
  PointerEvent,
  ReactNode,
  SyntheticEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import {
  ClockIcon,
  DotsThreeIcon,
  GitPullRequestIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { KanbanBadge } from './KanbanBadge';
import { KanbanAssignee, type KanbanAssigneeUser } from './KanbanAssignee';
import { Button } from './Button';
import {
  RelationshipBadge,
  type RelationshipDisplayType,
} from './RelationshipBadge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './Dropdown';

export interface KanbanTag {
  id: string;
  name: string;
  color: string;
}

export interface KanbanRelationship {
  relationshipId: string;
  displayType: RelationshipDisplayType;
  relatedIssueDisplayId: string;
}

/** Chip recipes for a card's work class (see the design system). */
export type KanbanClassTone = 'bug' | 'story' | 'epic' | 'neutral';

export const KANBAN_CLASS_CHIP_STYLES: Record<KanbanClassTone, string> = {
  bug: 'bg-error-soft text-error',
  story: 'bg-brand-soft text-brand-soft-foreground',
  epic: 'bg-merged/10 text-merged',
  neutral: 'bg-panel text-normal',
};

/** The card's work class as a chip, with actions behind it. */
export interface KanbanClassLabel {
  /** The class, as shown ("investigation"). */
  name: string;
  tone: KanbanClassTone;
  /** Opened from the chip when present (e.g. "Convert to bug"). */
  actions?: { label: string; onSelect: () => void }[];
}

/**
 * A state chip in the card's top row: the Shipped column's pull-request state
 * ("PR open", "CI failing", "Merged", "PR closed, not merged").
 */
export type KanbanCardIndicatorTone = 'brand' | 'error' | 'success' | 'neutral';

export interface KanbanCardIndicator {
  label: string;
  tone: KanbanCardIndicatorTone;
  /** Tooltip with the longer explanation. */
  title?: string;
}

const INDICATOR_CHIP_STYLES: Record<KanbanCardIndicatorTone, string> = {
  brand: 'bg-brand-soft text-brand-soft-foreground',
  error: 'bg-error-soft text-error',
  success: 'bg-success-soft text-success',
  neutral: 'bg-panel text-normal',
};

/** Who filed the card from outside the board: "Filed by Tammy from thread". */
export interface KanbanCardOrigin {
  /** "Filed by Tammy from". */
  text: string;
  /** The link's text ("thread"), or the reference itself when not a URL. */
  linkText: string;
  url?: string | null;
}

/**
 * The card's one status line. `attention` is amber (waits on a person),
 * `working` brand (an agent is on it), `idle` muted, `merged` green,
 * `error` red.
 */
export type KanbanCardStatusTone =
  | 'attention'
  | 'working'
  | 'idle'
  | 'merged'
  | 'error';

export interface KanbanCardStatusLine {
  tone: KanbanCardStatusTone;
  text: string;
  /** A link rendered before (`start`) or after (`end`) the text. */
  link?: { label: string; url: string } | null;
  linkAt?: 'start' | 'end';
  /** Idle lines lead with a 14px icon instead of a dot. */
  icon?: 'clock' | 'pullRequest';
  /** Tooltip with the longer explanation. */
  title?: string;
  /** Pulsing dot for a state about to change (an agent starting). */
  pulse?: boolean;
}

export interface KanbanCardAction {
  label: string;
  onClick: () => void;
  variant: 'primary' | 'secondary';
  disabled?: boolean;
  title?: string;
}

/** The amber "Needs you" strip and the inline decision actions. */
export interface KanbanCardNeedsYou {
  /** "Needs you · the RCA is ready". */
  label: string;
  /** One muted line under the title ("Confidence high · no open questions"). */
  detail?: string | null;
  actions: KanbanCardAction[];
}

export interface KanbanCardDiffStats {
  filesChanged: number;
  linesAdded: number;
  linesRemoved: number;
}

export interface TagEditRenderProps<TTag extends KanbanTag = KanbanTag> {
  allTags: TTag[];
  selectedTagIds: string[];
  onTagToggle: (tagId: string) => void;
  onCreateTag: (data: { name: string; color: string }) => string;
  trigger: ReactNode;
}

export interface TagEditProps<TTag extends KanbanTag = KanbanTag> {
  allTags: TTag[];
  selectedTagIds: string[];
  onTagToggle: (tagId: string) => void;
  onCreateTag: (data: { name: string; color: string }) => string;
  renderTagEditor?: (props: TagEditRenderProps<TTag>) => ReactNode;
}

export type KanbanCardContentProps<TTag extends KanbanTag = KanbanTag> = {
  /** The tracker key when the card has one, else its board id. */
  displayId: string;
  title: string;
  /** "P0"–"P3", or null for no priority. */
  priorityLabel?: string | null;
  classLabel?: KanbanClassLabel | null;
  /** A person took the ticket over: automation is hands-off. */
  takenOverLabel?: string | null;
  /** The card's state chip, after the class chip. */
  indicator?: KanbanCardIndicator | null;
  statusLine?: KanbanCardStatusLine | null;
  needsYou?: KanbanCardNeedsYou | null;
  diffStats?: KanbanCardDiffStats | null;
  tags: KanbanTag[];
  assignees: KanbanAssigneeUser[];
  relationships?: KanbanRelationship[];
  isSubIssue?: boolean;
  className?: string;
  onPriorityClick?: (e: MouseEvent) => void;
  onAssigneeClick?: (e: MouseEvent) => void;
  onMoreActionsClick?: () => void;
  tagEditProps?: TagEditProps<TTag>;
  isMobile?: boolean;
  /** Jira key of the parent epic (subtask cards): renders an epic chip. */
  epicKey?: string | null;
  /** Deterministic accent color derived from the epic key. */
  epicColor?: string;
  /** Subtask completion for epic cards: renders a progress bar. */
  epicProgress?: { done: number; total: number };
};

/**
 * Stops a control's pointer events from reaching the card underneath, which
 * opens the card panel on mouse-up and starts a drag on mouse-down.
 */
export const stopCardEvents = {
  onClick: (e: SyntheticEvent) => e.stopPropagation(),
  onMouseDown: (e: SyntheticEvent) => e.stopPropagation(),
  onMouseUp: (e: SyntheticEvent) => e.stopPropagation(),
  onPointerDown: (e: PointerEvent) => e.stopPropagation(),
};

const CHIP =
  'inline-flex h-[22px] shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full px-2 text-xs font-medium leading-none';

function ClassChip({ label }: { label: KanbanClassLabel }) {
  const chipClassName = cn(CHIP, KANBAN_CLASS_CHIP_STYLES[label.tone]);
  if (!label.actions || label.actions.length === 0) {
    return <span className={chipClassName}>{label.name}</span>;
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          {...stopCardEvents}
          className={cn(chipClassName, 'cursor-pointer hover:opacity-80')}
        >
          {label.name}
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" onClick={(e) => e.stopPropagation()}>
        {label.actions.map((action) => (
          <DropdownMenuItem
            key={action.label}
            onClick={(e) => {
              e.stopPropagation();
              action.onSelect();
            }}
          >
            {action.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const STATUS_TEXT: Record<KanbanCardStatusTone, string> = {
  attention: 'text-attention-foreground font-semibold',
  working: 'text-brand-secondary font-medium',
  idle: 'text-low',
  merged: 'text-success font-semibold',
  error: 'text-error font-medium',
};

const STATUS_DOT: Record<KanbanCardStatusTone, string> = {
  attention: 'bg-attention',
  working: 'bg-brand',
  idle: 'bg-low/60',
  merged: 'bg-success',
  error: 'bg-error',
};

export function KanbanCardStatus({
  line,
  className,
}: {
  line: KanbanCardStatusLine;
  className?: string;
}) {
  const Icon =
    line.icon === 'pullRequest'
      ? GitPullRequestIcon
      : line.icon === 'clock'
        ? ClockIcon
        : null;
  const link = line.link ? (
    <a
      href={line.link.url}
      target="_blank"
      rel="noopener noreferrer"
      {...stopCardEvents}
      className={cn(
        'shrink-0 hover:underline',
        line.tone === 'merged'
          ? 'text-success'
          : line.tone === 'error'
            ? 'text-error'
            : 'text-brand-secondary'
      )}
    >
      {line.link.label}
    </a>
  ) : null;
  const linkAtStart = line.linkAt !== 'end';

  return (
    <div
      className={cn(
        'flex min-w-0 items-center gap-2 text-xs',
        STATUS_TEXT[line.tone],
        className
      )}
      title={line.title}
    >
      {Icon ? (
        <Icon className="size-3.5 shrink-0 text-low" aria-hidden="true" />
      ) : (
        <span
          aria-hidden="true"
          className={cn(
            'size-2 shrink-0 rounded-full',
            STATUS_DOT[line.tone],
            line.pulse && 'animate-pulse'
          )}
        />
      )}
      <span className="flex min-w-0 items-center gap-1">
        {linkAtStart && link}
        {line.text && <span className="truncate">{line.text}</span>}
        {!linkAtStart && link}
      </span>
    </div>
  );
}

/** "3 files +41 −7" in mono, additions green and deletions red. */
export function KanbanDiffStats({
  stats,
  filesLabel,
  className,
}: {
  stats: KanbanCardDiffStats;
  filesLabel: string;
  className?: string;
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 font-mono text-xs text-low',
        className
      )}
    >
      <span>{filesLabel}</span>
      <span className="text-success">+{stats.linesAdded}</span>
      <span className="text-error">−{stats.linesRemoved}</span>
    </span>
  );
}

export function KanbanCardContent<TTag extends KanbanTag = KanbanTag>({
  displayId,
  title,
  priorityLabel,
  classLabel,
  takenOverLabel,
  indicator,
  statusLine,
  needsYou,
  diffStats,
  tags,
  assignees,
  relationships = [],
  isSubIssue,
  className,
  onPriorityClick,
  onAssigneeClick,
  onMoreActionsClick,
  tagEditProps,
  isMobile,
  epicKey,
  epicColor,
  epicProgress,
}: KanbanCardContentProps<TTag>) {
  const { t } = useTranslation('common');

  const hasDiff = !!diffStats && diffStats.filesChanged > 0;
  const visibleTags = tags.slice(0, 2);
  const tagChips =
    tags.length > 0 ? (
      <>
        {visibleTags.map((tag) => (
          <KanbanBadge key={tag.id} name={tag.name} color={tag.color} />
        ))}
        {tags.length > 2 && (
          <span className="text-xs text-low">+{tags.length - 2}</span>
        )}
      </>
    ) : null;
  const tagEditorTrigger = (
    <button
      type="button"
      {...stopCardEvents}
      className="flex min-w-0 items-center gap-1 rounded-full transition-opacity hover:opacity-80"
    >
      {tagChips}
    </button>
  );
  const tagsNode =
    tags.length === 0
      ? null
      : tagEditProps
        ? (tagEditProps.renderTagEditor?.({
            allTags: tagEditProps.allTags,
            selectedTagIds: tagEditProps.selectedTagIds,
            onTagToggle: tagEditProps.onTagToggle,
            onCreateTag: tagEditProps.onCreateTag,
            trigger: tagEditorTrigger,
          }) ?? tagEditorTrigger)
        : tagChips;
  const showFooter =
    hasDiff ||
    tags.length > 0 ||
    relationships.length > 0 ||
    assignees.length > 0;

  const priorityChip = priorityLabel ? (
    onPriorityClick ? (
      <button
        type="button"
        onClick={onPriorityClick}
        onMouseDown={(e) => e.stopPropagation()}
        onMouseUp={(e) => e.stopPropagation()}
        className={cn(CHIP, 'bg-panel text-normal hover:text-high')}
      >
        {priorityLabel}
      </button>
    ) : (
      <span className={cn(CHIP, 'bg-panel text-normal')}>{priorityLabel}</span>
    )
  ) : null;

  const body = (
    <>
      {/* Row 1: key, class and takeover chips; priority pushed right */}
      <div className="flex min-w-0 items-center gap-1.5">
        {isSubIssue && (
          <span className="text-xs text-low" aria-hidden="true">
            {t('kanban.subIssueIndicator')}
          </span>
        )}
        <span className="shrink-0 font-mono text-xs text-low">{displayId}</span>
        {classLabel && <ClassChip label={classLabel} />}
        {epicKey && (
          <span
            className={cn(CHIP, 'border border-border text-low')}
            title={epicKey}
          >
            <span
              className="size-1.5 shrink-0 rounded-full"
              style={epicColor ? { backgroundColor: epicColor } : undefined}
            />
            {epicKey}
          </span>
        )}
        {indicator && (
          <span
            className={cn(CHIP, INDICATOR_CHIP_STYLES[indicator.tone])}
            title={indicator.title}
          >
            {indicator.label}
          </span>
        )}
        {takenOverLabel && (
          <span
            className="inline-flex h-[22px] min-w-0 items-center rounded-full bg-panel px-2 text-xs font-medium leading-none text-normal"
            title={takenOverLabel}
          >
            <span className="truncate">{takenOverLabel}</span>
          </span>
        )}
        <span className="ml-auto flex shrink-0 items-center gap-1">
          {priorityChip}
          {onMoreActionsClick && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onMoreActionsClick();
              }}
              onMouseDown={(e) => e.stopPropagation()}
              onMouseUp={(e) => e.stopPropagation()}
              className={cn(
                'flex size-[22px] items-center justify-center rounded-full text-low hover:bg-panel hover:text-high',
                isMobile
                  ? ''
                  : 'hidden group-hover:flex group-focus-within:flex',
                'transition-colors'
              )}
              aria-label={t('kanban.moreActions')}
              title={t('kanban.moreActions')}
            >
              <DotsThreeIcon className="size-icon-xs" weight="bold" />
            </button>
          )}
        </span>
      </div>

      {/* Row 2: title */}
      <span className="line-clamp-3 text-sm font-semibold text-high">
        {title}
      </span>

      {/* Row 3: the one status line, or the decision detail */}
      {needsYou?.detail && (
        <span className="text-xs text-low">{needsYou.detail}</span>
      )}
      {statusLine && <KanbanCardStatus line={statusLine} />}

      {epicProgress && epicProgress.total > 0 && (
        <div className="flex items-center gap-2">
          <div className="h-1 flex-1 overflow-hidden rounded-full bg-panel">
            <div
              className="h-full rounded-full bg-brand transition-[width]"
              style={{
                width: `${Math.round((epicProgress.done / epicProgress.total) * 100)}%`,
              }}
            />
          </div>
          <span className="shrink-0 text-xs text-low">
            {epicProgress.done}/{epicProgress.total}
          </span>
        </div>
      )}

      {needsYou && needsYou.actions.length > 0 && (
        <div className="flex gap-2 pt-0.5">
          {needsYou.actions.map((action) => (
            <Button
              key={action.label}
              type="button"
              variant={action.variant === 'primary' ? 'default' : 'outline'}
              disabled={action.disabled}
              title={action.title}
              className="flex-1"
              {...stopCardEvents}
              onClick={(e) => {
                e.stopPropagation();
                action.onClick();
              }}
            >
              {action.label}
            </Button>
          ))}
        </div>
      )}

      {/* Footer: diff stats, tags, links; the assignee on the right */}
      {showFooter && (
        <div className="flex min-w-0 items-center justify-between gap-2 border-t border-divider pt-2">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            {hasDiff && diffStats && (
              <KanbanDiffStats
                stats={diffStats}
                filesLabel={t('kanban.card.files', {
                  count: diffStats.filesChanged,
                })}
              />
            )}
            {tagsNode}
            {relationships.slice(0, 2).map((rel) => (
              <RelationshipBadge
                key={rel.relationshipId}
                displayType={rel.displayType}
                relatedIssueDisplayId={rel.relatedIssueDisplayId}
                compact
              />
            ))}
            {relationships.length > 2 && (
              <span className="text-xs text-low">
                +{relationships.length - 2}
              </span>
            )}
          </div>
          {assignees.length > 0 &&
            (onAssigneeClick ? (
              <button
                type="button"
                onClick={onAssigneeClick}
                onMouseDown={(e) => e.stopPropagation()}
                onMouseUp={(e) => e.stopPropagation()}
                className="shrink-0 rounded-full transition-opacity hover:opacity-80"
              >
                <KanbanAssignee assignees={assignees} />
              </button>
            ) : (
              <KanbanAssignee assignees={assignees} className="shrink-0" />
            ))}
        </div>
      )}
    </>
  );

  if (needsYou) {
    return (
      <div className={cn('flex min-w-0 flex-col', className)}>
        <div className="flex items-center gap-2 bg-attention-soft px-3.5 py-2 text-xs font-bold text-attention-foreground">
          <span
            aria-hidden="true"
            className="size-2 shrink-0 rounded-full bg-attention"
          />
          <span className="min-w-0">{needsYou.label}</span>
        </div>
        <div className="flex min-w-0 flex-col gap-2 px-3.5 pb-3 pt-2.5">
          {body}
        </div>
      </div>
    );
  }

  return (
    <div className={cn('flex min-w-0 flex-col gap-2 px-3.5 py-3', className)}>
      {body}
    </div>
  );
}

export interface KanbanArchivedCardContentProps {
  displayId: string;
  title: string;
  classLabel?: KanbanClassLabel | null;
  /** When it was cleaned up: "2 d ago", "Oct 2". */
  when: string;
  links: { label: string; url: string }[];
  className?: string;
}

/** A cleaned-up card: key, title, when it was cleaned up and its links. */
export function KanbanArchivedCardContent({
  displayId,
  title,
  classLabel,
  when,
  links,
  className,
}: KanbanArchivedCardContentProps) {
  return (
    <div className={cn('flex min-w-0 flex-col gap-1.5 px-3 py-2.5', className)}>
      <div className="flex min-w-0 items-center gap-1.5">
        <span className="truncate font-mono text-xs text-low">{displayId}</span>
        {classLabel && (
          <span className={cn(CHIP, 'bg-panel text-normal')}>
            {classLabel.name}
          </span>
        )}
      </div>
      <span className="line-clamp-2 text-[13px] font-semibold text-normal">
        {title}
      </span>
      <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs text-low">
        <span>{when}</span>
        {links.map((link) => (
          <span key={link.url} className="flex items-center gap-1.5">
            <span aria-hidden="true">·</span>
            <a
              href={link.url}
              target="_blank"
              rel="noopener noreferrer"
              {...stopCardEvents}
              className="text-brand-secondary hover:underline"
            >
              {link.label}
            </a>
          </span>
        ))}
      </div>
    </div>
  );
}
