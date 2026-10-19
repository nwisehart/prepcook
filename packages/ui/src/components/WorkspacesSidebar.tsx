import type { ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';
import {
  PlusIcon,
  ArrowLeftIcon,
  ArchiveIcon,
  StackIcon,
  SpinnerIcon,
  CaretDownIcon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { InputField } from './InputField';
import { IconButton } from './IconButton';
import {
  WorkspaceSummary,
  type WorkspaceSummaryStageTone,
  type WorkspaceSummaryStatusTone,
} from './WorkspaceSummary';

export type WorkspaceLayoutMode = 'flat' | 'accordion';

export type WorkspacesSidebarGroup =
  | 'needs-you'
  | 'running'
  | 'done-today'
  | 'earlier';

export interface WorkspacesSidebarWorkspace {
  id: string;
  name: string;
  /** Card key, or the branch when the session has no card */
  cardKey?: string;
  stageLabel?: string;
  stageTone?: WorkspaceSummaryStageTone;
  statusLabel?: string;
  statusTone?: WorkspaceSummaryStatusTone;
  /** Which status group the session belongs to */
  group: WorkspacesSidebarGroup;
  isMuted?: boolean;
  isPinned?: boolean;
  hasRunningDevServer?: boolean;
}

export interface WorkspacesSidebarPersistKeys {
  raisedHand: string;
  notRunning: string;
  running: string;
  earlier?: string;
}

const EARLIER_PERSIST_KEY = 'workspaces-sidebar-earlier';

const DEFAULT_PERSIST_KEYS: WorkspacesSidebarPersistKeys = {
  raisedHand: 'workspaces-sidebar-raised-hand',
  notRunning: 'workspaces-sidebar-not-running',
  running: 'workspaces-sidebar-running',
  earlier: EARLIER_PERSIST_KEY,
};

// Shared with CollapsibleSectionHeader so a group's stored open/closed state
// survives whichever header renders it.
const COLLAPSE_STORAGE_PREFIX = 'vibe.ui.collapsible.';

function readStoredOpen(key: string, fallback: boolean): boolean {
  if (typeof window === 'undefined') return fallback;
  try {
    const stored = window.localStorage.getItem(
      `${COLLAPSE_STORAGE_PREFIX}${key}`
    );
    return stored == null ? fallback : stored === 'true';
  } catch {
    return fallback;
  }
}

function usePersistedOpen(key: string) {
  const [open, setOpen] = useState(() => readStoredOpen(key, true));

  useEffect(() => {
    try {
      window.localStorage.setItem(
        `${COLLAPSE_STORAGE_PREFIX}${key}`,
        String(open)
      );
    } catch {
      // Ignore localStorage failures (private mode/quota/security errors).
    }
  }, [key, open]);

  return [open, setOpen] as const;
}

export interface WorkspacesSidebarProps {
  workspaces: WorkspacesSidebarWorkspace[];
  totalWorkspacesCount: number;
  /** Sessions active today (needs you, running or finished today) */
  todayCount?: number;
  archivedWorkspaces?: WorkspacesSidebarWorkspace[];
  isLoading?: boolean;
  selectedWorkspaceId: string | null;
  onSelectWorkspace: (id: string) => void;
  onAddWorkspace?: () => void;
  searchQuery: string;
  onSearchChange: (value: string) => void;
  /** Whether we're in create mode */
  isCreateMode?: boolean;
  /** Title extracted from draft message (only shown when isCreateMode and non-empty) */
  draftTitle?: string;
  /** Handler to navigate back to create mode */
  onSelectCreate?: () => void;
  /** Whether to show archived workspaces */
  showArchive?: boolean;
  /** Handler for toggling archive view */
  onShowArchiveChange?: (show: boolean) => void;
  /** Layout mode for active workspaces */
  layoutMode?: WorkspaceLayoutMode;
  /** Handler for toggling layout mode */
  onToggleLayoutMode?: () => void;
  /** Handler to load more workspaces on scroll */
  onLoadMore?: () => void;
  /** Whether there are more workspaces to load */
  hasMoreWorkspaces?: boolean;
  /** Controls rendered beside the search input */
  searchControls?: ReactNode;
  /** Callback for opening workspace actions */
  onOpenWorkspaceActions?: (workspaceId: string) => void;
  /** Persist keys for collapsible sections */
  persistKeys?: WorkspacesSidebarPersistKeys;
}

export interface WorkspacesSidebarReopenTagProps {
  active?: boolean;
  onHoverStart?: () => void;
  onHoverEnd?: () => void;
  ariaLabel?: string;
  className?: string;
}

export function WorkspacesSidebarReopenTag({
  active = false,
  onHoverStart,
  onHoverEnd,
  ariaLabel,
  className,
}: WorkspacesSidebarReopenTagProps) {
  return (
    <button
      type="button"
      onMouseEnter={onHoverStart}
      onMouseLeave={onHoverEnd}
      aria-label={ariaLabel ?? 'Preview workspaces sidebar'}
      title={ariaLabel ?? 'Preview workspaces sidebar'}
      className={cn(
        'group inline-flex h-24 w-4 items-center justify-center rounded-md border border-border bg-secondary/95 transition-colors cursor-e-resize',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
        active ? 'bg-panel text-normal' : 'text-low hover:text-normal',
        className
      )}
    >
      <span className="grid grid-cols-2 gap-[2px]">
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
        <span className="size-dot rounded-full bg-low/70 group-hover:bg-low" />
      </span>
    </button>
  );
}

const sectionLabelClassName =
  'text-[11px] font-bold uppercase tracking-[0.08em] text-low';

function SessionGroupSection({
  label,
  persistKey,
  children,
}: {
  label: string;
  persistKey: string;
  children: ReactNode;
}) {
  const [open, setOpen] = usePersistedOpen(persistKey);

  return (
    <section className="flex flex-col gap-0.5">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className={cn(
          'group/label flex items-center gap-1 rounded-sm px-2.5 pb-1 pt-2 text-left hover:text-normal',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
          sectionLabelClassName
        )}
      >
        <span>{label}</span>
        <CaretDownIcon
          className={cn(
            'size-3 transition-transform',
            open
              ? 'opacity-0 group-hover/label:opacity-100 group-focus-visible/label:opacity-100'
              : '-rotate-90'
          )}
          weight="bold"
          aria-hidden
        />
      </button>
      {open && <div className="flex flex-col gap-0.5">{children}</div>}
    </section>
  );
}

function SessionRows({
  workspaces,
  selectedWorkspaceId,
  onSelectWorkspace,
  onOpenWorkspaceActions,
}: {
  workspaces: WorkspacesSidebarWorkspace[];
  selectedWorkspaceId: string | null;
  onSelectWorkspace: (id: string) => void;
  onOpenWorkspaceActions?: (workspaceId: string) => void;
}) {
  return (
    <>
      {workspaces.map((workspace) => (
        <WorkspaceSummary
          key={workspace.id}
          name={workspace.name}
          workspaceId={workspace.id}
          cardKey={workspace.cardKey}
          stageLabel={workspace.stageLabel}
          stageTone={workspace.stageTone}
          statusLabel={workspace.statusLabel}
          statusTone={workspace.statusTone}
          isMuted={workspace.isMuted}
          isActive={selectedWorkspaceId === workspace.id}
          isPinned={workspace.isPinned}
          hasRunningDevServer={workspace.hasRunningDevServer}
          onOpenWorkspaceActions={onOpenWorkspaceActions}
          onClick={() => onSelectWorkspace(workspace.id)}
        />
      ))}
    </>
  );
}

export function WorkspacesSidebar({
  workspaces,
  totalWorkspacesCount,
  todayCount,
  archivedWorkspaces = [],
  isLoading = false,
  selectedWorkspaceId,
  onSelectWorkspace,
  onAddWorkspace,
  searchQuery,
  onSearchChange,
  isCreateMode = false,
  draftTitle,
  onSelectCreate,
  showArchive = false,
  onShowArchiveChange,
  layoutMode = 'flat',
  onToggleLayoutMode,
  onLoadMore,
  hasMoreWorkspaces = false,
  searchControls,
  onOpenWorkspaceActions,
  persistKeys = DEFAULT_PERSIST_KEYS,
}: WorkspacesSidebarProps) {
  const { t } = useTranslation(['tasks', 'common']);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Handle scroll to load more
  const handleScroll = () => {
    if (!hasMoreWorkspaces || !onLoadMore) return;

    const container = scrollContainerRef.current;
    if (!container) return;

    const { scrollTop, scrollHeight, clientHeight } = container;
    // Load more when scrolled within 100px of the bottom
    if (scrollHeight - scrollTop - clientHeight < 100) {
      onLoadMore();
    }
  };

  const draftRow = draftTitle ? (
    <WorkspaceSummary
      name={draftTitle}
      isActive={isCreateMode}
      isDraft={true}
      onClick={onSelectCreate}
    />
  ) : null;

  const groups: {
    id: WorkspacesSidebarGroup;
    label: string;
    persistKey: string;
  }[] = [
    {
      id: 'needs-you',
      label: t('common:workspaces.groups.needsYou'),
      persistKey: persistKeys.raisedHand,
    },
    {
      id: 'running',
      label: t('common:workspaces.running'),
      persistKey: persistKeys.running,
    },
    {
      id: 'done-today',
      label: t('common:workspaces.groups.doneToday'),
      persistKey: persistKeys.notRunning,
    },
    {
      id: 'earlier',
      label: t('common:workspaces.groups.earlier'),
      persistKey: persistKeys.earlier ?? EARLIER_PERSIST_KEY,
    },
  ];

  const emptyState = (
    <span className="px-2.5 py-1 text-sm text-low">
      {t('common:workspaces.noSessions')}
    </span>
  );

  const renderGrouped = () => {
    const visibleGroups = groups
      .map((group) => ({
        ...group,
        rows: workspaces.filter((ws) => ws.group === group.id),
      }))
      .filter(
        (group) =>
          group.rows.length > 0 || (group.id === 'needs-you' && draftRow)
      );

    if (visibleGroups.length === 0) return emptyState;

    return visibleGroups.map((group) => (
      <SessionGroupSection
        key={group.id}
        label={group.label}
        persistKey={group.persistKey}
      >
        {group.id === 'needs-you' && draftRow}
        <SessionRows
          workspaces={group.rows}
          selectedWorkspaceId={selectedWorkspaceId}
          onSelectWorkspace={onSelectWorkspace}
          onOpenWorkspaceActions={onOpenWorkspaceActions}
        />
      </SessionGroupSection>
    ));
  };

  return (
    <div className="flex h-full w-full flex-col border-r bg-primary">
      {/* Header + search */}
      <div className="flex flex-col gap-2 px-2.5 pt-[18px]">
        <div className="flex items-center justify-between gap-2 px-2.5 pb-1">
          <div className="flex min-w-0 items-baseline gap-2">
            <h2 className="text-lg font-bold text-high">
              {t('common:workspaces.sessionsTitle')}
            </h2>
            {!isLoading && todayCount !== undefined && (
              <span className="truncate text-[12.5px] text-low">
                {t('common:workspaces.todayCount', { count: todayCount })}
              </span>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-0.5">
            {onToggleLayoutMode && (
              <IconButton
                icon={StackIcon}
                onClick={onToggleLayoutMode}
                aria-label={t('common:workspaces.groupByStatus')}
                title={t('common:workspaces.groupByStatus')}
                className={cn(
                  'size-7',
                  layoutMode === 'accordion' && 'text-brand hover:text-brand'
                )}
                iconClassName="size-4"
              />
            )}
            {onAddWorkspace && (
              <IconButton
                icon={PlusIcon}
                onClick={onAddWorkspace}
                aria-label={t('common:workspaces.newSession')}
                title={t('common:workspaces.newSession')}
                className="size-7"
                iconClassName="size-4"
              />
            )}
          </div>
        </div>
        {!isLoading && (
          <div className="flex items-stretch gap-1">
            <div className="min-w-0 flex-1">
              <InputField
                variant="search"
                value={searchQuery}
                onChange={onSearchChange}
                placeholder={t('common:workspaces.searchPlaceholder')}
              />
            </div>
            {searchControls}
          </div>
        )}
      </div>

      {/* Scrollable session list */}
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-2.5 pb-3 pt-1"
      >
        {isLoading ? (
          <div className="flex h-full min-h-[220px] items-center justify-center">
            <SpinnerIcon
              className="size-6 animate-spin text-low"
              weight="bold"
            />
          </div>
        ) : showArchive ? (
          <section className="flex flex-col gap-0.5">
            <span className={cn('px-2.5 pb-1 pt-2', sectionLabelClassName)}>
              {t('common:workspaces.archived')}
            </span>
            {archivedWorkspaces.length === 0 ? (
              <span className="px-2.5 py-1 text-sm text-low">
                {t('common:workspaces.noArchivedSessions')}
              </span>
            ) : (
              <SessionRows
                workspaces={archivedWorkspaces.map((ws) => ({
                  ...ws,
                  isMuted: true,
                }))}
                selectedWorkspaceId={selectedWorkspaceId}
                onSelectWorkspace={onSelectWorkspace}
                onOpenWorkspaceActions={onOpenWorkspaceActions}
              />
            )}
          </section>
        ) : layoutMode === 'accordion' ? (
          <div className="flex flex-col gap-1">{renderGrouped()}</div>
        ) : (
          <section className="flex flex-col gap-0.5">
            <div
              className={cn(
                'flex items-center justify-between px-2.5 pb-1 pt-2',
                sectionLabelClassName
              )}
            >
              <span>{t('common:workspaces.active')}</span>
              <span className="font-medium tracking-normal">
                {totalWorkspacesCount}
              </span>
            </div>
            {draftRow}
            {workspaces.length === 0 && !draftRow ? (
              emptyState
            ) : (
              <SessionRows
                workspaces={workspaces}
                selectedWorkspaceId={selectedWorkspaceId}
                onSelectWorkspace={onSelectWorkspace}
                onOpenWorkspaceActions={onOpenWorkspaceActions}
              />
            )}
          </section>
        )}
      </div>

      {/* Footer: archive toggle */}
      <div className="border-t px-2.5 py-2">
        <button
          type="button"
          onClick={() => onShowArchiveChange?.(!showArchive)}
          className={cn(
            'flex h-8 w-full items-center gap-2 rounded px-2.5 text-sm text-low transition-colors duration-100 hover:bg-panel hover:text-high',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand'
          )}
        >
          {showArchive ? (
            <>
              <ArrowLeftIcon className="size-4" />
              <span>{t('common:workspaces.backToActive')}</span>
            </>
          ) : (
            <>
              <ArchiveIcon className="size-4" />
              <span>{t('common:workspaces.viewArchive')}</span>
              <span className="ml-auto inline-flex h-5 items-center rounded-full bg-panel px-1.5 text-xs font-medium text-normal">
                {archivedWorkspaces.length}
              </span>
            </>
          )}
        </button>
      </div>
    </div>
  );
}
