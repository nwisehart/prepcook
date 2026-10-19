'use client';

import type { MouseEvent } from 'react';
import { cn } from '../lib/cn';
import { Draggable } from '@hello-pangea/dnd';
import { DotsSixVerticalIcon } from '@phosphor-icons/react';
import { PriorityIcon, type PriorityLevel } from './PriorityIcon';
import { StatusDot } from './StatusDot';
import { KanbanBadge } from './KanbanBadge';
import { KanbanAssignee, type KanbanAssigneeUser } from './KanbanAssignee';
import {
  RelationshipBadge,
  type RelationshipDisplayType,
} from './RelationshipBadge';
import { Checkbox } from './Checkbox';
import { formatRelativeTime } from './KanbanRelativeTime';
import { useTranslation } from 'react-i18next';

const MAX_VISIBLE_TAGS = 2;

export interface IssueListRowIssue {
  id: string;
  simple_id: string;
  title: string;
  priority: PriorityLevel | null;
  created_at: string;
}

export interface IssueListRowTag {
  id: string;
  name: string;
  color: string;
}

export interface IssueListRowRelationship {
  relationshipId: string;
  displayType: RelationshipDisplayType;
  relatedIssueDisplayId: string;
}

export interface IssueListRowProps {
  issue: IssueListRowIssue;
  index: number;
  statusColor: string;
  tags: IssueListRowTag[];
  relationships?: IssueListRowRelationship[];
  assignees: KanbanAssigneeUser[];
  onClick: (e: MouseEvent) => void;
  isSelected: boolean;
  isMultiSelectActive?: boolean;
  isChecked?: boolean;
  onCheckboxChange?: (checked: boolean) => void;
  className?: string;
}

export function IssueListRow({
  issue,
  index,
  statusColor,
  tags,
  relationships = [],
  assignees,
  onClick,
  isSelected,
  isMultiSelectActive = false,
  isChecked = false,
  onCheckboxChange,
  className,
}: IssueListRowProps) {
  const { t } = useTranslation('common');
  const showCheckbox = isMultiSelectActive || isChecked;
  const visibleTags = tags.slice(0, MAX_VISIBLE_TAGS);

  return (
    <Draggable draggableId={issue.id} index={index}>
      {(provided, snapshot) => (
        <div
          ref={provided.innerRef}
          {...provided.draggableProps}
          role="button"
          tabIndex={0}
          onClick={onClick}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              onClick(e as unknown as MouseEvent);
            }
          }}
          className={cn(
            'group/row flex items-center justify-between gap-double border-b border-divider px-double py-2',
            'transition-colors',
            'hover:bg-panel/60',
            (isSelected || isChecked) && 'bg-brand-soft/50',
            snapshot.isDragging &&
              'cursor-grabbing bg-secondary shadow-overlay',
            className
          )}
        >
          {/* Left side: Checkbox/Drag handle, Priority, ID, Status, Title */}
          <div className="flex items-center gap-double flex-1 min-w-0">
            <div className="relative shrink-0 w-4 flex items-center justify-center">
              {/* Drag handle — hidden when checkbox is shown */}
              <div
                {...provided.dragHandleProps}
                className={cn(
                  'cursor-grab',
                  showCheckbox ? 'hidden' : 'flex group-hover/row:hidden'
                )}
                onClick={(e) => e.stopPropagation()}
              >
                <DotsSixVerticalIcon
                  className="size-icon-xs text-low"
                  weight="bold"
                />
              </div>
              {/* Checkbox — shown on hover or when multi-select active */}
              <div
                className={cn(
                  'items-center justify-center',
                  showCheckbox ? 'flex' : 'hidden group-hover/row:flex'
                )}
                onClick={(e) => e.stopPropagation()}
              >
                <Checkbox
                  checked={isChecked}
                  onCheckedChange={(checked) => {
                    onCheckboxChange?.(checked);
                  }}
                />
              </div>
            </div>
            <PriorityIcon priority={issue.priority} />
            <span className="shrink-0 font-mono text-xs text-low">
              {issue.simple_id}
            </span>
            <StatusDot color={statusColor} />
            <span className="truncate text-sm font-semibold text-high">
              {issue.title}
            </span>
          </div>

          {/* Right side: Tags, Assignee, Age */}
          <div className="flex items-center gap-base shrink-0">
            {visibleTags.length > 0 && (
              <div className="flex items-center gap-half">
                {visibleTags.map((tag) => (
                  <KanbanBadge key={tag.id} name={tag.name} color={tag.color} />
                ))}
              </div>
            )}
            {relationships.length > 0 && (
              <div className="flex items-center gap-half">
                {relationships.slice(0, 2).map((rel) => (
                  <RelationshipBadge
                    key={rel.relationshipId}
                    displayType={rel.displayType}
                    relatedIssueDisplayId={rel.relatedIssueDisplayId}
                    compact
                  />
                ))}
                {relationships.length > 2 && (
                  <span className="text-xs text-low">
                    +{relationships.length - 2}
                  </span>
                )}
              </div>
            )}
            <KanbanAssignee assignees={assignees} />
            <span className="min-w-12 text-right text-xs text-low">
              {formatRelativeTime(t, issue.created_at)}
            </span>
          </div>
        </div>
      )}
    </Draggable>
  );
}
