import type { KeyboardEvent, ReactNode } from 'react';
import {
  CaretDownIcon,
  CheckIcon,
  HandIcon,
  ProhibitIcon,
  XIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { RunningDots } from './RunningDots';
import type { ToolStatusLike } from './ToolStatusDot';

export type ChatStepState = 'done' | 'running' | 'error' | 'denied' | 'waiting';

/** Maps a tool status to the mark a step row shows. */
export function chatStepStateFromStatus(
  status: ToolStatusLike | null | undefined
): ChatStepState {
  switch (status?.status) {
    case 'created':
      return 'running';
    case 'pending_approval':
      return 'waiting';
    case 'failed':
    case 'timed_out':
      return 'error';
    case 'denied':
      return 'denied';
    default:
      return 'done';
  }
}

/** The worst state among several steps, for a grouped row. */
export function worstChatStepState(states: ChatStepState[]): ChatStepState {
  const order: ChatStepState[] = [
    'error',
    'denied',
    'waiting',
    'running',
    'done',
  ];
  return order.find((state) => states.includes(state)) ?? 'done';
}

interface ChatStepMarkProps {
  state: ChatStepState;
  className?: string;
}

/** The 24px circle at the start of a step: check, running dots or a mark. */
export function ChatStepMark({ state, className }: ChatStepMarkProps) {
  return (
    <span
      className={cn(
        'inline-flex size-6 shrink-0 items-center justify-center rounded-full',
        state === 'error' || state === 'denied'
          ? 'bg-error-soft text-error'
          : state === 'waiting'
            ? 'bg-attention-soft text-attention-foreground'
            : state === 'running'
              ? 'bg-brand-soft'
              : 'bg-panel text-low',
        className
      )}
      aria-hidden
    >
      {state === 'done' && <CheckIcon className="size-3.5" weight="bold" />}
      {state === 'running' && <RunningDots dotClassName="size-1" />}
      {state === 'error' && <XIcon className="size-3.5" weight="bold" />}
      {state === 'denied' && (
        <ProhibitIcon className="size-3.5" weight="bold" />
      )}
      {state === 'waiting' && <HandIcon className="size-3.5" weight="bold" />}
    </span>
  );
}

interface ChatStepProps {
  state: ChatStepState;
  /** One line, 600 */
  label: ReactNode;
  /** Muted line under the label: counts, durations, mono file names */
  detail?: ReactNode;
  /** Right-aligned clock time */
  time?: string;
  /** Clicking the row: opens the step's output, or toggles `expanded` */
  onActivate?: () => void;
  /** When set, the row shows a caret and is a disclosure */
  expanded?: boolean;
  /** Inline controls after the label (e.g. "open in review") */
  actions?: ReactNode;
  /** Content below the detail line (expanded list, diff, comment box) */
  children?: ReactNode;
  className?: string;
}

/**
 * One quiet step in the transcript: a status mark, a label, a muted detail
 * line and the time it happened.
 */
export function ChatStep({
  state,
  label,
  detail,
  time,
  onActivate,
  expanded,
  actions,
  children,
  className,
}: ChatStepProps) {
  const isDisclosure = expanded !== undefined;
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!onActivate || event.target !== event.currentTarget) return;
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    onActivate();
  };

  return (
    <div className={cn('flex items-start gap-3 py-1.5', className)}>
      <ChatStepMark state={state} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-start gap-2">
          <div
            className={cn(
              'flex min-w-0 flex-1 flex-col gap-0.5 rounded-sm',
              onActivate && 'cursor-pointer',
              onActivate &&
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
            )}
            onClick={onActivate}
            onKeyDown={handleKeyDown}
            role={onActivate ? 'button' : undefined}
            tabIndex={onActivate ? 0 : undefined}
            aria-expanded={isDisclosure ? expanded : undefined}
            data-scroll-anchor-target={isDisclosure ? '' : undefined}
          >
            <span className="flex min-w-0 items-center gap-1.5 pt-0.5 text-sm font-semibold text-high">
              <span className="min-w-0 truncate">{label}</span>
              {isDisclosure && (
                <CaretDownIcon
                  className={cn(
                    'size-3 shrink-0 text-low transition-transform',
                    !expanded && '-rotate-90'
                  )}
                  weight="bold"
                  aria-hidden
                />
              )}
            </span>
            {detail && (
              <span className="min-w-0 truncate text-[12.5px] text-low">
                {detail}
              </span>
            )}
          </div>
          {actions && (
            <div className="flex shrink-0 items-center gap-1 pt-0.5">
              {actions}
            </div>
          )}
          {time && (
            <span className="shrink-0 pt-1 text-xs tabular-nums text-low">
              {time}
            </span>
          )}
        </div>
        {children}
      </div>
    </div>
  );
}

/** Joins detail-line parts with a middle dot. */
export function ChatStepDetail({ parts }: { parts: ReactNode[] }) {
  const visible = parts.filter(
    (part) => part !== null && part !== undefined && part !== false
  );
  return (
    <>
      {visible.map((part, index) => (
        <span key={index}>
          {index > 0 && <span aria-hidden> · </span>}
          {part}
        </span>
      ))}
    </>
  );
}

/**
 * A `REPRODUCTION:` / `REVIEW:` / `gate:` style comment: mono on a bordered
 * surface, the prefix in the attention colour.
 */
export function ChatPrefixedComment({
  prefix,
  body,
  className,
}: {
  prefix: string;
  body: string;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'max-w-[680px] whitespace-pre-wrap break-words rounded border bg-secondary px-3 py-2.5 font-mono text-[12.5px] text-high',
        className
      )}
    >
      <span className="font-medium text-attention-foreground">{prefix}</span>{' '}
      {body}
    </div>
  );
}
