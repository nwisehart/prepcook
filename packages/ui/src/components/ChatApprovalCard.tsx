import type { ReactNode } from 'react';
import { CaretDownIcon, ListChecksIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { Button } from './Button';

export interface ChatApprovalCardRenderProps {
  content: string;
  workspaceId?: string;
}

export interface ChatApprovalCardStatusLike {
  status: string;
}

/** The decision controls, present while the approval is pending. */
export interface ChatApprovalCardDecision {
  /** Helper line under the header */
  helper: string;
  /** Extra facts under the helper line (e.g. verdict chips) */
  meta?: ReactNode;
  note: string;
  onNoteChange: (note: string) => void;
  noteLabel: string;
  notePlaceholder: string;
  approveLabel: string;
  requestChangesLabel: string;
  onApprove: () => void;
  /** Sends the note back to the agent; disabled while the note is empty */
  onRequestChanges: () => void;
  isSubmitting?: boolean;
  error?: string | null;
  /** Link to the plan document (the RCA or spec), when there is one */
  documentLink?: { label: string; onClick: () => void } | null;
}

interface ChatApprovalCardProps {
  /** Header: "Your decision on the plan", or the tool asking for approval */
  title: string;
  content: string;
  /** Short name of what is being decided, shown on the collapsed row */
  contentTitle?: string;
  expanded?: boolean;
  onToggle?: () => void;
  className?: string;
  workspaceId?: string;
  status: ChatApprovalCardStatusLike;
  /** Chip naming the outcome once the decision is made (e.g. "Approved") */
  outcomeLabel?: string;
  decision?: ChatApprovalCardDecision | null;
  renderMarkdown: (props: ChatApprovalCardRenderProps) => ReactNode;
}

/**
 * A plan or tool call waiting on a person. While pending: an amber header,
 * the plan, an optional note and the decision buttons. Once decided: a quiet
 * card that folds the plan away behind its title and outcome.
 */
export function ChatApprovalCard({
  title,
  content,
  contentTitle,
  expanded = false,
  onToggle,
  className,
  workspaceId,
  status,
  outcomeLabel,
  decision,
  renderMarkdown,
}: ChatApprovalCardProps) {
  if (decision) {
    const canRequestChanges =
      decision.note.trim().length > 0 && !decision.isSubmitting;

    return (
      <section
        aria-label={title}
        className={cn(
          'my-2 ml-10 max-w-[680px] overflow-hidden rounded-lg border border-attention-border bg-secondary',
          className
        )}
      >
        <div className="flex items-center gap-2 bg-attention-soft px-3.5 py-2 text-[12.5px] font-bold text-attention-foreground">
          <span className="size-2 shrink-0 rounded-full bg-attention" />
          <span className="min-w-0 truncate">{title}</span>
        </div>
        <div className="flex flex-col gap-2.5 px-3.5 pb-3.5 pt-3">
          <span className="text-[13px] text-normal">{decision.helper}</span>
          {decision.meta && (
            <div className="flex flex-wrap items-center gap-2">
              {decision.meta}
            </div>
          )}
          {content.trim() && (
            <div className="max-h-[420px] overflow-y-auto rounded border border-divider bg-surface-raised px-3 py-2.5">
              {renderMarkdown({ content, workspaceId })}
            </div>
          )}
          <textarea
            rows={2}
            value={decision.note}
            onChange={(e) => decision.onNoteChange(e.target.value)}
            placeholder={decision.notePlaceholder}
            aria-label={decision.noteLabel}
            disabled={decision.isSubmitting}
            className={cn(
              'w-full resize-y rounded border bg-surface-raised px-2.5 py-2 text-sm text-high placeholder:text-low',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
              'disabled:opacity-60'
            )}
          />
          {decision.error && (
            <div
              role="alert"
              className="rounded bg-error-soft px-3 py-2 text-sm text-error"
            >
              {decision.error}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <Button
              onClick={decision.onApprove}
              disabled={decision.isSubmitting}
            >
              {decision.approveLabel}
            </Button>
            <Button
              variant="secondary"
              onClick={decision.onRequestChanges}
              disabled={!canRequestChanges}
            >
              {decision.requestChangesLabel}
            </Button>
            {decision.documentLink && (
              <button
                type="button"
                onClick={decision.documentLink.onClick}
                className={cn(
                  'ml-auto rounded-sm text-[13px] font-semibold text-brand-secondary hover:underline',
                  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
                )}
              >
                {decision.documentLink.label}
              </button>
            )}
          </div>
        </div>
      </section>
    );
  }

  const isDenied = status.status === 'denied';

  return (
    <section
      className={cn(
        'my-1.5 ml-10 max-w-[680px] overflow-hidden rounded-lg border bg-secondary',
        className
      )}
    >
      <button
        type="button"
        onClick={onToggle}
        disabled={!onToggle}
        aria-expanded={onToggle ? expanded : undefined}
        data-scroll-anchor-target=""
        className={cn(
          'flex w-full items-center gap-2 px-3.5 py-2.5 text-left disabled:cursor-default',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
        )}
      >
        <ListChecksIcon className="size-4 shrink-0 text-low" aria-hidden />
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-high">
          {contentTitle ?? title}
        </span>
        {outcomeLabel && (
          <span
            className={cn(
              'inline-flex h-[22px] shrink-0 items-center rounded-full px-2 text-xs font-medium',
              isDenied
                ? 'bg-error-soft text-error'
                : status.status === 'success'
                  ? 'bg-success-soft text-success'
                  : 'bg-panel text-normal'
            )}
          >
            {outcomeLabel}
          </span>
        )}
        {onToggle && (
          <CaretDownIcon
            className={cn(
              'size-3 shrink-0 text-low transition-transform',
              !expanded && '-rotate-90'
            )}
            weight="bold"
            aria-hidden
          />
        )}
      </button>
      {expanded && (
        <div className="border-t border-divider px-3.5 py-3">
          {renderMarkdown({ content, workspaceId })}
        </div>
      )}
    </section>
  );
}
