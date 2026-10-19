import { InfoIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

interface ChatSystemMessageProps {
  content: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
}

/** A system notice: muted, aligned with the step column, one line until expanded. */
export function ChatSystemMessage({
  content,
  className,
  expanded,
  onToggle,
}: ChatSystemMessageProps) {
  return (
    <div className={cn('flex items-start gap-3 py-1', className)}>
      <span className="inline-flex size-6 shrink-0 items-center justify-center text-low">
        <InfoIcon className="size-4" aria-hidden />
      </span>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        className={cn(
          'min-w-0 flex-1 rounded-sm pt-1 text-left text-[12.5px] text-low hover:text-normal',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
          expanded ? 'whitespace-pre-wrap break-words' : 'truncate'
        )}
      >
        {content}
      </button>
    </div>
  );
}
