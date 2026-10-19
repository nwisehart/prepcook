import { useTranslation } from 'react-i18next';
import { ChatStep } from './ChatStep';

interface ChatErrorMessageProps {
  content: string;
  time?: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
}

/** An agent error as a failed step; expands to the full message. */
export function ChatErrorMessage({
  content,
  time,
  className,
  expanded,
  onToggle,
}: ChatErrorMessageProps) {
  const { t } = useTranslation('tasks');

  return (
    <ChatStep
      state="error"
      label={t('conversation.error')}
      detail={expanded ? undefined : content}
      time={time}
      expanded={onToggle ? Boolean(expanded) : undefined}
      onActivate={onToggle}
      className={className}
    >
      {expanded && (
        <div className="mt-1 max-w-[680px] whitespace-pre-wrap break-words rounded bg-error-soft px-3 py-2 text-[12.5px] text-error">
          {content}
        </div>
      )}
    </ChatStep>
  );
}
