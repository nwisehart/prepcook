import type { ReactNode } from 'react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { ChatStep, chatStepStateFromStatus } from './ChatStep';
import type { ToolStatusLike } from './ToolStatusDot';

export interface ChatSubagentResultLike {
  value?: unknown | null;
}

export interface ChatSubagentEntryRenderProps {
  content: string;
  workspaceId?: string;
}

interface ChatSubagentEntryProps {
  description: string;
  subagentType?: string | null;
  result?: ChatSubagentResultLike | null;
  expanded?: boolean;
  onToggle?: () => void;
  time?: string;
  className?: string;
  status?: ToolStatusLike;
  workspaceId?: string;
  renderMarkdown: (props: ChatSubagentEntryRenderProps) => ReactNode;
}

/**
 * A subagent (Task tool) run as a transcript step: its description, its
 * type, and its output when expanded.
 */
export function ChatSubagentEntry({
  description,
  subagentType,
  result,
  expanded = false,
  onToggle,
  time,
  className,
  status,
  workspaceId,
  renderMarkdown,
}: ChatSubagentEntryProps) {
  const { t } = useTranslation('common');

  const formattedType = useMemo(() => {
    if (!subagentType) return t('conversation.subagent.defaultType');
    return subagentType.charAt(0).toUpperCase() + subagentType.slice(1);
  }, [subagentType, t]);

  // Extract the result content for display
  const resultContent = useMemo(() => {
    if (!result?.value) return null;
    if (typeof result.value === 'string') {
      return result.value;
    }
    return JSON.stringify(result.value, null, 2);
  }, [result]);

  const isInteractive = Boolean(onToggle && resultContent);

  return (
    <ChatStep
      state={chatStepStateFromStatus(status)}
      label={description}
      detail={formattedType}
      time={time}
      expanded={isInteractive ? expanded : undefined}
      onActivate={isInteractive ? onToggle : undefined}
      className={className}
    >
      {expanded && resultContent && (
        <div className="mt-1.5 max-w-[680px] rounded border bg-secondary px-3.5 py-3">
          <div className="pb-1.5 text-[11px] font-bold uppercase tracking-[0.08em] text-low">
            {t('conversation.output')}
          </div>
          {renderMarkdown({ content: resultContent, workspaceId })}
        </div>
      )}
    </ChatStep>
  );
}
