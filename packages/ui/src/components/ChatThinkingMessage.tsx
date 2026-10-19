import type { ReactNode } from 'react';
import { ChatDotsIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export interface ChatThinkingMessageRenderProps {
  content: string;
  workspaceId?: string;
  className?: string;
}

interface ChatThinkingMessageProps {
  content: string;
  className?: string;
  workspaceId?: string;
  renderMarkdown: (props: ChatThinkingMessageRenderProps) => ReactNode;
}

/** The agent's thinking: muted text on a quiet 8px-radius block. */
export function ChatThinkingMessage({
  content,
  className,
  workspaceId,
  renderMarkdown,
}: ChatThinkingMessageProps) {
  return (
    <div className={cn('flex items-start gap-3 py-1.5', className)}>
      <span className="inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-panel text-low">
        <ChatDotsIcon className="size-3.5" aria-hidden />
      </span>
      <div className="min-w-0 flex-1 rounded bg-panel/60 px-3 py-2 text-[13px] text-low">
        {renderMarkdown({
          content,
          workspaceId: workspaceId,
          className: 'text-[13px] text-low',
        })}
      </div>
    </div>
  );
}
