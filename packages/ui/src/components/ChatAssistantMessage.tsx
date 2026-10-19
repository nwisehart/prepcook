import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { PrepCookMark } from './PrepCookMark';
import { ChatPrefixedComment } from './ChatStep';

export interface ChatAssistantMessageRenderProps {
  content: string;
  workspaceId?: string;
}

interface ChatAssistantMessageProps {
  content: string;
  workspaceId?: string;
  time?: string;
  /**
   * Set when the message is a board comment such as `REPRODUCTION: …`; it is
   * then shown as a mono comment box instead of prose.
   */
  prefixedComment?: { prefix: string; body: string } | null;
  renderMarkdown: (props: ChatAssistantMessageRenderProps) => ReactNode;
}

/** The agent speaking: the Prepcook mark, its name, the time, then prose. */
export function ChatAssistantMessage({
  content,
  workspaceId,
  time,
  prefixedComment,
  renderMarkdown,
}: ChatAssistantMessageProps) {
  const { t } = useTranslation('common');

  return (
    <article className="mt-2.5 flex items-start gap-3 py-1">
      <span className="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-brand text-on-brand">
        <PrepCookMark className="size-4" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-2.5">
        <div className="flex min-h-7 items-baseline gap-2 pt-1">
          <span className="text-sm font-bold text-high">
            {t('conversation.assistantName')}
          </span>
          {time && <span className="text-xs text-low">{time}</span>}
        </div>
        {prefixedComment ? (
          <ChatPrefixedComment
            prefix={prefixedComment.prefix}
            body={prefixedComment.body}
          />
        ) : (
          <div className="max-w-[680px] text-sm text-high">
            {renderMarkdown({ content, workspaceId })}
          </div>
        )}
      </div>
    </article>
  );
}
