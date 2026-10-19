import { useTranslation } from 'react-i18next';
import {
  CheckIcon,
  CircleIcon,
  CircleHalfIcon,
  XIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { ChatStep } from './ChatStep';

export interface TodoItemLike {
  content: string;
  status?: string | null;
}

interface ChatTodoListProps {
  todos: TodoItemLike[];
  expanded?: boolean;
  onToggle?: () => void;
  time?: string;
}

function normalizeStatus(status?: string | null) {
  const s = (status || '').toLowerCase();
  return s === 'in-progress' ? 'in_progress' : s;
}

function TodoStatusIcon({ status }: { status: string }) {
  if (status === 'completed') {
    return <CheckIcon className="size-3.5 text-success" weight="bold" />;
  }
  if (status === 'in_progress') {
    return <CircleHalfIcon className="size-3.5 text-brand" weight="fill" />;
  }
  if (status === 'cancelled') {
    return <XIcon className="size-3.5 text-low" />;
  }
  return <CircleIcon className="size-3.5 text-low" />;
}

/** A to-do list update as a step: progress on the detail line. */
export function ChatTodoList({
  todos,
  expanded,
  onToggle,
  time,
}: ChatTodoListProps) {
  const { t } = useTranslation('common');
  const done = todos.filter(
    (todo) => normalizeStatus(todo.status) === 'completed'
  ).length;

  return (
    <ChatStep
      state="done"
      label={t('conversation.steps.updatedTodos')}
      detail={t('conversation.steps.todosDone', {
        done,
        total: todos.length,
      })}
      time={time}
      expanded={onToggle && todos.length > 0 ? Boolean(expanded) : undefined}
      onActivate={todos.length > 0 ? onToggle : undefined}
    >
      {expanded && todos.length > 0 && (
        <ul className="mt-1 flex flex-col gap-1">
          {todos.map((todo, index) => {
            const status = normalizeStatus(todo.status);
            return (
              <li
                key={`${todo.content}-${index}`}
                className="flex items-start gap-2 text-[13px]"
              >
                <span className="flex h-5 w-4 shrink-0 items-center justify-center">
                  <TodoStatusIcon status={status} />
                </span>
                <span
                  className={cn(
                    'break-words leading-5',
                    status === 'completed' || status === 'cancelled'
                      ? 'text-low'
                      : 'text-normal',
                    status === 'cancelled' && 'line-through'
                  )}
                >
                  {todo.content}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </ChatStep>
  );
}
