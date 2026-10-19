import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ChatStep,
  ChatStepDetail,
  chatStepStateFromStatus,
  worstChatStepState,
} from './ChatStep';
import {
  ChatDiffCounts,
  ChatOpenInReviewButton,
  type ChatFileEntryDiffInput,
} from './ChatFileEntry';
import { ToolStatusDot, type ToolStatusLike } from './ToolStatusDot';

export type ChatAggregatedDiffChange = {
  action: 'edit' | 'write' | 'delete' | 'rename';
  unified_diff?: string;
  has_line_numbers?: boolean;
  content?: string;
  new_path?: string;
};

export interface AggregatedDiffEntry {
  /** The file change data */
  change: ChatAggregatedDiffChange;
  /** Tool status for this change */
  status: ToolStatusLike | null;
  /** Unique key for expansion state */
  expansionKey: string;
}

interface ChatAggregatedDiffEntriesProps {
  /** Step label, e.g. "Edited quantity.ts" */
  label: string;
  /** The file path being edited */
  filePath: string;
  /** The individual diff entries for this file */
  entries: AggregatedDiffEntry[];
  /** Whether the step is expanded */
  expanded: boolean;
  /** Callback when toggling expansion */
  onToggle: () => void;
  /** Callback to open file in changes panel */
  onOpenInChanges: (() => void) | null;
  time?: string;
  className?: string;
  renderDiffBody?: (args: {
    filePath: string;
    change: ChatAggregatedDiffChange;
    diffContent?: ChatFileEntryDiffInput;
  }) => React.ReactNode;
}

function parseUnifiedDiffStats(unifiedDiff: string) {
  let additions = 0;
  let deletions = 0;

  for (const line of unifiedDiff.split('\n')) {
    if (line.startsWith('+++') || line.startsWith('---')) {
      continue;
    }
    if (line.startsWith('+')) {
      additions += 1;
    } else if (line.startsWith('-')) {
      deletions += 1;
    }
  }

  return { additions, deletions };
}

function changeStats(change: ChatAggregatedDiffChange) {
  if (change.action === 'edit' && change.unified_diff) {
    return parseUnifiedDiffStats(change.unified_diff);
  }
  if (change.action === 'write' && change.content) {
    return { additions: change.content.split('\n').length, deletions: 0 };
  }
  return { additions: 0, deletions: 0 };
}

function buildDiffContent(
  change: ChatAggregatedDiffChange,
  filePath: string
): ChatFileEntryDiffInput | undefined {
  if (change.action === 'edit' && change.unified_diff) {
    return {
      type: 'unified',
      path: filePath,
      unifiedDiff: change.unified_diff,
      hasLineNumbers: change.has_line_numbers ?? true,
    };
  }
  if (change.action === 'write' && change.content) {
    return {
      type: 'content',
      oldContent: '',
      newContent: change.content,
      newPath: filePath,
    };
  }
  return undefined;
}

function DiffEntry({
  filePath,
  change,
  status,
  renderDiffBody,
}: {
  filePath: string;
  change: ChatAggregatedDiffChange;
  status: ToolStatusLike | null;
  renderDiffBody?: ChatAggregatedDiffEntriesProps['renderDiffBody'];
}) {
  const { t } = useTranslation('tasks');
  const { additions, deletions } = useMemo(() => changeStats(change), [change]);
  const diffContent = useMemo(
    () => buildDiffContent(change, filePath),
    [change, filePath]
  );

  const actionLabel = (() => {
    switch (change.action) {
      case 'edit':
        return t('conversation.fileEntry.modified');
      case 'write':
        return t('conversation.fileEntry.created');
      case 'delete':
        return t('conversation.fileEntry.deleted');
      case 'rename':
        return change.new_path
          ? `${t('conversation.fileEntry.renamed')} → ${change.new_path}`
          : t('conversation.fileEntry.renamed');
    }
  })();

  return (
    <div className="border-t border-divider first:border-t-0">
      <div className="flex items-center gap-2 px-3 py-1.5 text-[12.5px] text-low">
        {status && <ToolStatusDot status={status} className="shrink-0" />}
        <span>{actionLabel}</span>
        <ChatDiffCounts additions={additions} deletions={deletions} />
      </div>
      {diffContent && renderDiffBody?.({ filePath, change, diffContent })}
    </div>
  );
}

/** Several edits to one file as a single step; expands to every diff. */
export function ChatAggregatedDiffEntries({
  label,
  filePath,
  entries,
  expanded,
  onToggle,
  onOpenInChanges,
  time,
  className,
  renderDiffBody,
}: ChatAggregatedDiffEntriesProps) {
  const { t } = useTranslation('common');

  const totalStats = useMemo(
    () =>
      entries.reduce(
        (total, entry) => {
          const stats = changeStats(entry.change);
          return {
            additions: total.additions + stats.additions,
            deletions: total.deletions + stats.deletions,
          };
        },
        { additions: 0, deletions: 0 }
      ),
    [entries]
  );

  const state = worstChatStepState(
    entries.map((entry) => chatStepStateFromStatus(entry.status))
  );

  return (
    <ChatStep
      state={state}
      label={label}
      detail={
        <ChatStepDetail
          parts={[
            <span key="path" className="font-mono">
              {filePath}
            </span>,
            t('conversation.steps.editCount', { count: entries.length }),
            totalStats.additions || totalStats.deletions ? (
              <ChatDiffCounts
                key="counts"
                additions={totalStats.additions}
                deletions={totalStats.deletions}
              />
            ) : null,
          ]}
        />
      }
      time={time}
      expanded={expanded}
      onActivate={onToggle}
      actions={
        onOpenInChanges ? (
          <ChatOpenInReviewButton onClick={onOpenInChanges} />
        ) : undefined
      }
      className={className}
    >
      {expanded && (
        <div className="mt-1.5 overflow-hidden rounded border bg-secondary">
          {entries.map((entry) => (
            <DiffEntry
              key={entry.expansionKey}
              filePath={filePath}
              change={entry.change}
              status={entry.status}
              renderDiffBody={renderDiffBody}
            />
          ))}
        </div>
      )}
    </ChatStep>
  );
}
