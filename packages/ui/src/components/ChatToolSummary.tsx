import type { ReactNode } from 'react';
import { cn } from '../lib/cn';
import { ChatStep, chatStepStateFromStatus } from './ChatStep';
import type { ToolStatusLike } from './ToolStatusDot';

interface ChatToolSummaryProps {
  /** What the step did, e.g. "Read a file" */
  label: string;
  /** The object of the step (path, query, command), muted under the label */
  detail?: ReactNode;
  /** Full text of a long detail, shown when the row is expanded */
  fullDetail?: string;
  time?: string;
  className?: string;
  expanded?: boolean;
  /** Toggles `fullDetail`; only used when there is no output to open */
  onToggle?: () => void;
  status?: ToolStatusLike;
  /** Opens the tool's output in the logs panel */
  onViewContent?: () => void;
  /** Extra content under the detail (e.g. a posted comment) */
  children?: ReactNode;
}

/** A single tool call rendered as a transcript step. */
export function ChatToolSummary({
  label,
  detail,
  fullDetail,
  time,
  className,
  expanded,
  onToggle,
  status,
  onViewContent,
  children,
}: ChatToolSummaryProps) {
  const canExpand = !onViewContent && Boolean(fullDetail && onToggle);

  return (
    <ChatStep
      state={chatStepStateFromStatus(status)}
      label={label}
      detail={canExpand && expanded ? undefined : detail}
      time={time}
      expanded={canExpand ? Boolean(expanded) : undefined}
      onActivate={onViewContent ?? (canExpand ? onToggle : undefined)}
      className={className}
    >
      {canExpand && expanded && (
        <pre
          className={cn(
            'mt-0.5 whitespace-pre-wrap break-all font-mono text-[12.5px] text-low'
          )}
        >
          {fullDetail}
        </pre>
      )}
      {children}
    </ChatStep>
  );
}
