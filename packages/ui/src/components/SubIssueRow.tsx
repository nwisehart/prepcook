'use client';

import { Draggable } from '@hello-pangea/dnd';
import {
  CircleDashedIcon,
  DotsSixVerticalIcon,
  DotsThreeIcon,
  LinkBreakIcon,
  TrashIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { PriorityIcon, type PriorityLevel } from './PriorityIcon';
import { StatusDot } from './StatusDot';
import { KanbanAssignee, type KanbanAssigneeUser } from './KanbanAssignee';
import { useTranslation } from 'react-i18next';
import { formatRelativeTime } from './KanbanRelativeTime';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './DropdownMenu';

export interface SubIssueRowProps {
  id: string;
  index: number;
  simpleId: string;
  title: string;
  priority: PriorityLevel | null;
  statusColor: string;
  assignees: KanbanAssigneeUser[];
  createdAt: string;
  onClick?: () => void;
  onPriorityClick?: (e: React.MouseEvent) => void;
  onAssigneeClick?: (e: React.MouseEvent) => void;
  onMarkIndependentClick?: (e: React.MouseEvent) => void;
  onDeleteClick?: (e: React.MouseEvent) => void;
  className?: string;
}

export function SubIssueRow({
  id,
  index,
  simpleId,
  title,
  priority,
  statusColor,
  assignees,
  createdAt,
  onClick,
  onPriorityClick,
  onAssigneeClick,
  onMarkIndependentClick,
  onDeleteClick,
  className,
}: SubIssueRowProps) {
  const { t } = useTranslation('common');

  return (
    <Draggable draggableId={id} index={index}>
      {(provided, snapshot) => (
        <div
          ref={provided.innerRef}
          {...provided.draggableProps}
          role={onClick ? 'button' : undefined}
          tabIndex={onClick ? 0 : undefined}
          onClick={onClick}
          onKeyDown={(e) => {
            if (onClick && (e.key === 'Enter' || e.key === ' ')) {
              e.preventDefault();
              onClick();
            }
          }}
          className={cn(
            'flex items-center gap-2 rounded-sm px-2 py-1.5 transition-colors',
            onClick && 'cursor-pointer hover:bg-panel',
            snapshot.isDragging &&
              'cursor-grabbing bg-secondary shadow-overlay',
            className
          )}
        >
          {/* Drag handle */}
          <div
            {...provided.dragHandleProps}
            className="cursor-grab shrink-0"
            onClick={(e) => e.stopPropagation()}
          >
            <DotsSixVerticalIcon
              className="size-icon-xs text-low"
              weight="bold"
            />
          </div>

          {/* Left side: Priority, ID, Status, Title */}
          <div className="flex items-center gap-half flex-1 min-w-0">
            {onPriorityClick ? (
              <button
                type="button"
                onClick={onPriorityClick}
                className="flex items-center cursor-pointer hover:bg-panel rounded-sm transition-colors"
              >
                <PriorityIcon priority={priority} />
                {!priority && (
                  <CircleDashedIcon
                    className="size-icon-xs text-low"
                    weight="bold"
                  />
                )}
              </button>
            ) : (
              <PriorityIcon priority={priority} />
            )}
            <span className="shrink-0 font-mono text-xs text-low">
              {simpleId}
            </span>
            <StatusDot color={statusColor} />
            <span className="truncate text-high">{title}</span>
          </div>

          {/* Right side: Assignee, Age */}
          <div className="flex items-center gap-half shrink-0">
            {onAssigneeClick ? (
              <button
                type="button"
                onClick={onAssigneeClick}
                className="cursor-pointer hover:bg-panel rounded-full transition-colors"
              >
                <KanbanAssignee assignees={assignees} />
              </button>
            ) : (
              <KanbanAssignee assignees={assignees} />
            )}
            <span className="text-xs text-low">
              {formatRelativeTime(t, createdAt)}
            </span>
            {(onMarkIndependentClick || onDeleteClick) && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    onClick={(e) => e.stopPropagation()}
                    className="p-half rounded-sm text-low hover:text-high hover:bg-panel transition-colors"
                    aria-label="Sub-issue actions"
                    title="Sub-issue actions"
                  >
                    <DotsThreeIcon className="size-icon-xs" weight="bold" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {onMarkIndependentClick && (
                    <DropdownMenuItem
                      onClick={(e) => {
                        e.stopPropagation();
                        onMarkIndependentClick(e);
                      }}
                    >
                      <LinkBreakIcon className="size-icon-xs" />
                      {t('kanban.markIndependentIssue')}
                    </DropdownMenuItem>
                  )}
                  {onDeleteClick && (
                    <DropdownMenuItem
                      onClick={(e) => {
                        e.stopPropagation();
                        onDeleteClick(e);
                      }}
                      className="text-error focus:text-error"
                    >
                      <TrashIcon className="size-icon-xs" />
                      {t('buttons.delete')}
                    </DropdownMenuItem>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>
        </div>
      )}
    </Draggable>
  );
}
