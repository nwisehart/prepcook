import {
  useMemo,
  useCallback,
  useState,
  useEffect,
  useRef,
  type MouseEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { useProjectContext } from '@/shared/hooks/useProjectContext';
import { useOrgContext } from '@/shared/hooks/useOrgContext';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { useActions } from '@/shared/hooks/useActions';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useIsMobile } from '@/shared/hooks/useIsMobile';
import { cn } from '@/shared/lib/utils';
import { useCurrentKanbanRouteState } from '@/shared/hooks/useCurrentKanbanRouteState';
import {
  useUiPreferencesStore,
  resolveKanbanProjectState,
  usePersistedExpanded,
  KANBAN_ASSIGNEE_FILTER_VALUES,
  KANBAN_PROJECT_VIEW_IDS,
  PERSIST_KEYS,
  type KanbanFilterState,
  type KanbanSortField,
} from '@/shared/stores/useUiPreferencesStore';
import {
  useKanbanFilters,
  issueEpicKeys,
  epicColorFromKey,
  PRIORITY_ORDER,
} from '../model/hooks/useKanbanFilters';
import {
  cardCleanupGate,
  columnDotTone,
  withDiscardConfirmed,
  implementationSession,
  isTakenOver,
  isWithinLastWeek,
  latestPullRequest,
  needsYouReason,
  priorityChipLabel,
  shippedIndicator,
  workClassChipTone,
  workflowStageKey,
  type NeedsYouReason,
  type ShippedIndicator,
  type WorkflowStageKey,
} from '../model/cardSignals';
import {
  buildStatusLine,
  columnHelper,
  needsYouLabel,
  planGateDetail,
  pullRequestLabel,
  shippedIndicatorLabel,
} from '../model/cardCopy';
import { postPlanDecision } from '../model/planDecisionsApi';
import {
  bulkUpdateIssues,
  type BulkUpdateIssueItem,
} from '@/shared/lib/remoteApi';
import { DotsThreeIcon } from '@phosphor-icons/react';
import { Actions } from '@/shared/actions';
import {
  buildKanbanIssueComposerKey,
  closeKanbanIssueComposer,
  openKanbanIssueComposer,
  type ProjectIssueCreateOptions,
  useKanbanIssueComposer,
} from '@/shared/stores/useKanbanIssueComposerStore';
import type { OrganizationMemberWithProfile } from 'shared/types';
import {
  KanbanProvider,
  KanbanBoard,
  KanbanCard,
  KanbanCards,
  KanbanColumnFoldButton,
  KanbanColumnGroupLabel,
  KanbanColumnHeader,
  KanbanColumnSearch,
  KanbanFoldedColumn,
  type DragStart,
  type DropResult,
} from '@prepcook/ui/components/KanbanBoard';
import { ConfirmDialog } from '@prepcook/ui/components/ConfirmDialog';
import { useUserContext } from '@/shared/hooks/useUserContext';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useProjectWorkspaceCreateDraft } from '@/shared/hooks/useProjectWorkspaceCreateDraft';
import { getWorkspaceDefaults } from '@/shared/lib/workspaceDefaults';
import {
  buildLinkedIssueCreateState,
  buildLocalWorkspaceIdSet,
  buildWorkspaceCreateInitialState,
} from '@/shared/lib/workspaceCreateState';
import {
  interpolatePlaybookTemplate,
  resolveStage,
  resolveStagePlaybook,
} from '@/shared/lib/stagePlaybooks';
import { cardsApi, playbooksApi, repoApi } from '@/shared/lib/api';
import { cardOrigin, cardWorkClass } from '@/shared/lib/workClass';
import { parseIssueExtMetadata } from '@/shared/lib/stagePlaybooks';
import {
  isTransitionAllowed,
  planningGateError,
} from '@/shared/lib/workflowTransitions';
import {
  KanbanArchivedCardContent,
  KanbanCardContent,
  type KanbanCardIndicatorTone,
  type KanbanCardNeedsYou,
  type KanbanClassLabel,
} from '@prepcook/ui/components/KanbanCardContent';
import { formatRelativeTime } from '@prepcook/ui/components/KanbanRelativeTime';
import type {
  WorkspacePr,
  WorkspaceWithStats,
} from '@prepcook/ui/components/IssueWorkspaceCard';
import { resolveRelationshipsForIssue } from '@/shared/lib/resolveRelationships';
import {
  KanbanBoardTitle,
  KanbanFilterBar,
} from '@prepcook/ui/components/KanbanFilterBar';
import { ViewNavTabs } from '@prepcook/ui/components/ViewNavTabs';
import { IssueListView } from '@prepcook/ui/components/IssueListView';
import { CommandBarDialog } from '@/shared/dialogs/command-bar/CommandBarDialog';
import { KanbanFiltersDialog } from '@/shared/dialogs/kanban/KanbanFiltersDialog';
import { ImportFromTrackerDialog } from '@/shared/dialogs/kanban/ImportFromTrackerDialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@prepcook/ui/components/Dropdown';
import { SearchableTagDropdownContainer } from '@/shared/components/SearchableTagDropdownContainer';
import type { IssuePriority, JsonValue } from 'shared/remote-types';
import { useIssueMultiSelect } from '@/shared/hooks/useIssueMultiSelect';
import { useIssueSelectionStore } from '@/shared/stores/useIssueSelectionStore';
import { BulkActionBarContainer } from './BulkActionBarContainer';
import { confirmDiscard } from './confirmDiscard';

/** Cleaned-up cards shown before "Show N more". */
const CLEAN_UP_PAGE_SIZE = 10;

/** Tags the orchestrator sets; the card says them in its status instead. */
const SYSTEM_TAG_NAMES = new Set(['agent-failed', 'taken-over']);

/** The Shipped chip's recipe for each pull-request state. */
const DONE_INDICATOR_TONES: Record<
  ShippedIndicator['kind'],
  KanbanCardIndicatorTone
> = {
  open: 'brand',
  ciFailing: 'error',
  merged: 'success',
  closed: 'neutral',
};

const areStringSetsEqual = (left: string[], right: string[]): boolean => {
  if (left.length !== right.length) {
    return false;
  }

  const rightSet = new Set(right);
  return left.every((value) => rightSet.has(value));
};

const areKanbanFiltersEqual = (
  left: KanbanFilterState,
  right: KanbanFilterState
): boolean => {
  if (left.searchQuery.trim() !== right.searchQuery.trim()) {
    return false;
  }

  if (!areStringSetsEqual(left.priorities, right.priorities)) {
    return false;
  }

  if (!areStringSetsEqual(left.assigneeIds, right.assigneeIds)) {
    return false;
  }

  if (!areStringSetsEqual(left.tagIds, right.tagIds)) {
    return false;
  }

  return (
    left.sortField === right.sortField &&
    left.sortDirection === right.sortDirection
  );
};

function LoadingState() {
  const { t } = useTranslation('common');
  return (
    <div className="flex items-center justify-center h-full">
      <p className="text-low">{t('states.loading')}</p>
    </div>
  );
}

/**
 * KanbanContainer displays the kanban board using data from ProjectContext and OrgContext.
 * Must be rendered within both OrgProvider and ProjectProvider.
 */
export function KanbanContainer() {
  const isMobile = useIsMobile();
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const routeState = useCurrentKanbanRouteState();

  // Get data from contexts (set up by WorkspacesLayout)
  const {
    projectId,
    issues,
    statuses,
    tags,
    issueAssignees,
    issueTags,
    issueRelationships,
    getTagObjectsForIssue,
    getTagsForIssue,
    getPullRequestsForIssue,
    getWorkspacesForIssue,
    getRelationshipsForIssue,
    issuesById,
    insertIssueTag,
    removeIssueTag,
    insertTag,
    updateIssue,
    pullRequests,
    isLoading: projectLoading,
  } = useProjectContext();

  const {
    projects,
    membersWithProfilesById,
    isLoading: orgLoading,
  } = useOrgContext();
  const { activeWorkspaces, archivedWorkspaces } = useWorkspaceContext();
  const { userId } = useAuth();

  // Stage-playbook confirm flow (drop into Planning/Executing)
  const { workspaces } = useUserContext();
  const { config } = useUserSystem();
  const { openWorkspaceCreateFromState } = useProjectWorkspaceCreateDraft();
  const localWorkspaceIds = useMemo(
    () => buildLocalWorkspaceIdSet(activeWorkspaces, archivedWorkspaces),
    [activeWorkspaces, archivedWorkspaces]
  );

  // Get project name by finding the project matching current projectId
  const projectName = projects.find((p) => p.id === projectId)?.name ?? '';

  const selectedKanbanIssueId = routeState.issueId;
  const issueComposerKey = useMemo(
    () => buildKanbanIssueComposerKey(projectId),
    [projectId]
  );
  const issueComposer = useKanbanIssueComposer(issueComposerKey);
  const isIssueComposerOpen = issueComposer !== null;
  const openIssue = useCallback(
    (issueId: string) => {
      if (isIssueComposerOpen) {
        closeKanbanIssueComposer(issueComposerKey);
      }

      appNavigation.goToProjectIssue(projectId, issueId);
    },
    [isIssueComposerOpen, issueComposerKey, appNavigation, projectId]
  );
  const openIssueWorkspace = useCallback(
    (issueId: string, workspaceAttemptId: string) => {
      appNavigation.goToProjectIssueWorkspace(
        projectId,
        issueId,
        workspaceAttemptId
      );
    },
    [appNavigation, projectId]
  );
  const startCreate = useCallback(
    (options?: ProjectIssueCreateOptions) => {
      openKanbanIssueComposer(issueComposerKey, options);
    },
    [issueComposerKey]
  );

  // Get setter and executor from ActionsContext
  const {
    setDefaultCreateStatusId,
    executeAction,
    openPrioritySelection,
    openAssigneeSelection,
  } = useActions();
  const openProjectsGuide = useCallback(() => {
    executeAction(Actions.ProjectsGuide);
  }, [executeAction]);

  const projectViewSelection = useUiPreferencesStore(
    (s) => s.kanbanProjectViewSelections[projectId]
  );
  const projectViewPreferencesById = useUiPreferencesStore(
    (s) => s.kanbanProjectViewPreferences[projectId]
  );
  const setKanbanProjectView = useUiPreferencesStore(
    (s) => s.setKanbanProjectView
  );
  const setKanbanProjectViewFilters = useUiPreferencesStore(
    (s) => s.setKanbanProjectViewFilters
  );
  const setKanbanProjectViewShowSubIssues = useUiPreferencesStore(
    (s) => s.setKanbanProjectViewShowSubIssues
  );
  const setKanbanProjectViewShowWorkspaces = useUiPreferencesStore(
    (s) => s.setKanbanProjectViewShowWorkspaces
  );
  const setKanbanProjectViewHideBlocked = useUiPreferencesStore(
    (s) => s.setKanbanProjectViewHideBlocked
  );
  const clearKanbanProjectViewPreferences = useUiPreferencesStore(
    (s) => s.clearKanbanProjectViewPreferences
  );
  const resolvedProjectState = useMemo(
    () => resolveKanbanProjectState(projectViewSelection),
    [projectViewSelection]
  );
  const {
    activeViewId,
    filters: defaultKanbanFilters,
    showSubIssues: defaultShowSubIssues,
    showWorkspaces: defaultShowWorkspaces,
    hideBlocked: defaultHideBlocked,
  } = resolvedProjectState;
  const projectViewPreferences = projectViewPreferencesById?.[activeViewId];
  const kanbanFilters = projectViewPreferences?.filters ?? defaultKanbanFilters;
  const showSubIssues =
    projectViewPreferences?.showSubIssues ?? defaultShowSubIssues;
  const showWorkspaces =
    projectViewPreferences?.showWorkspaces ?? defaultShowWorkspaces;
  const hideBlocked = projectViewPreferences?.hideBlocked ?? defaultHideBlocked;

  const hasActiveFilters = useMemo(
    () =>
      !areKanbanFiltersEqual(kanbanFilters, defaultKanbanFilters) ||
      showSubIssues !== defaultShowSubIssues ||
      showWorkspaces !== defaultShowWorkspaces ||
      hideBlocked !== defaultHideBlocked,
    [
      kanbanFilters,
      defaultKanbanFilters,
      showSubIssues,
      defaultShowSubIssues,
      showWorkspaces,
      defaultShowWorkspaces,
      hideBlocked,
      defaultHideBlocked,
    ]
  );
  const shouldAnimateCreateButton = issues.length === 0;

  // Compute resolved status IDs for the blocked filter.
  // A blocking issue is considered resolved when it's in:
  // - The last visible status (rightmost kanban column, e.g. "Done")
  // - Any hidden status (terminal states like "Cancelled" that don't appear as columns)
  const doneStatusIds = useMemo(() => {
    const ids = new Set<string>();
    for (const s of statuses) {
      if (s.hidden) ids.add(s.id);
    }
    const sorted = statuses
      .filter((s) => !s.hidden)
      .sort((a, b) => a.sort_order - b.sort_order);
    const lastVisible = sorted[sorted.length - 1];
    if (lastVisible) ids.add(lastVisible.id);
    return ids;
  }, [statuses]);

  // Epic tab selection: null = whole board, otherwise an epic's Jira key.
  // Local state (resets on project switch), matching the cheap filter-tab
  // approach: subtasks are matched by extension_metadata.epic_key.
  const [epicFilter, setEpicFilter] = useState<string | null>(null);
  // "N need you" header pill: when on, the board shows only the cards that
  // wait on a person. Local, like the epic tabs; resets on project switch.
  const [needsYouOnly, setNeedsYouOnly] = useState(false);
  const [cleanUpQuery, setCleanUpQuery] = useState('');
  const [showAllCleanedUp, setShowAllCleanedUp] = useState(false);
  useEffect(() => {
    setEpicFilter(null);
    setNeedsYouOnly(false);
    setCleanUpQuery('');
    setShowAllCleanedUp(false);
  }, [projectId]);

  // The Clean up column folds to a rail by default; unfolding persists.
  const [isCleanUpExpanded, setCleanUpExpanded] = usePersistedExpanded(
    PERSIST_KEYS.kanbanCleanUpExpanded,
    false
  );

  // Status lines show elapsed and relative times: re-render once a minute.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);

  // Statuses counting as "done" for epic progress: the workflow's terminal
  // columns when present, otherwise the generic done set. A merged card in
  // Shipped counts too (see `epicSummaries`).
  const epicDoneStatusIds = useMemo(() => {
    const terminal = statuses.filter((s) =>
      [
        'close',
        'deployed',
        'closed',
        'clean up',
        'clean-up',
        'cleanup',
      ].includes(s.name.trim().toLowerCase())
    );
    if (terminal.length === 0) return doneStatusIds;
    return new Set(terminal.map((s) => s.id));
  }, [statuses, doneStatusIds]);

  // Epics present on the board (work_class epic with a Jira key), plus
  // subtask done/total counts per epic for progress bars.
  const epicSummaries = useMemo(() => {
    const summaries = new Map<
      string,
      { title: string; done: number; total: number }
    >();
    for (const issue of issues) {
      const meta =
        issue.extension_metadata && typeof issue.extension_metadata === 'object'
          ? (issue.extension_metadata as Record<string, unknown>)
          : {};
      if (meta.work_class !== 'epic') continue;
      const { jiraKey } = issueEpicKeys(issue);
      if (!jiraKey) continue;
      summaries.set(jiraKey, { title: issue.title, done: 0, total: 0 });
    }
    for (const issue of issues) {
      const { epicKey } = issueEpicKeys(issue);
      if (!epicKey) continue;
      const summary = summaries.get(epicKey);
      if (!summary) continue;
      summary.total += 1;
      const merged =
        parseIssueExtMetadata(issue.extension_metadata)?.merged_at != null;
      if (epicDoneStatusIds.has(issue.status_id) || merged) summary.done += 1;
    }
    return summaries;
  }, [issues, epicDoneStatusIds]);

  const { filteredIssues } = useKanbanFilters({
    issues,
    issueAssignees,
    issueTags,
    issueRelationships,
    issuesById,
    doneStatusIds,
    filters: kanbanFilters,
    showSubIssues,
    hideBlocked,
    currentUserId: userId,
    epicKey: epicFilter,
  });

  // Workflow stage per column, matched on the stage names and their aliases.
  const stageByStatusId = useMemo(() => {
    const map = new Map<string, WorkflowStageKey | null>();
    for (const status of statuses) {
      map.set(status.id, workflowStageKey(status.name));
    }
    return map;
  }, [statuses]);

  // Why each card in the current view waits on a person: its plan is ready
  // for a decision in Plan, it is a triaged investigation, or it sits in Shipped
  // with its pull request merged or closed without merging. The header pill
  // counts these and can filter the board to them.
  const needsYouByIssueId = useMemo(() => {
    const map = new Map<string, NeedsYouReason>();
    for (const issue of filteredIssues) {
      const reason = needsYouReason(
        stageByStatusId.get(issue.status_id) ?? null,
        parseIssueExtMetadata(issue.extension_metadata),
        getPullRequestsForIssue(issue.id)
      );
      if (reason) map.set(issue.id, reason);
    }
    return map;
  }, [filteredIssues, stageByStatusId, getPullRequestsForIssue]);

  const boardIssues = useMemo(
    () =>
      needsYouOnly
        ? filteredIssues.filter((issue) => needsYouByIssueId.has(issue.id))
        : filteredIssues,
    [needsYouOnly, filteredIssues, needsYouByIssueId]
  );

  const setKanbanSearchQuery = useCallback(
    (searchQuery: string) => {
      setKanbanProjectViewFilters(projectId, activeViewId, {
        ...kanbanFilters,
        searchQuery,
      });
    },
    [activeViewId, kanbanFilters, projectId, setKanbanProjectViewFilters]
  );

  const setKanbanPriorities = useCallback(
    (priorities: IssuePriority[]) => {
      setKanbanProjectViewFilters(projectId, activeViewId, {
        ...kanbanFilters,
        priorities,
      });
    },
    [activeViewId, kanbanFilters, projectId, setKanbanProjectViewFilters]
  );

  const setKanbanAssignees = useCallback(
    (assigneeIds: string[]) => {
      setKanbanProjectViewFilters(projectId, activeViewId, {
        ...kanbanFilters,
        assigneeIds,
      });
    },
    [activeViewId, kanbanFilters, projectId, setKanbanProjectViewFilters]
  );

  const setKanbanTags = useCallback(
    (tagIds: string[]) => {
      setKanbanProjectViewFilters(projectId, activeViewId, {
        ...kanbanFilters,
        tagIds,
      });
    },
    [activeViewId, kanbanFilters, projectId, setKanbanProjectViewFilters]
  );

  const setKanbanSort = useCallback(
    (sortField: KanbanSortField, sortDirection: 'asc' | 'desc') => {
      setKanbanProjectViewFilters(projectId, activeViewId, {
        ...kanbanFilters,
        sortField,
        sortDirection,
      });
    },
    [activeViewId, kanbanFilters, projectId, setKanbanProjectViewFilters]
  );

  const setShowSubIssues = useCallback(
    (show: boolean) => {
      setKanbanProjectViewShowSubIssues(projectId, activeViewId, show);
    },
    [activeViewId, projectId, setKanbanProjectViewShowSubIssues]
  );

  const setShowWorkspaces = useCallback(
    (show: boolean) => {
      setKanbanProjectViewShowWorkspaces(projectId, activeViewId, show);
    },
    [activeViewId, projectId, setKanbanProjectViewShowWorkspaces]
  );

  const setHideBlocked = useCallback(
    (hide: boolean) => {
      setKanbanProjectViewHideBlocked(projectId, activeViewId, hide);
    },
    [activeViewId, projectId, setKanbanProjectViewHideBlocked]
  );

  const clearKanbanFilters = useCallback(() => {
    clearKanbanProjectViewPreferences(projectId, activeViewId);
  }, [activeViewId, clearKanbanProjectViewPreferences, projectId]);

  const handleKanbanProjectViewChange = useCallback(
    (viewId: string) => {
      setKanbanProjectView(projectId, viewId);
    },
    [projectId, setKanbanProjectView]
  );
  const kanbanViewMode = useUiPreferencesStore((s) => s.kanbanViewMode);
  const listViewStatusFilter = useUiPreferencesStore(
    (s) => s.listViewStatusFilter
  );
  const setKanbanViewMode = useUiPreferencesStore((s) => s.setKanbanViewMode);
  const setListViewStatusFilter = useUiPreferencesStore(
    (s) => s.setListViewStatusFilter
  );
  // Reset view mode when navigating projects
  const prevProjectIdRef = useRef<string | null>(null);

  // Track when drag-drop sync is in progress to prevent flicker
  const isSyncingRef = useRef(false);

  useEffect(() => {
    if (
      prevProjectIdRef.current !== null &&
      prevProjectIdRef.current !== projectId
    ) {
      setKanbanViewMode('kanban');
      setListViewStatusFilter(null);
    }

    prevProjectIdRef.current = projectId;
  }, [projectId, setKanbanViewMode, setListViewStatusFilter]);

  // Sort all statuses for display settings
  const sortedStatuses = useMemo(
    () => [...statuses].sort((a, b) => a.sort_order - b.sort_order),
    [statuses]
  );

  // Filter statuses: visible (non-hidden) for kanban, hidden for tabs
  const visibleStatuses = useMemo(
    () => sortedStatuses.filter((s) => !s.hidden),
    [sortedStatuses]
  );

  // Map status ID to 1-based column index for sort_order calculation
  const statusColumnIndexMap = useMemo(() => {
    const map = new Map<string, number>();
    visibleStatuses.forEach((status, index) => {
      map.set(status.id, index + 1);
    });
    return map;
  }, [visibleStatuses]);

  const hiddenStatuses = useMemo(
    () => sortedStatuses.filter((s) => s.hidden),
    [sortedStatuses]
  );

  const defaultCreateStatusId = useMemo(() => {
    if (kanbanViewMode === 'kanban') {
      return visibleStatuses[0]?.id;
    }
    if (listViewStatusFilter) {
      return listViewStatusFilter;
    }
    return sortedStatuses[0]?.id;
  }, [kanbanViewMode, visibleStatuses, listViewStatusFilter, sortedStatuses]);

  // Update default create status for command bar based on current tab
  useEffect(() => {
    setDefaultCreateStatusId(defaultCreateStatusId);
  }, [defaultCreateStatusId, setDefaultCreateStatusId]);

  const createAssigneeIds = useMemo(() => {
    const assigneeIds = new Set<string>();

    for (const assigneeId of kanbanFilters.assigneeIds) {
      if (assigneeId === KANBAN_ASSIGNEE_FILTER_VALUES.UNASSIGNED) {
        continue;
      }

      if (assigneeId === KANBAN_ASSIGNEE_FILTER_VALUES.SELF) {
        if (userId) {
          assigneeIds.add(userId);
        }
        continue;
      }

      assigneeIds.add(assigneeId);
    }

    return [...assigneeIds];
  }, [kanbanFilters.assigneeIds, userId]);

  // Get statuses to display in list view (all or filtered to one)
  const listViewStatuses = useMemo(() => {
    if (listViewStatusFilter) {
      return sortedStatuses.filter((s) => s.id === listViewStatusFilter);
    }
    return sortedStatuses;
  }, [sortedStatuses, listViewStatusFilter]);

  // Track items as arrays of IDs grouped by status
  const [items, setItems] = useState<Record<string, string[]>>({});
  const [isFiltersDialogOpen, setIsFiltersDialogOpen] = useState(false);

  // Rebuild the per-status ID lists from the source-of-truth issues. Runs on
  // data changes (below) and after a rejected drag-drop bulk update, where the
  // optimistic column state must be rolled back.
  const rebuildItems = useCallback(() => {
    const { sortField, sortDirection } = kanbanFilters;
    const grouped: Record<string, string[]> = {};

    for (const status of statuses) {
      // Filter issues for this status
      let statusIssues = boardIssues.filter((i) => i.status_id === status.id);

      // Sort within column based on user preference
      statusIssues = [...statusIssues].sort((a, b) => {
        let comparison = 0;
        switch (sortField) {
          case 'priority':
            comparison =
              (a.priority ? PRIORITY_ORDER[a.priority] : Infinity) -
              (b.priority ? PRIORITY_ORDER[b.priority] : Infinity);
            break;
          case 'created_at':
            comparison =
              new Date(a.created_at).getTime() -
              new Date(b.created_at).getTime();
            break;
          case 'updated_at':
            comparison =
              new Date(a.updated_at).getTime() -
              new Date(b.updated_at).getTime();
            break;
          case 'title':
            comparison = a.title.localeCompare(b.title);
            break;
          case 'sort_order':
          default:
            comparison = a.sort_order - b.sort_order;
        }
        return sortDirection === 'desc' ? -comparison : comparison;
      });

      grouped[status.id] = statusIssues.map((i) => i.id);
    }
    setItems(grouped);
  }, [boardIssues, statuses, kanbanFilters]);

  // Sync items from filtered issues when they change
  useEffect(() => {
    // Skip rebuild during drag-drop sync to prevent flicker
    if (isSyncingRef.current) {
      return;
    }

    rebuildItems();
  }, [rebuildItems]);

  // Create a lookup map for issue data
  const issueMap = useMemo(() => {
    const map: Record<string, (typeof issues)[0]> = {};
    for (const issue of issues) {
      map[issue.id] = issue;
    }
    return map;
  }, [issues]);

  // Create a lookup map for issue assignees (issue_id -> OrganizationMemberWithProfile[])
  const issueAssigneesMap = useMemo(() => {
    const map: Record<string, OrganizationMemberWithProfile[]> = {};
    for (const assignee of issueAssignees) {
      const member = membersWithProfilesById.get(assignee.user_id);
      if (member) {
        if (!map[assignee.issue_id]) {
          map[assignee.issue_id] = [];
        }
        map[assignee.issue_id].push(member);
      }
    }
    return map;
  }, [issueAssignees, membersWithProfilesById]);

  const membersWithProfiles = useMemo(
    () => [...membersWithProfilesById.values()],
    [membersWithProfilesById]
  );

  const localWorkspacesById = useMemo(() => {
    const map = new Map<string, (typeof activeWorkspaces)[number]>();

    for (const workspace of activeWorkspaces) {
      map.set(workspace.id, workspace);
    }

    return map;
  }, [activeWorkspaces]);

  const prsByWorkspaceId = useMemo(() => {
    const map = new Map<string, WorkspacePr[]>();

    for (const pr of pullRequests) {
      if (!pr.workspace_id) continue;

      const prs = map.get(pr.workspace_id) ?? [];
      prs.push({
        number: pr.number,
        url: pr.url,
        status: pr.status as 'open' | 'merged' | 'closed',
      });
      map.set(pr.workspace_id, prs);
    }

    return map;
  }, [pullRequests]);

  const workspacesByIssueId = useMemo(() => {
    if (!showWorkspaces) {
      return new Map<string, WorkspaceWithStats[]>();
    }

    const map = new Map<string, WorkspaceWithStats[]>();

    for (const issue of issues) {
      const nonArchivedWorkspaces = getWorkspacesForIssue(issue.id)
        .filter(
          (workspace) =>
            !workspace.archived &&
            !!workspace.local_workspace_id &&
            localWorkspacesById.has(workspace.local_workspace_id)
        )
        .map((workspace) => {
          const localWorkspace = localWorkspacesById.get(
            workspace.local_workspace_id!
          );

          return {
            id: workspace.id,
            localWorkspaceId: workspace.local_workspace_id,
            name: workspace.name,
            archived: workspace.archived,
            filesChanged: workspace.files_changed ?? 0,
            linesAdded: workspace.lines_added ?? 0,
            linesRemoved: workspace.lines_removed ?? 0,
            prs: prsByWorkspaceId.get(workspace.id) ?? [],
            owner: membersWithProfilesById.get(workspace.owner_user_id) ?? null,
            updatedAt: workspace.updated_at,
            isOwnedByCurrentUser: workspace.owner_user_id === userId,
            isRunning: localWorkspace?.isRunning,
            hasPendingApproval: localWorkspace?.hasPendingApproval,
            hasRunningDevServer: localWorkspace?.hasRunningDevServer,
            hasUnseenActivity: localWorkspace?.hasUnseenActivity,
            latestProcessCompletedAt: localWorkspace?.latestProcessCompletedAt,
            latestProcessStatus: localWorkspace?.latestProcessStatus,
          };
        });

      if (nonArchivedWorkspaces.length > 0) {
        map.set(issue.id, nonArchivedWorkspaces);
      }
    }

    return map;
  }, [
    showWorkspaces,
    issues,
    getWorkspacesForIssue,
    localWorkspacesById,
    prsByWorkspaceId,
    membersWithProfilesById,
    userId,
  ]);

  const statusNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const status of statuses) {
      map.set(status.id, status.name);
    }
    return map;
  }, [statuses]);

  // While a card is being dragged, remember its source column and issue so
  // invalid workflow destinations can be disabled and grayed out (including
  // the Planning → Executing gate, which depends on the issue's metadata).
  const [dragSourceStatusId, setDragSourceStatusId] = useState<string | null>(
    null
  );
  const [draggedIssueId, setDraggedIssueId] = useState<string | null>(null);
  const handleDragStart = useCallback((start: DragStart) => {
    setDragSourceStatusId(start.source.droppableId);
    setDraggedIssueId(start.draggableId);
  }, []);

  // After an issue lands in Planning/Executing, offer to start an agent
  // workspace prefilled with the stage playbook (same flow as the issue-panel
  // "Run planning/execution agent" button).
  const maybePromptStageAgent = useCallback(
    async (issueId: string, destStatusName: string) => {
      if (!config) return;
      // With the background orchestrator on, it launches the stage agent
      // itself — prompting here too would start duplicate sessions.
      if (config.orchestrator?.mode === 'auto') return;

      const issue = issuesById.get(issueId);
      if (!issue || !resolveStage(destStatusName)) return;

      const tagNamesById = new Map(tags.map((tag) => [tag.id, tag.name]));
      const tagNames = getTagsForIssue(issueId)
        .map((issueTag) => tagNamesById.get(issueTag.tag_id))
        .filter((name): name is string => name != null);

      const playbook = resolveStagePlaybook({
        issue,
        tagNames,
        statusName: destStatusName,
        playbooks: config.playbooks,
        context: await playbooksApi.getContext(projectId).catch(() => ({})),
      });
      if (!playbook) return;

      const result = await ConfirmDialog.show({
        title:
          playbook.stage === 'planning'
            ? t('kanban.workflow.startPlanningAgentTitle')
            : t('kanban.workflow.startExecutionAgentTitle'),
        message: t('kanban.workflow.startAgentMessage', {
          simpleId: issue.simple_id,
          title: issue.title,
        }),
        confirmText: t('kanban.workflow.startAgentConfirm'),
        cancelText: t('kanban.workflow.startAgentCancel'),
        variant: 'info',
      });
      if (result !== 'confirmed') return;

      const defaults = await getWorkspaceDefaults(
        workspaces,
        localWorkspaceIds,
        projectId
      );
      const createState = buildWorkspaceCreateInitialState({
        prompt: playbook.prompt,
        defaults,
        linkedIssue: buildLinkedIssueCreateState(issue, projectId),
      });
      const draftId = await openWorkspaceCreateFromState(createState, {
        issueId: issue.id,
      });
      if (!draftId) {
        await ConfirmDialog.show({
          title: t('common:error'),
          message: t(
            'workspaces.createDraftError',
            'Failed to prepare workspace draft. Please try again.'
          ),
          confirmText: t('common:ok'),
          showCancelButton: false,
        });
      }
    },
    [
      config,
      issuesById,
      tags,
      getTagsForIssue,
      t,
      workspaces,
      localWorkspaceIds,
      projectId,
      openWorkspaceCreateFromState,
    ]
  );

  // Convert an investigation card (its class label's menu). The server swaps
  // the class, keeps everything the card has, and moves it on: to Executing
  // when the investigation's RCA and harness count as the plan, else to Plan.
  const handleConvertCard = useCallback(
    async (issueId: string, workClass: 'bug' | 'story') => {
      try {
        await cardsApi.convert(issueId, workClass);
      } catch (err) {
        await ConfirmDialog.show({
          title: t('common:error'),
          message:
            err instanceof Error && err.message
              ? err.message
              : t('kanban.workClassActions.convertFailed'),
          confirmText: t('common:ok'),
          showCancelButton: false,
        });
      }
    },
    [t]
  );

  // Analyze RCAs (project menu): opens the workspace-create flow prefilled
  // with the rca_analysis playbook — the agent connects the stored RCA
  // corpus into the knowledge graph via the kb/graph MCP tools.
  const handleAnalyzeRcas = useCallback(async () => {
    const playbook = config?.playbooks?.['rca_analysis'];
    if (!playbook) {
      await ConfirmDialog.show({
        title: t('common:error'),
        message: t(
          'kanban.analyzeRcasMissingPlaybook',
          "No 'rca_analysis' playbook configured (Settings > Playbooks)."
        ),
        confirmText: t('common:ok'),
        showCancelButton: false,
      });
      return;
    }
    const defaults = await getWorkspaceDefaults(
      workspaces,
      localWorkspaceIds,
      projectId
    );
    const context = await playbooksApi.getContext(projectId).catch(() => ({}));
    const createState = buildWorkspaceCreateInitialState({
      prompt: interpolatePlaybookTemplate(playbook, context),
      defaults,
    });
    await openWorkspaceCreateFromState(createState, {});
  }, [
    config,
    t,
    workspaces,
    localWorkspaceIds,
    projectId,
    openWorkspaceCreateFromState,
  ]);

  // Calculate sort_order based on column index and issue position
  // Formula: 1000 * [COLUMN_INDEX] + [ISSUE_INDEX] (both 1-based)
  const calculateSortOrder = useCallback(
    (statusId: string, issueIndex: number): number => {
      const columnIndex = statusColumnIndexMap.get(statusId) ?? 1;
      return 1000 * columnIndex + (issueIndex + 1);
    },
    [statusColumnIndexMap]
  );

  // Simple onDragEnd handler - the library handles all visual movement
  const handleDragEnd = useCallback(
    (result: DropResult) => {
      setDragSourceStatusId(null);
      setDraggedIssueId(null);

      const { source, destination, draggableId } = result;

      // Dropped outside a valid droppable
      if (!destination) return;

      // No movement
      if (
        source.droppableId === destination.droppableId &&
        source.index === destination.index
      ) {
        return;
      }

      const isManualSort = kanbanFilters.sortField === 'sort_order';

      // Block within-column reordering when not in manual sort mode
      // (cross-column moves are always allowed for status changes)
      if (source.droppableId === destination.droppableId && !isManualSort) {
        return;
      }

      const sourceId = source.droppableId;
      const destId = destination.droppableId;
      const isCrossColumn = sourceId !== destId;

      // Applies the move; `movedMetadata` is written with the moved card's
      // status (a confirmed discard travels that way).
      const applyMove = (movedMetadata?: JsonValue) => {
        // Compute the new layout synchronously from current state. Do NOT
        // capture it from inside a setItems(prev => …) updater: React only
        // runs updaters at render time (the setState calls above already
        // queued work, defeating eager evaluation), so the captured value
        // stayed empty and the bulk update silently sent zero changes —
        // cards moved visually while the server never heard about it.
        // Locate the card by id rather than by index: the Clean up column
        // lists its cards newest first (and filtered by its own search), so
        // its visual positions need not match the stored order.
        const sourceItems = [...(items[sourceId] ?? [])];
        const sourceIndex = sourceItems.indexOf(draggableId);
        if (sourceIndex === -1) return;
        const [moved] = sourceItems.splice(sourceIndex, 1);
        if (!moved) return;

        let newItems: Record<string, string[]>;
        if (!isCrossColumn) {
          // Within-column reorder
          sourceItems.splice(destination.index, 0, moved);
          newItems = { ...items, [sourceId]: sourceItems };
        } else {
          // Cross-column move
          const destItems = [...(items[destId] ?? [])];
          destItems.splice(destination.index, 0, moved);
          newItems = {
            ...items,
            [sourceId]: sourceItems,
            [destId]: destItems,
          };
        }
        setItems(newItems);

        // Build bulk updates for all issues in affected columns
        const updates: BulkUpdateIssueItem[] = [];

        // Always update destination column
        const destIssueIds = newItems[destId] ?? [];
        destIssueIds.forEach((issueId, index) => {
          updates.push({
            id: issueId,
            changes: {
              status_id: destId,
              sort_order: calculateSortOrder(destId, index),
              ...(issueId === draggableId && movedMetadata !== undefined
                ? { extension_metadata: movedMetadata }
                : {}),
            },
          });
        });

        // Update source column if cross-column move
        if (isCrossColumn) {
          const sourceIssueIds = newItems[sourceId] ?? [];
          sourceIssueIds.forEach((issueId, index) => {
            updates.push({
              id: issueId,
              changes: {
                sort_order: calculateSortOrder(sourceId, index),
              },
            });
          });
        }

        // Perform bulk update
        isSyncingRef.current = true;
        bulkUpdateIssues(updates)
          .then(() => {
            // Offer to start a stage agent when an issue moved into a
            // Planning/Executing column (never for same-column reorders).
            const destStatusName = statusNameById.get(destId);
            if (isCrossColumn && destStatusName) {
              void maybePromptStageAgent(draggableId, destStatusName);
            }
          })
          .catch((err) => {
            console.error('Failed to bulk update sort order:', err);
            // Server rejected the move (e.g. invalid workflow transition).
            // Roll the optimistic column state back and tell the user.
            isSyncingRef.current = false;
            rebuildItems();
            void ConfirmDialog.show({
              title: t('common:error'),
              message:
                err instanceof Error && err.message
                  ? err.message
                  : t('kanban.workflow.moveRejected'),
              confirmText: t('common:ok'),
              showCancelButton: false,
            });
          })
          .finally(() => {
            // Delay clearing flag to let Electric sync complete
            setTimeout(() => {
              isSyncingRef.current = false;
            }, 500);
          });
      };

      // Clean up reads the card's pull requests (the server holds the same
      // gate): an open one is refused with the reason, and a closed, unmerged
      // one asks before its branch and worktree are discarded.
      if (isCrossColumn && stageByStatusId.get(destId) === 'cleanUp') {
        const meta = parseIssueExtMetadata(
          issuesById.get(draggableId)?.extension_metadata
        );
        const gate = cardCleanupGate(
          meta,
          getPullRequestsForIssue(draggableId)
        );
        if (gate.kind === 'refused') {
          void ConfirmDialog.show({
            title: t('common:error'),
            message: t('kanban.cleanUpGate.prOpen'),
            confirmText: t('common:ok'),
            showCancelButton: false,
          });
          return;
        }
        if (gate.kind === 'confirm') {
          void confirmDiscard(t).then((confirmed) => {
            if (confirmed) {
              applyMove(
                withDiscardConfirmed(
                  issuesById.get(draggableId)?.extension_metadata
                )
              );
            }
          });
          return;
        }
      }
      applyMove();
    },
    [
      kanbanFilters.sortField,
      calculateSortOrder,
      statusNameById,
      maybePromptStageAgent,
      rebuildItems,
      items,
      stageByStatusId,
      issuesById,
      getPullRequestsForIssue,
      t,
    ]
  );

  // Multi-select support
  const {
    selectedIssueIds,
    isMultiSelectActive,
    handleIssueClick,
    handleCheckboxChange,
    clearSelection,
  } = useIssueMultiSelect();
  const setOrderedIssueIds = useIssueSelectionStore(
    (s) => s.setOrderedIssueIds
  );
  const setAnchor = useIssueSelectionStore((s) => s.setAnchor);

  // Compute ordered issue IDs for range selection
  // A folded Clean up column shows no cards, so keyboard range selection
  // skips them.
  const orderedIssueIds = useMemo(() => {
    const statusOrder =
      kanbanViewMode === 'kanban'
        ? visibleStatuses.filter(
            (status) =>
              isCleanUpExpanded || stageByStatusId.get(status.id) !== 'cleanUp'
          )
        : listViewStatuses;
    return statusOrder.flatMap((status) => items[status.id] ?? []);
  }, [
    kanbanViewMode,
    visibleStatuses,
    listViewStatuses,
    items,
    isCleanUpExpanded,
    stageByStatusId,
  ]);

  // Keep the store's ordered IDs in sync
  useEffect(() => {
    setOrderedIssueIds(orderedIssueIds);
  }, [orderedIssueIds, setOrderedIssueIds]);

  // Clear multi-selection when project or view mode changes
  useEffect(() => {
    clearSelection();
  }, [projectId, kanbanViewMode, clearSelection]);

  // Keep anchor in sync with the currently opened issue (e.g. from URL on
  // page load) so Shift/Cmd+Click on another issue includes it.
  useEffect(() => {
    if (selectedKanbanIssueId) {
      setAnchor(selectedKanbanIssueId);
    }
  }, [selectedKanbanIssueId, setAnchor]);

  const handleCardClick = useCallback(
    (issueId: string, e?: MouseEvent) => {
      if (e && (e.metaKey || e.ctrlKey || e.shiftKey)) {
        handleIssueClick(issueId, e);
      } else {
        if (selectedIssueIds.size > 0) {
          clearSelection();
        }
        // Set as anchor so Shift+Click from this issue works
        setAnchor(issueId);
        openIssue(issueId);
      }
    },
    [
      openIssue,
      handleIssueClick,
      selectedIssueIds.size,
      clearSelection,
      setAnchor,
    ]
  );

  const handleAddTask = useCallback(() => {
    const createPayload = {
      statusId: defaultCreateStatusId,
      ...(createAssigneeIds.length > 0
        ? { assigneeIds: createAssigneeIds }
        : {}),
    };
    startCreate(createPayload);
  }, [createAssigneeIds, defaultCreateStatusId, startCreate]);

  // Inline editing callbacks for kanban cards
  // When multi-select is active, apply to all selected issues
  const handleCardPriorityClick = useCallback(
    (issueId: string) => {
      const ids = isMultiSelectActive ? [...selectedIssueIds] : [issueId];
      openPrioritySelection(projectId, ids);
    },
    [projectId, openPrioritySelection, selectedIssueIds, isMultiSelectActive]
  );

  const handleCardAssigneeClick = useCallback(
    (issueId: string) => {
      const ids = isMultiSelectActive ? [...selectedIssueIds] : [issueId];
      openAssigneeSelection(projectId, ids);
    },
    [projectId, openAssigneeSelection, selectedIssueIds, isMultiSelectActive]
  );

  const handleCardMoreActionsClick = useCallback(
    (issueId: string) => {
      const ids = isMultiSelectActive ? [...selectedIssueIds] : [issueId];
      CommandBarDialog.show({
        page: 'issueActions',
        projectId,
        issueIds: ids,
      });
    },
    [projectId, selectedIssueIds, isMultiSelectActive]
  );

  const handleCardTagToggle = useCallback(
    (issueId: string, tagId: string) => {
      const currentIssueTags = getTagsForIssue(issueId);
      const existing = currentIssueTags.find((it) => it.tag_id === tagId);
      if (existing) {
        removeIssueTag(existing.id);
      } else {
        insertIssueTag({ issue_id: issueId, tag_id: tagId });
      }
    },
    [getTagsForIssue, insertIssueTag, removeIssueTag]
  );

  const getResolvedRelationshipsForIssue = useCallback(
    (issueId: string) =>
      resolveRelationshipsForIssue(
        issueId,
        getRelationshipsForIssue(issueId),
        issuesById
      ),
    [getRelationshipsForIssue, issuesById]
  );

  const handleCreateTag = useCallback(
    (data: { name: string; color: string }): string => {
      const { data: newTag } = insertTag({
        project_id: projectId,
        name: data.name,
        color: data.color,
      });
      return newTag.id;
    },
    [insertTag, projectId]
  );

  // --- Needs-you actions on the card face ---------------------------------

  // Approve and Clean up in flight, so their buttons cannot double-fire.
  const [pendingCardActions, setPendingCardActions] = useState<Set<string>>(
    () => new Set()
  );
  const setCardActionPending = useCallback(
    (issueId: string, pending: boolean) => {
      setPendingCardActions((previous) => {
        const next = new Set(previous);
        if (pending) next.add(issueId);
        else next.delete(issueId);
        return next;
      });
    },
    []
  );

  const showActionError = useCallback(
    (err: unknown, fallback: string) =>
      ConfirmDialog.show({
        title: t('common:error'),
        message: err instanceof Error && err.message ? err.message : fallback,
        confirmText: t('common:ok'),
        showCancelButton: false,
      }),
    [t]
  );

  // Approve records the plan decision through the approvals endpoint, the
  // same record a tracker or card decision writes: the spec approval is
  // set and the card moves to Execute once the plan passes the gate.
  const handleApprovePlan = useCallback(
    async (issueId: string) => {
      setCardActionPending(issueId, true);
      try {
        await postPlanDecision(issueId, 'approve');
      } catch (err) {
        await showActionError(err, t('kanban.needsYou.decisionFailed'));
      } finally {
        setCardActionPending(issueId, false);
      }
    },
    [setCardActionPending, showActionError, t]
  );

  // Review opens the card's current session (where the plan was written);
  // a card with no session on this machine opens its panel instead.
  const handleReviewPlan = useCallback(
    (issueId: string) => {
      const session = implementationSession(
        getWorkspacesForIssue(issueId).map((workspace) => ({
          name: workspace.name,
          localWorkspaceId: workspace.local_workspace_id,
          updatedAt: workspace.updated_at,
          createdAt: workspace.created_at,
        })),
        (localWorkspaceId) => localWorkspaceIds.has(localWorkspaceId)
      );
      if (session?.localWorkspaceId) {
        openIssueWorkspace(issueId, session.localWorkspaceId);
        return;
      }
      openIssue(issueId);
    },
    [getWorkspacesForIssue, localWorkspaceIds, openIssueWorkspace, openIssue]
  );

  // Clean up (a merged or closed card in Shipped) and Close (a triaged
  // investigation) move the card to Clean up, the same status change the
  // Move to… menu makes; the server runs the clean-up from there. Clean up's
  // gate is asked first: a closed, unmerged pull request is discarded only
  // once confirmed, and an open one is refused with the reason.
  const cleanUpStatusId = useMemo(
    () =>
      sortedStatuses.find(
        (status) => workflowStageKey(status.name) === 'cleanUp'
      )?.id ?? null,
    [sortedStatuses]
  );
  const handleCleanUp = useCallback(
    async (issueId: string) => {
      if (!cleanUpStatusId) return;
      const issue = issuesById.get(issueId);
      const gate = cardCleanupGate(
        parseIssueExtMetadata(issue?.extension_metadata),
        getPullRequestsForIssue(issueId)
      );
      if (gate.kind === 'refused') {
        await showActionError(null, t('kanban.cleanUpGate.prOpen'));
        return;
      }
      if (gate.kind === 'confirm' && !(await confirmDiscard(t))) return;
      setCardActionPending(issueId, true);
      try {
        await updateIssue(issueId, {
          status_id: cleanUpStatusId,
          ...(gate.kind === 'confirm'
            ? {
                extension_metadata: withDiscardConfirmed(
                  issue?.extension_metadata
                ),
              }
            : {}),
        }).persisted;
      } catch (err) {
        await showActionError(err, t('kanban.workflow.moveRejected'));
      } finally {
        setCardActionPending(issueId, false);
      }
    },
    [
      cleanUpStatusId,
      issuesById,
      getPullRequestsForIssue,
      setCardActionPending,
      showActionError,
      t,
      updateIssue,
    ]
  );

  // --- Header facts ----------------------------------------------------------

  // The board's repository (the first one the orchestrator checks out for
  // it) names the repo and its default branch in the header.
  const boardRepoId =
    config?.orchestrator?.project_defaults?.[projectId]?.repo_ids[0] ?? null;
  const { data: repos } = useQuery({
    queryKey: ['repos'],
    queryFn: () => repoApi.list(),
    enabled: boardRepoId !== null,
    staleTime: 60_000,
  });
  const boardRepo = boardRepoId
    ? (repos?.find((repo) => repo.id === boardRepoId) ?? null)
    : null;
  const ticketing = config?.ticketing;
  const importsFromTracker =
    !!ticketing?.enabled && ticketing.project_id === projectId;
  const activeCardCount = useMemo(
    () =>
      filteredIssues.filter((issue) => {
        const stage = stageByStatusId.get(issue.status_id);
        return stage !== 'cleanUp';
      }).length,
    [filteredIssues, stageByStatusId]
  );
  const headerFacts = useMemo(() => {
    const facts: string[] = [];
    if (boardRepo?.default_target_branch) {
      facts.push(boardRepo.default_target_branch);
    }
    facts.push(t('kanban.cardCount', { count: activeCardCount }));
    if (importsFromTracker && ticketing) {
      facts.push(
        t('kanban.importsFromTracker', {
          tracker: t(`kanban.trackerNames.${ticketing.kind}`),
        })
      );
    }
    return facts;
  }, [boardRepo, activeCardCount, importsFromTracker, ticketing, t]);

  // --- Card face ---------------------------------------------------------------

  const buildClassLabel = useCallback(
    (issueId: string, workClass: ReturnType<typeof cardWorkClass>) => {
      if (!workClass) return null;
      const label: KanbanClassLabel = {
        name: t(`kanban.trackerImport.workClass.${workClass}`),
        tone: workClassChipTone(workClass),
        actions:
          workClass === 'investigation'
            ? [
                {
                  label: t('kanban.workClassActions.convertToBug'),
                  onSelect: () => void handleConvertCard(issueId, 'bug'),
                },
                {
                  label: t('kanban.workClassActions.convertToStory'),
                  onSelect: () => void handleConvertCard(issueId, 'story'),
                },
              ]
            : undefined,
      };
      return label;
    },
    [handleConvertCard, t]
  );

  const isLoading = projectLoading || orgLoading;

  if (isLoading) {
    return <LoadingState />;
  }

  const needsYouCount = needsYouByIssueId.size;
  const projectMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="flex size-7 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high"
          aria-label={t('kanban.projectMenu')}
        >
          <DotsThreeIcon className="size-4" weight="bold" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem onClick={openProjectsGuide}>
          {t('kanban.openProjectsGuide', 'Projects guide')}
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => executeAction(Actions.ProjectSettings)}
        >
          {t('kanban.editProjectSettings', 'Edit project settings')}
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => void ImportFromTrackerDialog.show({ projectId })}
        >
          {t('kanban.trackerImport.menuAction', 'Import from tracker')}
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => void handleAnalyzeRcas()}>
          {t('kanban.analyzeRcas', 'Analyze RCAs')}
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => appNavigation.goToProjectKnowledge(projectId)}
        >
          {t('kanban.knowledgeMenu', 'Knowledge')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  const renderIssueCard = (
    issue: (typeof issues)[number],
    index: number,
    stage: WorkflowStageKey | null
  ) => {
    const meta = parseIssueExtMetadata(issue.extension_metadata);
    const cardTags = getTagObjectsForIssue(issue.id);
    const workClass = cardWorkClass(
      meta,
      cardTags.map((tag) => tag.name)
    );
    const cardPullRequests = getPullRequestsForIssue(issue.id);
    const needsYou = needsYouByIssueId.get(issue.id) ?? null;
    const sessions = workspacesByIssueId.get(issue.id) ?? [];
    const runningSession = sessions.find((session) => session.isRunning);
    const currentSession =
      implementationSession(sessions, () => true) ?? sessions[0] ?? null;
    const statusSession = runningSession ?? currentSession;
    const takenOver =
      isTakenOver(meta) ||
      cardTags.some((tag) => tag.name.trim().toLowerCase() === 'taken-over');
    const agentFailed = cardTags.some(
      (tag) => tag.name.trim().toLowerCase() === 'agent-failed'
    );

    const { epicKey: cardEpicKey, jiraKey: cardJiraKey } = issueEpicKeys(issue);
    const cardEpicSummary = cardJiraKey
      ? epicSummaries.get(cardJiraKey)
      : undefined;
    const mrUrl = typeof meta?.mr_url === 'string' ? meta.mr_url : null;
    // MR publication state in the Shipped column: claimed-without-url =
    // pushing, mr_url = pushed, agent-failed tag without url = failed.
    const pushState =
      stage === 'shipped'
        ? mrUrl
          ? ('pushed' as const)
          : agentFailed
            ? ('failed' as const)
            : meta?.playbook_started_at != null
              ? ('pushing' as const)
              : null
        : null;
    // Instant feedback after a forward move into an automated column: the
    // forward move cleared the claim marker, and the orchestrator (Auto
    // mode, assignee == me, workflow ticket) claims within one poll tick,
    // so unclaimed here means "agent starting shortly". Only for boards
    // this machine's orchestrator actually scans: without that check the
    // card promised an agent for a project never configured here.
    const agentQueued =
      config?.orchestrator?.mode === 'auto' &&
      (config?.orchestrator?.project_defaults?.[projectId]?.repo_ids.length ??
        0) > 0 &&
      pushState == null &&
      (stage === 'plan' ||
        stage === 'execute' ||
        stage === 'review' ||
        stage === 'shipped' ||
        stage === 'cleanUp') &&
      meta?.playbook_started_at == null &&
      (meta?.jira != null || meta?.work_class != null) &&
      (issueAssigneesMap[issue.id] ?? []).some(
        (member) => member.user_id === userId
      );
    const filedFrom = cardOrigin(meta);
    const origin = filedFrom
      ? {
          text: t('kanban.origin.filedBy', {
            filer: t(`kanban.origin.kinds.${filedFrom.kind}`),
          }),
          linkText: filedFrom.url ? t('kanban.origin.thread') : filedFrom.ref,
          url: filedFrom.url,
        }
      : null;

    const statusLine = buildStatusLine(
      t,
      {
        stage,
        workClass,
        meta,
        createdAt: issue.created_at,
        needsYou,
        agentFailed,
        pushState,
        agentQueued,
        pullRequests: cardPullRequests,
        mrUrl,
        session: statusSession,
        origin,
      },
      now
    );

    const isActionPending = pendingCardActions.has(issue.id);
    let needsYouProps: KanbanCardNeedsYou | null = null;
    if (needsYou?.kind === 'plan') {
      needsYouProps = {
        label: needsYouLabel(t, needsYou),
        detail: planGateDetail(t, needsYou.gate) || null,
        actions: [
          {
            label: t('kanban.needsYou.approve'),
            variant: 'primary',
            disabled: isActionPending,
            onClick: () => void handleApprovePlan(issue.id),
          },
          {
            label: t('workspaces.review'),
            variant: 'secondary',
            onClick: () => handleReviewPlan(issue.id),
          },
        ],
      };
    } else if (needsYou?.kind === 'triaged') {
      needsYouProps = {
        label: needsYouLabel(t, needsYou),
        actions: [
          {
            label: t('kanban.workClassActions.convertToBug'),
            variant: 'primary',
            disabled: isActionPending,
            onClick: () => void handleConvertCard(issue.id, 'bug'),
          },
          ...(cleanUpStatusId
            ? [
                {
                  label: t('kanban.triaged.close'),
                  variant: 'secondary' as const,
                  disabled: isActionPending,
                  onClick: () => void handleCleanUp(issue.id),
                },
              ]
            : []),
        ],
      };
    } else if (
      needsYou?.kind === 'merged' ||
      needsYou?.kind === 'closedUnmerged'
    ) {
      needsYouProps = {
        label: needsYouLabel(t, needsYou),
        actions: cleanUpStatusId
          ? [
              {
                label: t('kanban.cleanUpAction'),
                variant: 'secondary',
                disabled: isActionPending,
                onClick: () => void handleCleanUp(issue.id),
              },
            ]
          : [],
      };
    }

    // Shipped (an older board's Push MR, Done and Close too) wears its pull
    // request's state as a chip.
    const indicatorKind =
      stage === 'shipped' || stage === 'close'
        ? shippedIndicator(meta, cardPullRequests)
        : null;
    const indicator = indicatorKind
      ? {
          label: shippedIndicatorLabel(t, indicatorKind),
          tone: DONE_INDICATOR_TONES[indicatorKind.kind],
          title:
            indicatorKind.kind === 'ciFailing'
              ? t('kanban.shippedIndicator.ciFailingHint')
              : undefined,
        }
      : null;

    const diffSession = currentSession ?? runningSession ?? null;

    return (
      <KanbanCard
        key={issue.id}
        id={issue.id}
        name={issue.title}
        index={index}
        className="group"
        onClick={(e) => handleCardClick(issue.id, e)}
        isOpen={selectedKanbanIssueId === issue.id}
        isMobile={isMobile}
        isSelected={selectedIssueIds.has(issue.id)}
        dragDisabled={isMultiSelectActive}
        tone={needsYouProps ? 'attention' : 'default'}
      >
        <KanbanCardContent
          displayId={meta?.jira?.key ?? issue.simple_id}
          title={issue.title}
          priorityLabel={priorityChipLabel(issue.priority)}
          classLabel={buildClassLabel(issue.id, workClass)}
          takenOverLabel={takenOver ? t('kanban.card.takenOver') : null}
          indicator={indicator}
          statusLine={statusLine}
          needsYou={needsYouProps}
          diffStats={
            diffSession && diffSession.filesChanged > 0
              ? {
                  filesChanged: diffSession.filesChanged,
                  linesAdded: diffSession.linesAdded,
                  linesRemoved: diffSession.linesRemoved,
                }
              : null
          }
          epicKey={cardEpicKey}
          epicColor={
            cardEpicKey
              ? epicColorFromKey(cardEpicKey)
              : cardEpicSummary && cardJiraKey
                ? epicColorFromKey(cardJiraKey)
                : undefined
          }
          epicProgress={cardEpicSummary}
          // The class chip and the status line already say what the class
          // and the orchestrator's own tags would.
          tags={cardTags.filter((tag) => {
            const name = tag.name.trim().toLowerCase();
            return name !== workClass && !SYSTEM_TAG_NAMES.has(name);
          })}
          assignees={issueAssigneesMap[issue.id] ?? []}
          relationships={resolveRelationshipsForIssue(
            issue.id,
            getRelationshipsForIssue(issue.id),
            issuesById
          )}
          isSubIssue={!!issue.parent_issue_id}
          isMobile={isMobile}
          onPriorityClick={(e) => {
            e.stopPropagation();
            handleCardPriorityClick(issue.id);
          }}
          onAssigneeClick={(e) => {
            e.stopPropagation();
            handleCardAssigneeClick(issue.id);
          }}
          onMoreActionsClick={() => handleCardMoreActionsClick(issue.id)}
          tagEditProps={{
            allTags: tags,
            selectedTagIds: getTagsForIssue(issue.id).map((it) => it.tag_id),
            onTagToggle: (tagId) => handleCardTagToggle(issue.id, tagId),
            onCreateTag: handleCreateTag,
            renderTagEditor: ({
              allTags,
              selectedTagIds,
              onTagToggle,
              onCreateTag,
              trigger,
            }) => (
              <SearchableTagDropdownContainer
                tags={allTags}
                selectedTagIds={selectedTagIds}
                onTagToggle={onTagToggle}
                onCreateTag={onCreateTag}
                disabled={false}
                contentClassName=""
                trigger={trigger}
              />
            ),
          }}
        />
      </KanbanCard>
    );
  };

  const renderArchivedCard = (
    issue: (typeof issues)[number],
    index: number
  ) => {
    const meta = parseIssueExtMetadata(issue.extension_metadata);
    const workClass = cardWorkClass(
      meta,
      getTagObjectsForIssue(issue.id).map((tag) => tag.name)
    );
    const links: { label: string; url: string }[] = [];
    const pr = latestPullRequest(getPullRequestsForIssue(issue.id));
    if (pr) {
      links.push({ label: pullRequestLabel(t, pr.number), url: pr.url });
    }
    if (meta?.spec?.approved_at) {
      links.push({
        label:
          meta.planning?.kind === 'rca' || workClass === 'bug'
            ? t('kanban.cleanUp.rcaLink')
            : t('kanban.planChip'),
        url: `/api/remote/issues/${issue.id}/spec`,
      });
    }
    const classLabel = buildClassLabel(issue.id, workClass);
    return (
      <KanbanCard
        key={issue.id}
        id={issue.id}
        name={issue.title}
        index={index}
        className="group"
        onClick={(e) => handleCardClick(issue.id, e)}
        isOpen={selectedKanbanIssueId === issue.id}
        isMobile={isMobile}
        isSelected={selectedIssueIds.has(issue.id)}
        dragDisabled={isMultiSelectActive}
        tone="archived"
      >
        <KanbanArchivedCardContent
          displayId={meta?.jira?.key ?? issue.simple_id}
          title={issue.title}
          classLabel={classLabel ? { ...classLabel, actions: undefined } : null}
          when={formatRelativeTime(t, issue.updated_at, now)}
          links={links}
        />
      </KanbanCard>
    );
  };

  const renderCleanUpColumn = (
    status: (typeof visibleStatuses)[number],
    issueIds: string[],
    isDropAllowed: boolean
  ) => {
    const query = cleanUpQuery.trim().toLowerCase();
    const cleanedUp = issueIds
      .map((issueId) => issueMap[issueId])
      .filter((issue): issue is (typeof issues)[number] => !!issue)
      .filter((issue) => {
        if (!query) return true;
        const meta = parseIssueExtMetadata(issue.extension_metadata);
        return [issue.title, issue.simple_id, meta?.jira?.key ?? '']
          .join(' ')
          .toLowerCase()
          .includes(query);
      })
      .sort(
        (a, b) =>
          new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
      );
    const shown =
      showAllCleanedUp || query
        ? cleanedUp
        : cleanedUp.slice(0, CLEAN_UP_PAGE_SIZE);
    const hiddenCount = cleanedUp.length - shown.length;
    const thisWeek = shown.filter((issue) =>
      isWithinLastWeek(issue.updated_at, now)
    );
    const earlier = shown.filter(
      (issue) => !isWithinLastWeek(issue.updated_at, now)
    );

    return (
      <KanbanBoard
        key={status.id}
        className={cn(
          'border-l border-dashed border-low/40 pl-3 transition-opacity',
          !isDropAllowed && 'opacity-40'
        )}
      >
        <KanbanColumnHeader
          name={status.name}
          count={issueIds.length}
          dot={columnDotTone('cleanUp')}
          helper={columnHelper(t, 'cleanUp')}
          trailing={
            <KanbanColumnFoldButton
              label={t('kanban.cleanUp.fold', { name: status.name })}
              onFold={() => setCleanUpExpanded(false)}
            />
          }
        />
        <KanbanColumnSearch
          value={cleanUpQuery}
          onChange={setCleanUpQuery}
          placeholder={t('kanban.cleanUp.searchPlaceholder')}
        />
        <KanbanCards id={status.id} isDropDisabled={!isDropAllowed}>
          {thisWeek.length > 0 && (
            <KanbanColumnGroupLabel>
              {t('kanban.cleanUp.thisWeek')}
            </KanbanColumnGroupLabel>
          )}
          {thisWeek.map((issue, index) => renderArchivedCard(issue, index))}
          {earlier.length > 0 && (
            <KanbanColumnGroupLabel>
              {t('kanban.cleanUp.earlier')}
            </KanbanColumnGroupLabel>
          )}
          {earlier.map((issue, index) =>
            renderArchivedCard(issue, thisWeek.length + index)
          )}
          {cleanedUp.length === 0 && (
            <p className="m-0 px-0.5 text-xs text-low">
              {query
                ? t('kanban.cleanUp.noMatches')
                : t('kanban.cleanUp.empty')}
            </p>
          )}
        </KanbanCards>
        {hiddenCount > 0 && (
          <button
            type="button"
            onClick={() => setShowAllCleanedUp(true)}
            className="self-start px-0.5 py-1 text-xs font-semibold text-brand-secondary hover:underline"
          >
            {t('kanban.cleanUp.showMore', { n: hiddenCount })}
          </button>
        )}
      </KanbanBoard>
    );
  };

  return (
    <div className="flex h-full flex-col gap-5 bg-primary">
      <div
        className={cn(
          'flex flex-col gap-3 px-7 pt-[22px]',
          isMobile && 'px-4 pt-4'
        )}
      >
        <header
          className={cn(
            'flex items-center gap-3',
            isMobile ? 'flex-col items-stretch' : 'flex-wrap'
          )}
        >
          <KanbanBoardTitle
            title={projectName}
            isMobile={isMobile}
            menu={projectMenu}
            repo={boardRepo ? boardRepo.display_name || boardRepo.name : null}
            facts={headerFacts}
            needsYou={
              needsYouCount > 0 || needsYouOnly
                ? {
                    label: t('kanban.needsYou.count', {
                      count: needsYouCount,
                    }),
                    active: needsYouOnly,
                    onToggle: () => setNeedsYouOnly((value) => !value),
                    toggleLabel: needsYouOnly
                      ? t('kanban.needsYou.showAll')
                      : t('kanban.needsYou.showOnly'),
                  }
                : null
            }
          />
          <KanbanFilterBar
            isFiltersDialogOpen={isFiltersDialogOpen}
            onFiltersDialogOpenChange={setIsFiltersDialogOpen}
            tags={tags}
            users={membersWithProfiles}
            activeViewId={activeViewId}
            onViewChange={handleKanbanProjectViewChange}
            viewIds={KANBAN_PROJECT_VIEW_IDS}
            projectId={projectId}
            currentUserId={userId}
            filters={kanbanFilters}
            showSubIssues={showSubIssues}
            showWorkspaces={showWorkspaces}
            hasActiveFilters={hasActiveFilters}
            onSearchQueryChange={setKanbanSearchQuery}
            onPrioritiesChange={setKanbanPriorities}
            onAssigneesChange={setKanbanAssignees}
            onTagsChange={setKanbanTags}
            onSortChange={setKanbanSort}
            onShowSubIssuesChange={setShowSubIssues}
            onShowWorkspacesChange={setShowWorkspaces}
            hideBlocked={hideBlocked}
            onHideBlockedChange={setHideBlocked}
            onClearFilters={clearKanbanFilters}
            onCreateIssue={handleAddTask}
            shouldAnimateCreateButton={shouldAnimateCreateButton}
            renderFiltersDialog={(props) => <KanbanFiltersDialog {...props} />}
            isMobile={isMobile}
            viewSwitcher={
              <ViewNavTabs
                activeView={kanbanViewMode}
                onViewChange={setKanbanViewMode}
                hiddenStatuses={hiddenStatuses}
                selectedStatusId={listViewStatusFilter}
                onStatusSelect={setListViewStatusFilter}
              />
            }
          />
        </header>

        {/* Epic chips: filter the board to one epic + its subtasks. */}
        {epicSummaries.size > 0 && (
          <div className="flex shrink-0 items-center gap-1.5 overflow-x-auto">
            <button
              type="button"
              onClick={() => setEpicFilter(null)}
              aria-pressed={epicFilter === null}
              className={cn(
                'inline-flex h-[22px] shrink-0 items-center rounded-full border px-2.5 text-xs font-medium transition-colors',
                epicFilter === null
                  ? 'border-transparent bg-brand-soft text-brand-soft-foreground'
                  : 'border-border bg-secondary text-normal hover:text-high'
              )}
            >
              {t('kanban.epicTabs.all')}
            </button>
            {[...epicSummaries.entries()].map(([key, summary]) => (
              <button
                key={key}
                type="button"
                onClick={() =>
                  setEpicFilter((current) => (current === key ? null : key))
                }
                aria-pressed={epicFilter === key}
                title={summary.title}
                className={cn(
                  'inline-flex h-[22px] shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition-colors',
                  epicFilter === key
                    ? 'border-transparent bg-brand-soft text-brand-soft-foreground'
                    : 'border-border bg-secondary text-normal hover:text-high'
                )}
              >
                <span
                  className="size-2 shrink-0 rounded-full"
                  style={{ backgroundColor: epicColorFromKey(key) }}
                />
                <span className="font-mono">{key}</span>
                <span className="text-low">
                  {summary.done}/{summary.total}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>

      {kanbanViewMode === 'kanban' ? (
        visibleStatuses.length === 0 ? (
          <div className="flex flex-1 items-center justify-center">
            <p className="text-low">{t('kanban.noVisibleStatuses')}</p>
          </div>
        ) : (
          <div
            className={cn(
              'min-h-0 flex-1 overflow-auto px-7 pb-7',
              isMobile && 'px-4 pb-4'
            )}
          >
            <KanbanProvider
              onDragEnd={handleDragEnd}
              onDragStart={handleDragStart}
            >
              {visibleStatuses.map((status) => {
                const issueIds = items[status.id] ?? [];
                const stage = stageByStatusId.get(status.id) ?? null;
                // During a drag, gray out and disable columns the workflow
                // does not allow the dragged issue to move into.
                const draggedIssue = draggedIssueId
                  ? issuesById.get(draggedIssueId)
                  : undefined;
                const isDropAllowed =
                  dragSourceStatusId === null ||
                  dragSourceStatusId === status.id ||
                  (isTransitionAllowed(
                    statusNameById.get(dragSourceStatusId),
                    status.name
                  ) &&
                    planningGateError(
                      statusNameById.get(dragSourceStatusId),
                      status.name,
                      draggedIssue?.extension_metadata
                    ) === null);

                if (stage === 'cleanUp') {
                  if (!isCleanUpExpanded) {
                    return (
                      <KanbanFoldedColumn
                        key={status.id}
                        name={status.name}
                        count={issueIds.length}
                        label={t('kanban.cleanUp.unfold', {
                          name: status.name,
                          count: issueIds.length,
                        })}
                        onUnfold={() => setCleanUpExpanded(true)}
                      />
                    );
                  }
                  return renderCleanUpColumn(status, issueIds, isDropAllowed);
                }

                return (
                  <KanbanBoard
                    key={status.id}
                    className={cn(
                      'transition-opacity',
                      !isDropAllowed && 'opacity-40'
                    )}
                  >
                    <KanbanColumnHeader
                      name={status.name}
                      count={issueIds.length}
                      dot={
                        stage ? columnDotTone(stage) : { color: status.color }
                      }
                      helper={stage ? columnHelper(t, stage) : null}
                    />
                    <KanbanCards id={status.id} isDropDisabled={!isDropAllowed}>
                      {issueIds.map((issueId, index) => {
                        const issue = issueMap[issueId];
                        if (!issue) return null;
                        return renderIssueCard(issue, index, stage);
                      })}
                    </KanbanCards>
                  </KanbanBoard>
                );
              })}
            </KanbanProvider>
          </div>
        )
      ) : (
        <div className={cn('flex-1 overflow-y-auto px-7', isMobile && 'px-4')}>
          <KanbanProvider onDragEnd={handleDragEnd} className="!block !w-full">
            <IssueListView
              statuses={listViewStatuses}
              items={items}
              issueMap={issueMap}
              issueAssigneesMap={issueAssigneesMap}
              getTagObjectsForIssue={getTagObjectsForIssue}
              getResolvedRelationshipsForIssue={
                getResolvedRelationshipsForIssue
              }
              onIssueClick={handleCardClick}
              selectedIssueId={selectedKanbanIssueId}
              selectedIssueIds={selectedIssueIds}
              isMultiSelectActive={isMultiSelectActive}
              onIssueCheckboxChange={handleCheckboxChange}
            />
          </KanbanProvider>
        </div>
      )}

      {isMultiSelectActive && <BulkActionBarContainer projectId={projectId} />}
    </div>
  );
}
