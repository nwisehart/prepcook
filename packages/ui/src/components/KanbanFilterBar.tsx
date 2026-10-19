import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowLeftIcon,
  FunnelIcon,
  MagnifyingGlassIcon,
  PlusIcon,
  XIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import type { PriorityLevel } from './PriorityIcon';
import { Button } from './Button';
import { ButtonGroup, ButtonGroupItem } from './IconButtonGroup';

export interface KanbanFilterTag {
  id: string;
  name: string;
  color: string;
}

export interface KanbanFilterUser {
  user_id: string;
  first_name?: string | null;
  last_name?: string | null;
  username?: string | null;
  avatar_url?: string | null;
}

export interface KanbanFilterState<TSortField extends string = string> {
  searchQuery: string;
  priorities: PriorityLevel[];
  assigneeIds: string[];
  tagIds: string[];
  sortField: TSortField;
  sortDirection: 'asc' | 'desc';
}

export interface KanbanProjectViewIds {
  TEAM: string;
  PERSONAL: string;
}

const DEFAULT_KANBAN_PROJECT_VIEW_IDS: KanbanProjectViewIds = {
  TEAM: 'team',
  PERSONAL: 'personal',
};

export interface RenderKanbanFiltersDialogProps<
  TTag extends KanbanFilterTag = KanbanFilterTag,
  TUser extends KanbanFilterUser = KanbanFilterUser,
  TSortField extends string = string,
> {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tags: TTag[];
  users: TUser[];
  projectId: string;
  currentUserId: string | null;
  filters: KanbanFilterState<TSortField>;
  showSubIssues: boolean;
  showWorkspaces: boolean;
  onPrioritiesChange: (priorities: PriorityLevel[]) => void;
  onAssigneesChange: (assigneeIds: string[]) => void;
  onTagsChange: (tagIds: string[]) => void;
  onSortChange: (sortField: TSortField, sortDirection: 'asc' | 'desc') => void;
  onShowSubIssuesChange: (show: boolean) => void;
  onShowWorkspacesChange: (show: boolean) => void;
  hideBlocked: boolean;
  onHideBlockedChange: (hide: boolean) => void;
}

interface KanbanFilterBarProps<
  TTag extends KanbanFilterTag = KanbanFilterTag,
  TUser extends KanbanFilterUser = KanbanFilterUser,
  TSortField extends string = string,
> {
  isFiltersDialogOpen: boolean;
  onFiltersDialogOpenChange: (open: boolean) => void;
  tags: TTag[];
  users: TUser[];
  activeViewId: string;
  onViewChange: (viewId: string) => void;
  viewIds?: KanbanProjectViewIds;
  projectId: string;
  currentUserId: string | null;
  filters: KanbanFilterState<TSortField>;
  showSubIssues: boolean;
  showWorkspaces: boolean;
  hasActiveFilters: boolean;
  onSearchQueryChange: (searchQuery: string) => void;
  onPrioritiesChange: (priorities: PriorityLevel[]) => void;
  onAssigneesChange: (assigneeIds: string[]) => void;
  onTagsChange: (tagIds: string[]) => void;
  onSortChange: (sortField: TSortField, sortDirection: 'asc' | 'desc') => void;
  onShowSubIssuesChange: (show: boolean) => void;
  onShowWorkspacesChange: (show: boolean) => void;
  hideBlocked: boolean;
  onHideBlockedChange: (hide: boolean) => void;
  onClearFilters: () => void;
  onCreateIssue: () => void;
  shouldAnimateCreateButton: boolean;
  isMobile?: boolean;
  /** The Active / All segmented control, rendered after the search. */
  viewSwitcher?: ReactNode;
  renderFiltersDialog?: (
    props: RenderKanbanFiltersDialogProps<TTag, TUser, TSortField>
  ) => ReactNode;
}

function SearchBox({
  value,
  onChange,
  placeholder,
  clearLabel,
  className,
  autoFocus,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  clearLabel: string;
  className?: string;
  autoFocus?: boolean;
}) {
  return (
    <label
      className={cn(
        'flex h-9 items-center gap-2 rounded border border-border bg-secondary px-3 text-low transition-colors',
        'focus-within:border-brand',
        className
      )}
    >
      <MagnifyingGlassIcon className="size-4 shrink-0" aria-hidden="true" />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') e.currentTarget.blur();
        }}
        placeholder={placeholder}
        aria-label={placeholder}
        autoFocus={autoFocus}
        className="min-w-0 flex-1 bg-transparent text-sm text-high outline-none placeholder:text-low [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange('')}
          className="shrink-0 rounded-full text-low hover:text-high"
          aria-label={clearLabel}
          title={clearLabel}
        >
          <XIcon className="size-3.5" weight="bold" />
        </button>
      )}
    </label>
  );
}

/**
 * The board's controls: search, the Active / All and Team / Personal
 * segmented controls, Filter and the primary New card button.
 */
export function KanbanFilterBar<
  TTag extends KanbanFilterTag = KanbanFilterTag,
  TUser extends KanbanFilterUser = KanbanFilterUser,
  TSortField extends string = string,
>({
  isFiltersDialogOpen,
  onFiltersDialogOpenChange,
  tags,
  users,
  activeViewId,
  onViewChange,
  viewIds = DEFAULT_KANBAN_PROJECT_VIEW_IDS,
  projectId,
  currentUserId,
  filters,
  showSubIssues,
  showWorkspaces,
  hasActiveFilters,
  onSearchQueryChange,
  onPrioritiesChange,
  onAssigneesChange,
  onTagsChange,
  onSortChange,
  onShowSubIssuesChange,
  onShowWorkspacesChange,
  hideBlocked,
  onHideBlockedChange,
  onClearFilters,
  onCreateIssue,
  shouldAnimateCreateButton,
  isMobile,
  viewSwitcher,
  renderFiltersDialog,
}: KanbanFilterBarProps<TTag, TUser, TSortField>) {
  const { t } = useTranslation('common');
  const [mobileSearchExpanded, setMobileSearchExpanded] = useState(false);
  const searchPlaceholder = t('kanban.searchCards');
  const clearSearchLabel = t('kanban.clearSearch');

  return (
    <>
      {isMobile && mobileSearchExpanded ? (
        <div className="flex w-full items-center gap-2">
          <button
            type="button"
            onClick={() => {
              onSearchQueryChange('');
              setMobileSearchExpanded(false);
            }}
            className="flex size-9 shrink-0 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high"
            aria-label={t('kanban.closeSearch', 'Close search')}
          >
            <ArrowLeftIcon className="size-4" weight="bold" />
          </button>
          <SearchBox
            value={filters.searchQuery}
            onChange={onSearchQueryChange}
            placeholder={searchPlaceholder}
            clearLabel={clearSearchLabel}
            className="min-w-0 flex-1"
            autoFocus
          />
        </div>
      ) : (
        <div
          className={cn(
            'flex min-w-0 flex-wrap items-center',
            isMobile ? 'gap-2' : 'gap-3'
          )}
        >
          {isMobile ? (
            <button
              type="button"
              onClick={() => setMobileSearchExpanded(true)}
              className={cn(
                'flex size-9 items-center justify-center rounded border border-border bg-secondary transition-colors hover:bg-panel',
                filters.searchQuery ? 'text-brand' : 'text-low'
              )}
              aria-label={searchPlaceholder}
            >
              <MagnifyingGlassIcon className="size-4" />
            </button>
          ) : (
            <SearchBox
              value={filters.searchQuery}
              onChange={onSearchQueryChange}
              placeholder={searchPlaceholder}
              clearLabel={clearSearchLabel}
              className="w-[240px] max-w-full"
            />
          )}

          {viewSwitcher}

          <ButtonGroup>
            <ButtonGroupItem
              active={activeViewId === viewIds.TEAM}
              onClick={() => onViewChange(viewIds.TEAM)}
            >
              {t('kanban.team', 'Team')}
            </ButtonGroupItem>
            <ButtonGroupItem
              active={activeViewId === viewIds.PERSONAL}
              onClick={() => onViewChange(viewIds.PERSONAL)}
            >
              {t('kanban.personal', 'Personal')}
            </ButtonGroupItem>
          </ButtonGroup>

          <Button
            type="button"
            variant="outline"
            size="lg"
            onClick={() => onFiltersDialogOpenChange(true)}
            className="gap-2"
            aria-label={t('kanban.filters', 'Filters')}
          >
            <FunnelIcon
              className={cn('size-4', hasActiveFilters && 'text-brand')}
            />
            {!isMobile && t('kanban.filters', 'Filters')}
            {hasActiveFilters && (
              <span
                aria-hidden="true"
                className="size-1.5 rounded-full bg-brand"
              />
            )}
          </Button>

          {hasActiveFilters && (
            <Button
              type="button"
              variant="ghost"
              size="lg"
              onClick={onClearFilters}
              className="gap-1.5 text-low"
            >
              <XIcon className="size-3.5" weight="bold" />
              {t('kanban.clearFilters', 'Clear filters')}
            </Button>
          )}

          <Button
            type="button"
            size="lg"
            onClick={() => onCreateIssue()}
            className={cn(
              'gap-2',
              isMobile && 'w-9 px-0',
              shouldAnimateCreateButton && 'create-issue-attention'
            )}
            aria-label={t('kanban.newCard')}
          >
            <PlusIcon className="size-4" weight="bold" />
            {!isMobile && t('kanban.newCard')}
          </Button>
        </div>
      )}

      {renderFiltersDialog?.({
        open: isFiltersDialogOpen,
        onOpenChange: onFiltersDialogOpenChange,
        projectId,
        currentUserId,
        tags,
        users,
        filters,
        showSubIssues,
        showWorkspaces,
        onPrioritiesChange,
        onAssigneesChange,
        onTagsChange,
        onSortChange,
        onShowSubIssuesChange,
        onShowWorkspacesChange,
        hideBlocked,
        onHideBlockedChange,
      })}
    </>
  );
}

export interface KanbanBoardTitleProps {
  title: string;
  /** "2 need you": shown when a card waits on a person (or the filter is on). */
  needsYou?: {
    label: string;
    active: boolean;
    onToggle: () => void;
    toggleLabel: string;
  } | null;
  /** The repo slug, set in mono. */
  repo?: string | null;
  /** Default branch, card count, tracker: joined with middots. */
  facts: string[];
  /** The project menu (guide, settings, import, …). */
  menu?: ReactNode;
  isMobile?: boolean;
}

/** Project name (22px, 700) over the muted facts line with the amber pill. */
export function KanbanBoardTitle({
  title,
  needsYou,
  repo,
  facts,
  menu,
  isMobile,
}: KanbanBoardTitleProps) {
  const factLine = facts.filter((fact) => fact.length > 0);
  return (
    <div className="mr-auto flex min-w-0 flex-col gap-1.5">
      <div className="flex min-w-0 items-center gap-1">
        <h1
          className={cn(
            'm-0 truncate font-bold tracking-[-0.3px] text-high',
            isMobile ? 'text-lg' : 'text-[22px] leading-tight'
          )}
        >
          {title}
        </h1>
        {menu}
      </div>
      {(needsYou || repo || factLine.length > 0) && (
        <div className="flex min-w-0 flex-wrap items-center gap-2.5 text-xs text-low">
          {needsYou && (
            <button
              type="button"
              onClick={needsYou.onToggle}
              aria-pressed={needsYou.active}
              title={needsYou.toggleLabel}
              className={cn(
                'inline-flex h-6 items-center gap-1.5 rounded-full px-2 text-xs font-semibold transition-colors',
                'bg-attention-soft text-attention-foreground',
                needsYou.active
                  ? 'ring-1 ring-inset ring-attention'
                  : 'hover:ring-1 hover:ring-inset hover:ring-attention-border',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
              )}
            >
              <span
                aria-hidden="true"
                className="size-2 rounded-full bg-attention"
              />
              {needsYou.label}
              {needsYou.active && (
                <XIcon className="size-3" weight="bold" aria-hidden="true" />
              )}
            </button>
          )}
          {(repo || factLine.length > 0) && (
            <span className="min-w-0 truncate">
              {repo && <span className="font-mono">{repo}</span>}
              {repo && factLine.length > 0 && ' · '}
              {factLine.join(' · ')}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
