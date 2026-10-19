import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowUUpLeftIcon,
  CaretDownIcon,
  PencilSimpleIcon,
  UserIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { Tooltip } from './Tooltip';

export interface ChatUserMessageRenderProps {
  content: string;
  workspaceId?: string;
}

interface ChatUserMessageProps {
  content: string;
  expanded?: boolean;
  onToggle?: () => void;
  time?: string;
  className?: string;
  workspaceId?: string;
  onEdit?: () => void;
  onReset?: () => void;
  isGreyed?: boolean;
  renderMarkdown: (props: ChatUserMessageRenderProps) => ReactNode;
}

const headerActionClassName = cn(
  'flex size-6 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high',
  'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
);

/**
 * A message from a person (or the orchestrator's prompt): "You" with the
 * time, and the message on its own white surface so it reads apart from the
 * agent's prose.
 */
export function ChatUserMessage({
  content,
  expanded = true,
  onToggle,
  time,
  className,
  workspaceId,
  onEdit,
  onReset,
  isGreyed,
  renderMarkdown,
}: ChatUserMessageProps) {
  const { t } = useTranslation('tasks');
  const showActions = !isGreyed && (onEdit || onReset);

  return (
    <article
      className={cn(
        'flex items-start gap-3 py-2',
        isGreyed && 'pointer-events-none opacity-50',
        className
      )}
    >
      <span className="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-panel text-low">
        <UserIcon className="size-4" aria-hidden />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex min-h-7 items-center gap-2">
          <button
            type="button"
            onClick={onToggle}
            disabled={!onToggle}
            aria-expanded={onToggle ? expanded : undefined}
            className={cn(
              'flex items-baseline gap-2 rounded-sm text-left disabled:cursor-default',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
            )}
          >
            <span className="text-sm font-bold text-high">
              {t('conversation.you')}
            </span>
            {time && <span className="text-xs text-low">{time}</span>}
          </button>
          {onToggle && (
            <CaretDownIcon
              className={cn(
                'size-3 text-low transition-transform',
                !expanded && '-rotate-90'
              )}
              weight="bold"
              aria-hidden
            />
          )}
          {showActions && (
            <div className="ml-auto flex items-center gap-0.5">
              {onReset && (
                <Tooltip content={t('conversation.actions.resetTooltip')}>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onReset();
                    }}
                    className={headerActionClassName}
                    aria-label={t('conversation.actions.reset')}
                  >
                    <ArrowUUpLeftIcon className="size-3.5" />
                  </button>
                </Tooltip>
              )}
              {onEdit && (
                <Tooltip content={t('conversation.actions.edit')}>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onEdit();
                    }}
                    className={headerActionClassName}
                    aria-label={t('conversation.actions.edit')}
                  >
                    <PencilSimpleIcon className="size-3.5" />
                  </button>
                </Tooltip>
              )}
            </div>
          )}
        </div>
        {expanded && (
          <div className="max-w-[680px] rounded-lg border bg-secondary px-3.5 py-2.5 text-sm text-normal">
            {renderMarkdown({ content, workspaceId })}
          </div>
        )}
      </div>
    </article>
  );
}
