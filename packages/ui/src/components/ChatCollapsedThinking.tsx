import type { ReactNode } from 'react';
import { CaretDownIcon, ChatDotsIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';

export interface ThinkingEntry {
  content: string;
  expansionKey: string;
}

export interface ChatCollapsedThinkingRenderProps {
  content: string;
  workspaceId?: string;
  className?: string;
}

interface ChatCollapsedThinkingProps {
  entries: ThinkingEntry[];
  expanded: boolean;
  onToggle: () => void;
  className?: string;
  workspaceId?: string;
  renderMarkdown: (props: ChatCollapsedThinkingRenderProps) => ReactNode;
}

/**
 * Thinking from an earlier turn, folded to one muted "Thinking" line; when
 * expanded, every thinking entry on a quiet 8px-radius block.
 */
export function ChatCollapsedThinking({
  entries,
  expanded,
  onToggle,
  className,
  workspaceId,
  renderMarkdown,
}: ChatCollapsedThinkingProps) {
  const { t } = useTranslation('common');

  if (entries.length === 0) return null;

  return (
    <div className={cn('flex items-start gap-3 py-1.5', className)}>
      <span className="inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-panel text-low">
        <ChatDotsIcon className="size-3.5" aria-hidden />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          data-scroll-anchor-target=""
          className={cn(
            'flex items-center gap-1.5 self-start rounded-sm pt-0.5 text-sm font-medium text-low hover:text-normal',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
          )}
        >
          <span>{t('conversation.thinking')}</span>
          <CaretDownIcon
            className={cn(
              'size-3 transition-transform',
              !expanded && '-rotate-90'
            )}
            weight="bold"
            aria-hidden
          />
        </button>
        {expanded && (
          <div className="flex flex-col gap-2 rounded bg-panel/60 px-3 py-2">
            {entries.map((entry) => (
              <div key={entry.expansionKey} className="text-[13px] text-low">
                {renderMarkdown({
                  content: entry.content,
                  workspaceId: workspaceId,
                  className: 'text-[13px] text-low',
                })}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
