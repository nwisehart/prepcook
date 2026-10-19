import { cn } from '../lib/cn';
import {
  ChatStep,
  chatStepStateFromStatus,
  worstChatStepState,
} from './ChatStep';
import { ToolStatusDot, type ToolStatusLike } from './ToolStatusDot';

export interface AggregatedEntry {
  summary: string;
  status?: ToolStatusLike;
  expansionKey: string;
  content?: string;
  command?: string;
}

interface ChatAggregatedToolEntriesProps {
  entries: AggregatedEntry[];
  expanded: boolean;
  onToggle: () => void;
  /** Step label, e.g. "Read 5 files" */
  label: string;
  /** Muted line under the label, e.g. the first few paths */
  detail?: React.ReactNode;
  time?: string;
  className?: string;
  onViewContent?: (index: number) => void;
}

/**
 * Consecutive reads, searches or fetches as one step. A single entry is a
 * plain step; several are a disclosure listing each one, and each listed
 * entry opens its own output.
 */
export function ChatAggregatedToolEntries({
  entries,
  expanded,
  onToggle,
  label,
  detail,
  time,
  className,
  onViewContent,
}: ChatAggregatedToolEntriesProps) {
  if (entries.length === 0) return null;

  if (entries.length === 1) {
    return (
      <ChatStep
        state={chatStepStateFromStatus(entries[0].status)}
        label={label}
        detail={detail}
        time={time}
        onActivate={onViewContent ? () => onViewContent(0) : undefined}
        className={className}
      />
    );
  }

  const state = worstChatStepState(
    entries.map((entry) => chatStepStateFromStatus(entry.status))
  );

  return (
    <ChatStep
      state={state}
      label={label}
      detail={expanded ? undefined : detail}
      time={time}
      expanded={expanded}
      onActivate={onToggle}
      className={className}
    >
      {expanded && (
        <ul className="mt-0.5 flex flex-col">
          {entries.map((entry, index) => (
            <li key={entry.expansionKey} className="min-w-0">
              <button
                type="button"
                onClick={onViewContent ? () => onViewContent(index) : undefined}
                disabled={!onViewContent}
                className={cn(
                  'flex w-full min-w-0 items-center gap-2 rounded-sm py-0.5 text-left font-mono text-[12.5px] text-low',
                  onViewContent && 'hover:text-high',
                  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
                  'disabled:cursor-default'
                )}
              >
                {entry.status && (
                  <ToolStatusDot status={entry.status} className="shrink-0" />
                )}
                <span className="min-w-0 truncate">{entry.summary}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </ChatStep>
  );
}
