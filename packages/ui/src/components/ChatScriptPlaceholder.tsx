import { useTranslation } from 'react-i18next';
import { GearSixIcon, TerminalIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';

export type ScriptPlaceholderType = 'setup' | 'cleanup';

interface ChatScriptPlaceholderProps {
  type: ScriptPlaceholderType;
  className?: string;
  onConfigure?: () => void;
}

/** A step that has not happened because no script is configured for it. */
export function ChatScriptPlaceholder({
  type,
  className,
  onConfigure,
}: ChatScriptPlaceholderProps) {
  const { t } = useTranslation('tasks');

  const title =
    type === 'setup'
      ? t('conversation.scriptPlaceholder.setupTitle')
      : t('conversation.scriptPlaceholder.cleanupTitle');

  const description =
    type === 'setup'
      ? t('conversation.scriptPlaceholder.setupDescription')
      : t('conversation.scriptPlaceholder.cleanupDescription');

  return (
    <div className={cn('flex items-start gap-3 py-1.5', className)}>
      <span className="inline-flex size-6 shrink-0 items-center justify-center rounded-full border border-dashed text-low">
        <TerminalIcon className="size-3.5" aria-hidden />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="pt-0.5 text-sm font-semibold text-normal">
          {title}
        </span>
        <span className="text-[12.5px] text-low">{description}</span>
      </div>
      {onConfigure && (
        <button
          type="button"
          onClick={onConfigure}
          className={cn(
            'flex h-6 shrink-0 items-center gap-1 rounded px-2 text-xs font-semibold text-brand-secondary transition-colors hover:bg-panel',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
          )}
        >
          <GearSixIcon className="size-3.5" />
          <span>{t('conversation.scriptPlaceholder.configure')}</span>
        </button>
      )}
    </div>
  );
}
