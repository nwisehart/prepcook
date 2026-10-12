import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Virtuoso,
  type FollowOutput,
  type ItemContent,
  type VirtuosoHandle,
} from 'react-virtuoso';
import { WarningCircleIcon } from '@phosphor-icons/react/dist/ssr';
import RawLogText from '@/shared/components/RawLogText';
import type { PatchType } from 'shared/types';

export type LogEntry = Extract<
  PatchType,
  { type: 'STDOUT' } | { type: 'STDERR' }
>;

export interface VirtualizedProcessLogsProps {
  logs: LogEntry[];
  error: string | null;
  searchQuery: string;
  matchIndices: number[];
  currentMatchIndex: number;
}

interface SearchContext {
  searchQuery: string;
  matchIndices: number[];
  currentMatchIndex: number;
}

// Logs are append-only, so an entry's index is a stable identity.
const computeItemKey = (index: number) => `log-${index}`;

// Follow new output only while the viewport is pinned to the bottom; once the
// user scrolls up (or jumps to a search match) the view stays where it is.
const followOutput: FollowOutput = (isAtBottom) =>
  isAtBottom ? 'auto' : false;

const itemContent: ItemContent<LogEntry, SearchContext> = (
  index,
  entry,
  context
) => {
  const isMatch = context.matchIndices.includes(index);
  const isCurrentMatch =
    context.matchIndices[context.currentMatchIndex] === index;

  return (
    <RawLogText
      content={entry.content}
      channel={entry.type === 'STDERR' ? 'stderr' : 'stdout'}
      className="text-sm px-4 py-1"
      linkifyUrls
      searchQuery={isMatch ? context.searchQuery : undefined}
      isCurrentMatch={isCurrentMatch}
    />
  );
};

export function VirtualizedProcessLogs({
  logs,
  error,
  searchQuery,
  matchIndices,
  currentMatchIndex,
}: VirtualizedProcessLogsProps) {
  const { t } = useTranslation('tasks');
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const prevCurrentMatchRef = useRef<number | undefined>(undefined);

  // Scroll to the current match only when the user moves between matches, not
  // when new log lines change matchIndices.
  useEffect(() => {
    if (
      matchIndices.length > 0 &&
      currentMatchIndex >= 0 &&
      currentMatchIndex !== prevCurrentMatchRef.current
    ) {
      virtuosoRef.current?.scrollToIndex({
        index: matchIndices[currentMatchIndex],
        align: 'center',
        behavior: 'smooth',
      });
      prevCurrentMatchRef.current = currentMatchIndex;
    }
  }, [currentMatchIndex, matchIndices]);

  if (logs.length === 0 && !error) {
    return (
      <div className="h-full flex items-center justify-center">
        <p className="text-center text-muted-foreground text-sm">
          {t('processes.noLogsAvailable')}
        </p>
      </div>
    );
  }

  if (error && logs.length === 0) {
    return (
      <div className="h-full flex items-center justify-center">
        <p className="text-center text-destructive text-sm">
          <WarningCircleIcon className="size-icon-base inline mr-2" />
          {error}
        </p>
      </div>
    );
  }

  const context: SearchContext = {
    searchQuery,
    matchIndices,
    currentMatchIndex,
  };

  return (
    <div className="h-full overflow-hidden">
      <Virtuoso<LogEntry, SearchContext>
        ref={virtuosoRef}
        className="h-full"
        data={logs}
        context={context}
        initialTopMostItemIndex={{ index: 'LAST', align: 'end' }}
        followOutput={followOutput}
        computeItemKey={computeItemKey}
        itemContent={itemContent}
      />
    </div>
  );
}
