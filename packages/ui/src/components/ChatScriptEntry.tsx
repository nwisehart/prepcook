import { useTranslation } from 'react-i18next';
import { WrenchIcon } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { ChatStep, ChatStepDetail, chatStepStateFromStatus } from './ChatStep';
import type { ToolStatusLike } from './ToolStatusDot';

interface ChatScriptEntryProps {
  title: string;
  command?: string;
  processId: string;
  exitCode?: number | null;
  time?: string;
  className?: string;
  status: ToolStatusLike;
  onViewProcess: (processId: string) => void;
  onFix?: () => void;
}

/** A repository script run (setup, cleanup, ...) as a transcript step. */
export function ChatScriptEntry({
  title,
  command,
  processId,
  exitCode,
  time,
  className,
  status,
  onViewProcess,
  onFix,
}: ChatScriptEntryProps) {
  const { t } = useTranslation('tasks');
  const isRunning = status.status === 'created';
  const isSuccess = status.status === 'success';
  const isFailed = status.status === 'failed';

  const outcome = (() => {
    if (isRunning) {
      return t('conversation.script.running');
    }
    if (isFailed && exitCode !== null && exitCode !== undefined) {
      return t('conversation.script.exitCode', { code: exitCode });
    }
    if (isSuccess) {
      return t('conversation.script.completedSuccessfully');
    }
    return t('conversation.script.clickToViewLogs');
  })();

  return (
    <ChatStep
      state={chatStepStateFromStatus(status)}
      label={title}
      detail={
        <ChatStepDetail
          parts={[
            command ? (
              <span key="command" className="font-mono">
                {command}
              </span>
            ) : null,
            <span key="outcome" className={cn(isFailed && 'text-error')}>
              {outcome}
            </span>,
          ]}
        />
      }
      time={time}
      onActivate={() => onViewProcess(processId)}
      actions={
        isFailed && onFix ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onFix();
            }}
            className={cn(
              'flex h-6 items-center gap-1 rounded px-2 text-xs font-semibold text-brand-secondary transition-colors hover:bg-panel',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
            )}
            title={t('conversation.script.viewScripts')}
          >
            <WrenchIcon className="size-3.5" />
            <span>{t('conversation.script.viewScripts')}</span>
          </button>
        ) : undefined
      }
      className={className}
    />
  );
}
