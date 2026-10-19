import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowSquareUpRightIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { ChatStep, ChatStepDetail, chatStepStateFromStatus } from './ChatStep';
import type { ToolStatusLike } from './ToolStatusDot';

export type ChatFileEntryDiffInput =
  | {
      type: 'content';
      oldContent: string;
      newContent: string;
      oldPath?: string;
      newPath: string;
    }
  | {
      type: 'unified';
      path: string;
      unifiedDiff: string;
      hasLineNumbers?: boolean;
    };

/** "+12 −3" in mono, additions green and deletions red. */
export function ChatDiffCounts({
  additions,
  deletions,
}: {
  additions?: number;
  deletions?: number;
}) {
  const hasAdditions = additions !== undefined && additions > 0;
  const hasDeletions = deletions !== undefined && deletions > 0;
  if (!hasAdditions && !hasDeletions) return null;
  return (
    <span className="font-mono">
      {hasAdditions && <span className="text-success">+{additions}</span>}
      {hasAdditions && hasDeletions && ' '}
      {hasDeletions && <span className="text-error">−{deletions}</span>}
    </span>
  );
}

/** Small icon button that opens a file in the Review panel. */
export function ChatOpenInReviewButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation('tasks');
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        'flex size-6 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
      )}
      title={t('conversation.viewInReview')}
      aria-label={t('conversation.viewInReview')}
    >
      <ArrowSquareUpRightIcon className="size-3.5" />
    </button>
  );
}

interface ChatFileEntryProps {
  /** Step label, e.g. "Edited quantity.ts" */
  label: string;
  /** Full path, shown in mono on the detail line */
  filename: string;
  additions?: number;
  deletions?: number;
  time?: string;
  expanded?: boolean;
  onToggle?: () => void;
  className?: string;
  status?: ToolStatusLike;
  /** Optional diff content for expanded view */
  diffContent?: ChatFileEntryDiffInput;
  /** Optional callback to open file in the Review tab */
  onOpenInChanges?: () => void;
  /** Optional diff renderer from the app layer */
  renderDiffBody?: (diffContent: ChatFileEntryDiffInput) => ReactNode;
}

/** A file edit as a transcript step; expands to show its diff. */
export function ChatFileEntry({
  label,
  filename,
  additions,
  deletions,
  time,
  expanded = false,
  onToggle,
  className,
  status,
  diffContent,
  onOpenInChanges,
  renderDiffBody,
}: ChatFileEntryProps) {
  const hasDiffContent = Boolean(diffContent && renderDiffBody);

  return (
    <ChatStep
      state={chatStepStateFromStatus(status)}
      label={label}
      detail={
        <ChatStepDetail
          parts={[
            <span key="path" className="font-mono">
              {filename}
            </span>,
            additions || deletions ? (
              <ChatDiffCounts
                key="counts"
                additions={additions}
                deletions={deletions}
              />
            ) : null,
          ]}
        />
      }
      time={time}
      expanded={hasDiffContent && onToggle ? expanded : undefined}
      onActivate={hasDiffContent ? onToggle : undefined}
      actions={
        onOpenInChanges ? (
          <ChatOpenInReviewButton onClick={onOpenInChanges} />
        ) : undefined
      }
      className={className}
    >
      {expanded && diffContent && renderDiffBody && (
        <div className="mt-1.5 overflow-hidden rounded border bg-secondary">
          {renderDiffBody(diffContent)}
        </div>
      )}
    </ChatStep>
  );
}
