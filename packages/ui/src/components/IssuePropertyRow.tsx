import type { ReactNode } from 'react';
import { cn } from '../lib/cn';
import { CaretDownIcon, PlusIcon, XIcon } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { IconButton } from './IconButton';
import type { PriorityLevel } from './PriorityIcon';
import { UserAvatar, type UserAvatarUser } from './UserAvatar';
import { KanbanAssignee, type KanbanAssigneeUser } from './KanbanAssignee';
import { KanbanColumnDotMark, type KanbanColumnDot } from './KanbanBoard';

export interface IssuePropertyStatus {
  id: string;
  name: string;
  color: string;
}

/** A read-only property cell (Playbook, Tracker, Branch, …). */
export interface IssueExtraProperty {
  key: string;
  label: string;
  value: ReactNode;
  /** Spans two grid columns (long values such as a branch). */
  wide?: boolean;
  mono?: boolean;
}

export interface IssuePropertyRowProps {
  statusId: string;
  priority: PriorityLevel | null;
  assigneeIds: string[];
  assigneeUsers?: KanbanAssigneeUser[];
  statuses: IssuePropertyStatus[];
  creatorUser?: UserAvatarUser | null;
  parentIssue?: { id: string; simpleId: string } | null;
  onParentIssueClick?: () => void;
  onRemoveParentIssue?: () => void;
  onStatusClick: () => void;
  /** Priority is edited from the panel header's priority chip. */
  onPriorityClick: () => void;
  onAssigneeClick: () => void;
  onAddClick?: () => void;
  disabled?: boolean;
  // Disables only the status selector (e.g. when the status is locked)
  statusDisabled?: boolean;
  /** The stage dot for the current column; defaults to the column color. */
  statusDot?: KanbanColumnDot;
  extraProperties?: IssueExtraProperty[];
  className?: string;
}

function PropertyCell({
  label,
  children,
  wide,
}: {
  label: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div
      className={cn('flex min-w-0 flex-col gap-[3px]', wide && 'col-span-2')}
    >
      <span className="text-xs text-low">{label}</span>
      <div className="flex min-h-7 min-w-0 items-center gap-1.5 text-[13px] font-medium text-high">
        {children}
      </div>
    </div>
  );
}

const firstName = (user: KanbanAssigneeUser | UserAvatarUser) =>
  user.first_name?.trim() || user.username?.trim() || null;

/**
 * The card panel's property grid: Stage (opens the status menu), Owner
 * (opens the assignee menu), then the read-only facts the card has.
 */
export function IssuePropertyRow({
  statusId,
  assigneeUsers,
  statuses,
  creatorUser,
  parentIssue,
  onParentIssueClick,
  onRemoveParentIssue,
  onStatusClick,
  onAssigneeClick,
  onAddClick,
  disabled,
  statusDisabled,
  statusDot,
  extraProperties = [],
  className,
}: IssuePropertyRowProps) {
  const { t } = useTranslation('common');
  const status = statuses.find((s) => s.id === statusId);
  const owners = assigneeUsers ?? [];
  const creatorName = creatorUser ? firstName(creatorUser) : null;

  return (
    <div className={cn('grid grid-cols-3 gap-x-4 gap-y-2.5 pt-0.5', className)}>
      <PropertyCell label={t('kanban.panel.stage')}>
        <button
          type="button"
          onClick={onStatusClick}
          disabled={disabled || statusDisabled}
          className="inline-flex h-7 min-w-0 max-w-full items-center gap-2 rounded border border-border bg-secondary px-2.5 text-[13px] font-semibold text-high transition-colors hover:bg-panel disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
        >
          <KanbanColumnDotMark
            dot={statusDot ?? { color: status?.color ?? '0 0% 50%' }}
          />
          <span className="truncate">
            {status?.name ?? t('kanban.panel.selectStage')}
          </span>
          {!(disabled || statusDisabled) && (
            <CaretDownIcon className="size-3 shrink-0 text-low" />
          )}
        </button>
      </PropertyCell>

      <PropertyCell label={t('kanban.panel.owner')}>
        <button
          type="button"
          onClick={onAssigneeClick}
          disabled={disabled}
          className="-mx-1 inline-flex h-7 min-w-0 items-center gap-1.5 rounded px-1 transition-colors hover:bg-panel disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
        >
          {owners.length === 1 ? (
            <>
              <UserAvatar user={owners[0]} className="size-5 text-[10px]" />
              <span className="truncate">{firstName(owners[0])}</span>
            </>
          ) : owners.length > 1 ? (
            <KanbanAssignee assignees={owners} />
          ) : (
            <span className="font-normal text-low">
              {t('kanban.unassigned')}
            </span>
          )}
        </button>
      </PropertyCell>

      {extraProperties.map((property) => (
        <PropertyCell
          key={property.key}
          label={property.label}
          wide={property.wide}
        >
          <span
            className={cn(
              'truncate',
              property.mono && 'font-mono text-xs font-normal'
            )}
          >
            {property.value}
          </span>
        </PropertyCell>
      ))}

      {creatorUser && creatorName && (
        <PropertyCell label={t('kanban.createdBy', 'Created by')}>
          <UserAvatar user={creatorUser} className="size-5 text-[10px]" />
          <span className="truncate">{creatorName}</span>
        </PropertyCell>
      )}

      {parentIssue && (
        <PropertyCell label={t('kanban.parentIssue', 'Parent')}>
          <button
            type="button"
            onClick={onParentIssueClick}
            disabled={disabled}
            className="truncate font-mono text-xs text-brand-secondary hover:underline"
          >
            {parentIssue.simpleId}
          </button>
          {onRemoveParentIssue && (
            <IconButton
              icon={XIcon}
              onClick={onRemoveParentIssue}
              disabled={disabled}
              aria-label={t('kanban.panel.removeParent')}
              title={t('kanban.panel.removeParent')}
            />
          )}
        </PropertyCell>
      )}

      {onAddClick && (
        <div className="flex items-end">
          <IconButton
            icon={PlusIcon}
            onClick={onAddClick}
            disabled={disabled}
            aria-label={t('buttons.add')}
            title={t('buttons.add')}
          />
        </div>
      )}
    </div>
  );
}
