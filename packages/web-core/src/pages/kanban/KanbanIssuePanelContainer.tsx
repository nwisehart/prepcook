import {
  useState,
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useMemo,
} from 'react';
import { useDropzone } from 'react-dropzone';
import { useTranslation } from 'react-i18next';
import type { OrganizationMemberWithProfile } from 'shared/types';
import type { IssuePriority } from 'shared/remote-types';
import { useDebouncedCallback } from '@/shared/hooks/useDebouncedCallback';
import { useProjectContext } from '@/shared/hooks/useProjectContext';
import { useOrgContext } from '@/shared/hooks/useOrgContext';
import { useProjectWorkspaceCreateDraft } from '@/shared/hooks/useProjectWorkspaceCreateDraft';
import WYSIWYGEditor from '@/shared/components/WYSIWYGEditor';
import { SearchableTagDropdownContainer } from '@/shared/components/SearchableTagDropdownContainer';
import { IssueCommentsSectionContainer } from './IssueCommentsSectionContainer';
import { IssueSubIssuesSectionContainer } from './IssueSubIssuesSectionContainer';
import { IssueRelationshipsSectionContainer } from './IssueRelationshipsSectionContainer';
import { IssueWorkspacesSectionContainer } from './IssueWorkspacesSectionContainer';
import {
  KanbanIssuePanel,
  type IssueFormData,
} from '@prepcook/ui/components/KanbanIssuePanel';
import { useActions } from '@/shared/hooks/useActions';
import { useUserContext } from '@/shared/hooks/useUserContext';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { CommandBarDialog } from '@/shared/dialogs/command-bar/CommandBarDialog';
import { getWorkspaceDefaults } from '@/shared/lib/workspaceDefaults';
import {
  buildLinkedIssueCreateState,
  buildWorkspaceCreateInitialState,
  buildWorkspaceCreatePrompt,
} from '@/shared/lib/workspaceCreateState';
import {
  parseIssueExtMetadata,
  resolveStagePlaybook,
} from '@/shared/lib/stagePlaybooks';
import { cardOrigin, cardWorkClass } from '@/shared/lib/workClass';
import {
  cardCleanupGate,
  columnDotTone,
  latestMergedPullRequest,
  latestPullRequest,
  needsYouReason,
  priorityChipLabel,
  withDiscardConfirmed,
  workClassChipTone,
  workflowStageKey,
} from '@/features/kanban/model/cardSignals';
import { confirmDiscard } from '@/features/kanban/ui/confirmDiscard';
import {
  needsYouLabel,
  planGateDetail,
  pullRequestLabel,
} from '@/features/kanban/model/cardCopy';
import { postPlanDecision } from '@/features/kanban/model/planDecisionsApi';
import { formatRelativeTime } from '@prepcook/ui/components/KanbanRelativeTime';
import type { IssueExtraProperty } from '@prepcook/ui/components/IssuePropertyRow';
import type { KanbanIssuePanelDecision } from '@prepcook/ui/components/KanbanIssuePanel';
import {
  createBlankCreateFormData,
  createInitialKanbanIssuePanelFormState,
  kanbanIssuePanelFormReducer,
  selectDisplayData,
  selectIsCreateDraftDirty,
} from './kanban-issue-panel-state';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import { useAzureAttachments } from '@/shared/hooks/useAzureAttachments';
import {
  commitIssueAttachments,
  deleteAttachment,
} from '@/shared/lib/remoteApi';
import {
  extractAttachmentIds,
  removeAttachmentMarkdownBySource,
  replaceAttachmentSource,
} from '@/shared/lib/attachmentUtils';
import { ConfirmDialog } from '@prepcook/ui/components/ConfirmDialog';
import { playbooksApi, sessionsApi, workspacesApi } from '@/shared/lib/api';
import { useLavishSession } from '@/shared/hooks/useLavishSession';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useCurrentKanbanRouteState } from '@/shared/hooks/useCurrentKanbanRouteState';
import {
  buildKanbanIssueComposerKey,
  closeKanbanIssueComposer,
  patchKanbanIssueComposer,
  resetKanbanIssueComposer,
  useKanbanIssueComposer,
  useKanbanIssueComposerStore,
} from '@/shared/stores/useKanbanIssueComposerStore';

interface KanbanIssuePanelContainerProps {
  issueResolution: 'resolving' | 'ready' | 'missing' | null;
  onExpectIssueOpen: (issueId: string) => void;
}

/**
 * KanbanIssuePanelContainer manages the issue detail/create panel.
 * Uses ProjectContext and OrgContext for data and mutations.
 * Must be rendered within both OrgProvider and ProjectProvider.
 */
export function KanbanIssuePanelContainer({
  issueResolution,
  onExpectIssueOpen,
}: KanbanIssuePanelContainerProps) {
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const routeState = useCurrentKanbanRouteState();

  const { openWorkspaceCreateFromState } = useProjectWorkspaceCreateDraft();
  const { workspaces } = useUserContext();
  const { config } = useUserSystem();
  const { activeWorkspaces, archivedWorkspaces } = useWorkspaceContext();

  // Build set of local workspace IDs that exist on this machine
  const localWorkspaceIds = useMemo(
    () =>
      new Set([
        ...activeWorkspaces.map((w) => w.id),
        ...archivedWorkspaces.map((w) => w.id),
      ]),
    [activeWorkspaces, archivedWorkspaces]
  );

  // Get data from contexts
  const {
    projectId,
    issues,
    statuses,
    tags,
    issueAssignees,
    issueTags,
    insertIssue,
    updateIssue,
    insertIssueAssignee,
    insertIssueTag,
    removeIssueTag,
    insertTag,
    getTagsForIssue,
    getPullRequestsForIssue,
    getWorkspacesForIssue,
    isLoading: projectLoading,
  } = useProjectContext();
  const selectedKanbanIssueId = routeState.issueId;
  const issueComposerKey = useMemo(
    () => buildKanbanIssueComposerKey(projectId),
    [projectId]
  );
  const issueComposer = useKanbanIssueComposer(issueComposerKey);
  const kanbanCreateMode = issueComposer !== null;
  const createComposerInitial = issueComposer?.initial ?? null;
  const kanbanCreateDefaultStatusId = createComposerInitial?.statusId ?? null;
  const kanbanCreateDefaultPriority = createComposerInitial?.priority ?? null;
  const kanbanCreateDefaultAssigneeIds =
    createComposerInitial?.assigneeIds ?? null;
  const kanbanCreateDefaultParentIssueId =
    createComposerInitial?.parentIssueId ?? null;
  const createDraftWorkspaceByDefault = useUiPreferencesStore(
    (state) => state.createDraftWorkspaceByDefault
  );
  const setCreateDraftWorkspaceByDefault = useUiPreferencesStore(
    (state) => state.setCreateDraftWorkspaceByDefault
  );
  const openIssue = useCallback(
    (issueId: string) => {
      if (kanbanCreateMode && issueComposerKey) {
        closeKanbanIssueComposer(issueComposerKey);
      }
      appNavigation.goToProjectIssue(projectId, issueId);
    },
    [kanbanCreateMode, issueComposerKey, appNavigation, projectId]
  );
  const closeKanbanIssuePanel = useCallback(() => {
    if (kanbanCreateMode && issueComposerKey) {
      closeKanbanIssueComposer(issueComposerKey);
    }
    appNavigation.goToProject(projectId);
  }, [kanbanCreateMode, issueComposerKey, appNavigation, projectId]);
  const updateIssueComposerDraft = useCallback(
    (patch: {
      statusId?: string;
      priority?: IssuePriority | null;
      assigneeIds?: string[];
      parentIssueId?: string;
      title?: string;
      description?: string | null;
      tagIds?: string[];
      createDraftWorkspace?: boolean;
    }) => {
      if (!kanbanCreateMode || !issueComposerKey) {
        return;
      }

      patchKanbanIssueComposer(issueComposerKey, patch);
    },
    [kanbanCreateMode, issueComposerKey]
  );
  const resetIssueComposerDraft = useCallback(() => {
    if (!issueComposerKey) {
      return;
    }

    resetKanbanIssueComposer(issueComposerKey);
  }, [issueComposerKey]);

  const { isLoading: orgLoading, membersWithProfilesById } = useOrgContext();

  // Get action methods from actions context
  const { openStatusSelection, openPrioritySelection, openAssigneeSelection } =
    useActions();

  // Find selected issue if in edit mode
  const selectedIssue = useMemo(() => {
    if (kanbanCreateMode || !selectedKanbanIssueId) return null;
    return issues.find((i) => i.id === selectedKanbanIssueId) ?? null;
  }, [issues, selectedKanbanIssueId, kanbanCreateMode]);

  const creatorUserId = selectedIssue?.creator_user_id ?? null;
  const issueCreator = useMemo(() => {
    if (!creatorUserId) return null;
    return membersWithProfilesById.get(creatorUserId) ?? null;
  }, [membersWithProfilesById, creatorUserId]);

  // Find parent issue if current issue has one
  const parentIssue = useMemo(() => {
    if (!selectedIssue?.parent_issue_id) return null;
    const parent = issues.find((i) => i.id === selectedIssue.parent_issue_id);
    if (!parent) return null;
    return { id: parent.id, simpleId: parent.simple_id };
  }, [issues, selectedIssue]);

  // Handler for clicking on parent issue - navigate to that issue
  const handleParentIssueClick = useCallback(() => {
    if (parentIssue) {
      openIssue(parentIssue.id);
    }
  }, [parentIssue, openIssue]);

  const handleRemoveParentIssue = useCallback(() => {
    if (!selectedKanbanIssueId || !selectedIssue?.parent_issue_id) return;
    updateIssue(selectedKanbanIssueId, {
      parent_issue_id: null,
      parent_issue_sort_order: null,
    });
  }, [selectedKanbanIssueId, selectedIssue?.parent_issue_id, updateIssue]);

  // Get all current assignees from issue_assignees
  const currentAssigneeIds = useMemo(() => {
    if (!selectedKanbanIssueId) return [];
    return issueAssignees
      .filter((a) => a.issue_id === selectedKanbanIssueId)
      .map((a) => a.user_id);
  }, [issueAssignees, selectedKanbanIssueId]);

  // Get current tag IDs from issue_tags junction table
  const currentTagIds = useMemo(() => {
    if (!selectedKanbanIssueId) return [];
    const tagLinks = getTagsForIssue(selectedKanbanIssueId);
    return tagLinks.map((it) => it.tag_id);
  }, [getTagsForIssue, selectedKanbanIssueId]);

  // Get linked PRs for the issue
  const linkedPrs = useMemo(() => {
    if (!selectedKanbanIssueId) return [];
    return getPullRequestsForIssue(selectedKanbanIssueId).map((pr) => ({
      id: pr.id,
      number: pr.number,
      url: pr.url,
      status: pr.status,
    }));
  }, [getPullRequestsForIssue, selectedKanbanIssueId]);

  // Resolve the stage playbook for the issue's work-class and current column.
  // Null when the issue is outside Planning/Executing or no template is set.
  const stagePlaybookArgs = useMemo(() => {
    if (!selectedIssue || !config) return null;

    const statusName =
      statuses.find((status) => status.id === selectedIssue.status_id)?.name ??
      null;
    const tagNamesById = new Map(tags.map((tag) => [tag.id, tag.name]));
    const tagNames = getTagsForIssue(selectedIssue.id)
      .map((issueTag) => tagNamesById.get(issueTag.tag_id))
      .filter((name): name is string => name != null);

    return {
      issue: selectedIssue,
      tagNames,
      statusName,
      playbooks: config.playbooks,
    };
  }, [selectedIssue, config, statuses, tags, getTagsForIssue]);
  // Without the board's context: enough to know whether there is a playbook
  // and which stage it is. The prompt is filled when the agent is started.
  const stagePlaybook = useMemo(
    () => (stagePlaybookArgs ? resolveStagePlaybook(stagePlaybookArgs) : null),
    [stagePlaybookArgs]
  );

  // Shipped stage detection (alias-aware: an older board's Push MR or Done
  // too) for the Review-MR-comments action.
  const isPushMrStage = useMemo(() => {
    if (!selectedIssue) return false;
    const statusName =
      statuses.find((status) => status.id === selectedIssue.status_id)?.name ??
      '';
    return workflowStageKey(statusName) === 'shipped';
  }, [selectedIssue, statuses]);

  // Newest non-reviewer workspace linked to the issue that exists on this
  // machine — the implementation workspace whose branch carries the MR.
  const implWorkspaceId = useMemo(() => {
    if (!selectedIssue) return null;
    const candidates = getWorkspacesForIssue(selectedIssue.id)
      .filter(
        (workspace) =>
          !(workspace.name ?? '').startsWith('Review: ') &&
          workspace.local_workspace_id != null &&
          localWorkspaceIds.has(workspace.local_workspace_id)
      )
      .sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
    return candidates[0]?.local_workspace_id ?? null;
  }, [selectedIssue, getWorkspacesForIssue, localWorkspaceIds]);

  // Live Lavish review-gate session for the implementation workspace (5s
  // poll, deduped with the board cards' queries by workspace id) — powers
  // the panel's "Review plan" button.
  const { data: lavishSession } = useLavishSession(implWorkspaceId);

  // Reopen a dead review gate: spawns the headless lavish command in the
  // worktree; the poll above swaps the button for "Review plan" once the
  // session registers. Stays "opening" until then (or until an error).
  const [isReopeningReview, setIsReopeningReview] = useState(false);
  useEffect(() => {
    if (lavishSession?.url) {
      setIsReopeningReview(false);
    }
  }, [lavishSession?.url]);
  // The panel container survives ticket switches — the pending state is
  // per-ticket, so clear it when another issue opens.
  useEffect(() => {
    setIsReopeningReview(false);
  }, [selectedKanbanIssueId]);
  // Give up after 30s: if no session registered by then, the spawn failed
  // (check the app log) and the button should become clickable again.
  useEffect(() => {
    if (!isReopeningReview) return;
    const timer = setTimeout(() => setIsReopeningReview(false), 30_000);
    return () => clearTimeout(timer);
  }, [isReopeningReview]);
  const handleReopenReview = useCallback(
    async (specPath: string) => {
      if (!implWorkspaceId || isReopeningReview) return;
      setIsReopeningReview(true);
      try {
        await workspacesApi.reopenLavishSession(implWorkspaceId, {
          file: specPath,
        });
      } catch (error) {
        setIsReopeningReview(false);
        console.error('Failed to reopen the review gate:', error);
        await ConfirmDialog.show({
          title: t('common:error'),
          message:
            error instanceof Error
              ? error.message
              : t(
                  'kanban.reopenReviewError',
                  'Failed to reopen the review gate. Please try again.'
                ),
          confirmText: t('common:ok'),
          showCancelButton: false,
        });
      }
    },
    [implWorkspaceId, isReopeningReview, t]
  );

  // Spec/RCA artifact state from extension_metadata: path + Lavish approval.
  // Undefined (nothing rendered) when the issue has no spec recorded.
  const specStatus = useMemo(() => {
    const meta = selectedIssue?.extension_metadata;
    if (!meta || typeof meta !== 'object') return undefined;
    const spec = (meta as Record<string, unknown>).spec;
    if (!spec || typeof spec !== 'object') return undefined;
    const { path, approved_at: approvedAt } = spec as Record<string, unknown>;
    return {
      path: typeof path === 'string' ? path : null,
      approvedAt: typeof approvedAt === 'string' ? approvedAt : null,
    };
  }, [selectedIssue]);

  // Planning verdict from extension_metadata (kind/confidence/open questions/
  // summary) — the agent-recorded half of the executing gate, shown next to
  // the Lavish approval state so reviewers can see why a ticket is (or is
  // not) eligible to advance without querying the database.
  const planningVerdict = useMemo(() => {
    const meta = selectedIssue?.extension_metadata;
    if (!meta || typeof meta !== 'object') return undefined;
    const planning = (meta as Record<string, unknown>).planning;
    if (!planning || typeof planning !== 'object') return undefined;
    const {
      kind,
      confidence,
      confidence_reason: confidenceReason,
      open_questions: openQuestions,
      summary,
    } = planning as Record<string, unknown>;
    return {
      kind: typeof kind === 'string' ? kind : null,
      confidence: typeof confidence === 'string' ? confidence : null,
      confidenceReason:
        typeof confidenceReason === 'string' ? confidenceReason : null,
      openQuestions: typeof openQuestions === 'number' ? openQuestions : null,
      summary: typeof summary === 'string' ? summary : null,
    };
  }, [selectedIssue]);

  // Human-takeover pause marker (assignee moved off the agent in Jira);
  // rendered as an amber "paused" row on the panel.
  const humanTakeover = useMemo(() => {
    const meta = selectedIssue?.extension_metadata;
    if (!meta || typeof meta !== 'object') return undefined;
    const takeover = (meta as Record<string, unknown>).human_takeover;
    if (!takeover || typeof takeover !== 'object') return undefined;
    const { assignee, detected_at: detectedAt } = takeover as Record<
      string,
      unknown
    >;
    if (typeof detectedAt !== 'string') return undefined;
    return { assignee: typeof assignee === 'string' ? assignee : null };
  }, [selectedIssue]);

  // Determine mode from composer state (create) or issue route (edit).
  const mode = kanbanCreateMode ? 'create' : 'edit';

  // Sort statuses by sort_order
  const sortedStatuses = useMemo(
    () => [...statuses].sort((a, b) => a.sort_order - b.sort_order),
    [statuses]
  );

  // When the project has an "Unassigned" status (case-insensitive), new
  // issues are forced into that column: it is both the default and locked in
  // create mode. Edit mode and projects without such a status are unaffected.
  const lockedCreateStatusId = useMemo(
    () =>
      sortedStatuses.find((s) =>
        ['to do', 'unassigned'].includes(s.name.trim().toLowerCase())
      )?.id ?? null,
    [sortedStatuses]
  );

  // Default status: locked "Unassigned" status when present, otherwise
  // kanbanCreateDefaultStatusId if set, otherwise first by sort order
  const defaultStatusId =
    lockedCreateStatusId ??
    kanbanCreateDefaultStatusId ??
    sortedStatuses[0]?.id ??
    '';

  // Default create form values for the current create-default state + project context
  const createModeDefaults = useMemo<IssueFormData>(
    () => ({
      title: '',
      description: null,
      statusId: defaultStatusId,
      priority: kanbanCreateDefaultPriority ?? null,
      assigneeIds: [...(kanbanCreateDefaultAssigneeIds ?? [])],
      tagIds: [],
      createDraftWorkspace: createDraftWorkspaceByDefault,
    }),
    [
      defaultStatusId,
      kanbanCreateDefaultPriority,
      kanbanCreateDefaultAssigneeIds,
      createDraftWorkspaceByDefault,
    ]
  );

  // Track previous issue ID to detect actual issue switches (not just data updates)
  const prevIssueIdRef = useRef<string | null>(null);
  const prevHasPendingAttachmentsRef = useRef(false);
  const hasPendingAttachmentsRef = useRef(false);
  const titleInputRef = useRef<HTMLTextAreaElement>(null);

  const [formState, dispatchFormState] = useReducer(
    kanbanIssuePanelFormReducer,
    undefined,
    createInitialKanbanIssuePanelFormState
  );
  const createFormData = formState.createFormData;

  useEffect(() => {
    if (mode !== 'create') return;

    const titleInput = titleInputRef.current;
    if (!titleInput || document.activeElement === titleInput) return;

    const frameId = requestAnimationFrame(() => {
      const node = titleInputRef.current;
      if (!node || document.activeElement === node) return;

      node.focus();
      const caretIndex = node.value.length;
      node.setSelectionRange(caretIndex, caretIndex);
    });

    return () => cancelAnimationFrame(frameId);
  }, [mode, selectedKanbanIssueId, createFormData?.title]);

  // Display ID: use real simple_id in edit mode, placeholder for create mode
  // The tracker key when the card has one (as the card face shows it).
  const displayId = useMemo(() => {
    if (mode === 'edit' && selectedIssue) {
      return (
        parseIssueExtMetadata(selectedIssue.extension_metadata)?.jira?.key ??
        selectedIssue.simple_id
      );
    }
    return t('kanban.newCard');
  }, [mode, selectedIssue, t]);

  // Compute display values based on mode
  // - Create mode: createFormData is the single source of truth.
  // - Edit mode: text fields come from explicit local edit state, dropdown fields from server.
  const displayData = useMemo((): IssueFormData => {
    return selectDisplayData({
      state: formState,
      mode,
      createModeDefaults,
      selectedIssue,
      currentAssigneeIds,
      currentTagIds,
    });
  }, [
    formState,
    mode,
    createModeDefaults,
    selectedIssue,
    currentAssigneeIds,
    currentTagIds,
  ]);
  const latestDescriptionRef = useRef<string | null>(
    displayData.description ?? null
  );
  latestDescriptionRef.current = displayData.description ?? null;

  const isCreateDraftDirty = useMemo(() => {
    return selectIsCreateDraftDirty({
      state: formState,
      mode,
      createModeDefaults,
    });
  }, [formState, mode, createModeDefaults]);

  // Resolve assignee IDs to full profiles for avatar display
  const displayAssigneeUsers = useMemo(() => {
    return displayData.assigneeIds
      .map((id) => membersWithProfilesById.get(id))
      .filter((m): m is OrganizationMemberWithProfile => m != null);
  }, [displayData.assigneeIds, membersWithProfilesById]);

  const [isSubmitting, setIsSubmitting] = useState(false);

  // Save status for description (shown in WYSIWYG toolbar)
  const [descriptionSaveStatus, setDescriptionSaveStatus] = useState<
    'idle' | 'saved'
  >('idle');

  // Debounced save for title changes
  const { debounced: debouncedSaveTitle, cancel: cancelDebouncedTitle } =
    useDebouncedCallback((title: string) => {
      if (selectedKanbanIssueId && !kanbanCreateMode) {
        updateIssue(selectedKanbanIssueId, { title });
      }
    }, 500);

  // Debounced save for description changes
  const {
    debounced: debouncedSaveDescription,
    cancel: cancelDebouncedDescription,
  } = useDebouncedCallback((description: string | null) => {
    if (selectedKanbanIssueId && !kanbanCreateMode) {
      updateIssue(selectedKanbanIssueId, { description });
      setDescriptionSaveStatus('saved');
      setTimeout(() => setDescriptionSaveStatus('idle'), 1500);
    }
  }, 500);

  // Reset save status only when switching to a different issue or mode
  useEffect(() => {
    setDescriptionSaveStatus('idle');
  }, [selectedKanbanIssueId, kanbanCreateMode]);

  const createFormFallback = useMemo(
    () =>
      createBlankCreateFormData(defaultStatusId, createDraftWorkspaceByDefault),
    [defaultStatusId, createDraftWorkspaceByDefault]
  );

  // --- Image attachment upload integration ---

  // Callback to insert markdown into the description field
  const handleDescriptionInsert = useCallback(
    (markdown: string, options?: { persist?: boolean }) => {
      const currentDesc = latestDescriptionRef.current ?? '';
      const separator = currentDesc.length > 0 ? '\n' : '';
      const newDesc = currentDesc + separator + markdown;
      latestDescriptionRef.current = newDesc;

      if (kanbanCreateMode || !selectedKanbanIssueId) {
        // Create mode: update form data
        dispatchFormState({
          type: 'patchCreateFormData',
          patch: { description: newDesc },
          fallback: createFormFallback,
        });
      } else {
        // Edit mode: update local state + debounced save
        dispatchFormState({
          type: 'setEditDescription',
          description: newDesc,
        });
        if (options?.persist !== false && !hasPendingAttachmentsRef.current) {
          debouncedSaveDescription(newDesc);
        }
      }
    },
    [
      kanbanCreateMode,
      selectedKanbanIssueId,
      createFormFallback,
      debouncedSaveDescription,
    ]
  );

  const handleDescriptionSourceReplace = useCallback(
    (previousSrc: string, nextSrc: string, options?: { persist?: boolean }) => {
      const currentDesc = latestDescriptionRef.current ?? '';
      const { content: nextDesc, replaced } = replaceAttachmentSource(
        currentDesc,
        previousSrc,
        nextSrc
      );

      if (!replaced) {
        return false;
      }
      latestDescriptionRef.current = nextDesc;

      if (kanbanCreateMode || !selectedKanbanIssueId) {
        dispatchFormState({
          type: 'patchCreateFormData',
          patch: { description: nextDesc },
          fallback: createFormFallback,
        });
      } else {
        dispatchFormState({
          type: 'setEditDescription',
          description: nextDesc,
        });
        if (options?.persist !== false && !hasPendingAttachmentsRef.current) {
          debouncedSaveDescription(nextDesc);
        }
      }

      return true;
    },
    [
      kanbanCreateMode,
      selectedKanbanIssueId,
      createFormFallback,
      debouncedSaveDescription,
    ]
  );

  const handleDescriptionSourceRemove = useCallback(
    (src: string, options?: { persist?: boolean }) => {
      const currentDesc = latestDescriptionRef.current ?? '';
      const { content: nextDesc, removed } = removeAttachmentMarkdownBySource(
        currentDesc,
        src
      );

      if (!removed) {
        return false;
      }
      latestDescriptionRef.current = nextDesc || null;

      if (kanbanCreateMode || !selectedKanbanIssueId) {
        dispatchFormState({
          type: 'patchCreateFormData',
          patch: { description: nextDesc || null },
          fallback: createFormFallback,
        });
      } else {
        dispatchFormState({
          type: 'setEditDescription',
          description: nextDesc || null,
        });
        if (options?.persist !== false && !hasPendingAttachmentsRef.current) {
          debouncedSaveDescription(nextDesc || null);
        }
      }

      return true;
    },
    [
      kanbanCreateMode,
      selectedKanbanIssueId,
      createFormFallback,
      debouncedSaveDescription,
    ]
  );

  // Azure attachment upload hook
  const {
    uploadFiles,
    getAttachmentIds,
    clearAttachments,
    isUploading,
    hasPendingAttachments,
    uploadError,
    clearUploadError,
    localAttachments,
  } = useAzureAttachments({
    projectId,
    issueId: kanbanCreateMode
      ? undefined
      : (selectedKanbanIssueId ?? undefined),
    onMarkdownInsert: handleDescriptionInsert,
    onAttachmentSourceReplace: handleDescriptionSourceReplace,
    onAttachmentSourceRemove: handleDescriptionSourceRemove,
    onError: (msg) => console.error('[attachment]', msg),
  });
  hasPendingAttachmentsRef.current = hasPendingAttachments;

  // Dropzone for drag-drop image upload on description area
  const {
    getRootProps,
    getInputProps,
    isDragActive,
    open: openFilePicker,
  } = useDropzone({
    onDrop: (acceptedFiles) => {
      if (acceptedFiles.length > 0) uploadFiles(acceptedFiles);
    },
    multiple: true,
    noClick: true,
    noKeyboard: true,
  });

  // Paste handler for images
  const onPasteFiles = useCallback(
    (files: File[]) => {
      if (files.length > 0) uploadFiles(files);
    },
    [uploadFiles]
  );

  // Reset local state when switching issues or modes.
  useEffect(() => {
    const currentIssueId = selectedKanbanIssueId;
    const isNewIssue = currentIssueId !== prevIssueIdRef.current;
    const shouldSeedCreateForm = mode === 'create' && createFormData === null;

    if (!isNewIssue && !shouldSeedCreateForm) {
      // Same issue - no reset needed
      // (dropdown fields derive from server state, text fields preserve local edits)
      return;
    }

    // Track the new issue ID
    prevIssueIdRef.current = currentIssueId;

    // Cancel any pending debounced saves when switching issues
    cancelDebouncedTitle();
    cancelDebouncedDescription();

    let nextCreateFormData: IssueFormData | null = null;
    let restoredFromScratch = false;

    if (mode === 'create') {
      // Check if the composer store has a saved draft (e.g., restored from
      // localStorage on remote-web). Use it to seed the form instead of defaults.
      const composerDraft =
        useKanbanIssueComposerStore.getState().byKey[issueComposerKey]?.draft;
      const hasSavedDraft =
        composerDraft != null &&
        (composerDraft.title !== '' || composerDraft.description != null);

      if (hasSavedDraft) {
        nextCreateFormData = {
          title: composerDraft.title,
          description: composerDraft.description ?? null,
          statusId:
            lockedCreateStatusId ??
            composerDraft.statusId ??
            createModeDefaults.statusId,
          priority:
            composerDraft.priority === undefined
              ? createModeDefaults.priority
              : composerDraft.priority,
          assigneeIds:
            composerDraft.assigneeIds ?? createModeDefaults.assigneeIds,
          tagIds: composerDraft.tagIds ?? createModeDefaults.tagIds,
          createDraftWorkspace:
            composerDraft.createDraftWorkspace ??
            createModeDefaults.createDraftWorkspace,
        };
        restoredFromScratch = true;
      } else {
        nextCreateFormData = createModeDefaults;
      }
    }

    dispatchFormState({
      type: 'resetForIssueChange',
      mode,
      createFormData: nextCreateFormData,
      hasRestoredFromScratch: restoredFromScratch,
    });
  }, [
    mode,
    createFormData,
    selectedKanbanIssueId,
    cancelDebouncedTitle,
    cancelDebouncedDescription,
    createModeDefaults,
    issueComposerKey,
    lockedCreateStatusId,
  ]);

  useEffect(() => {
    const wasPending = prevHasPendingAttachmentsRef.current;
    prevHasPendingAttachmentsRef.current = hasPendingAttachments;

    if (kanbanCreateMode || !selectedKanbanIssueId) {
      return;
    }

    if (!wasPending || hasPendingAttachments) {
      return;
    }

    const currentDescription = displayData.description ?? null;
    const persistedDescription = selectedIssue?.description ?? null;

    if (currentDescription === persistedDescription) {
      return;
    }

    debouncedSaveDescription(currentDescription);
  }, [
    kanbanCreateMode,
    selectedKanbanIssueId,
    hasPendingAttachments,
    displayData.description,
    selectedIssue?.description,
    debouncedSaveDescription,
  ]);

  // Form change handler - persists changes immediately in edit mode
  const handlePropertyChange = useCallback(
    async <K extends keyof IssueFormData>(
      field: K,
      value: IssueFormData[K]
    ) => {
      // Create mode: update in-panel form state and composer draft.
      if (kanbanCreateMode) {
        // For statusId, open the status selection dialog with callback
        if (field === 'statusId') {
          // Status is locked to the "Unassigned" column when it exists.
          if (lockedCreateStatusId) {
            return;
          }
          const { ProjectSelectionDialog } = await import(
            '@/shared/dialogs/command-bar/selections/ProjectSelectionDialog'
          );
          const result = await ProjectSelectionDialog.show({
            projectId,
            selection: { type: 'status', issueIds: [], isCreateMode: true },
          });
          if (result && typeof result === 'object' && 'statusId' in result) {
            const statusId = result.statusId as string;
            updateIssueComposerDraft({ statusId });
            dispatchFormState({
              type: 'patchCreateFormData',
              patch: { statusId },
              fallback: createFormFallback,
            });
          }
          return;
        }

        // For priority, open the priority selection dialog with callback
        if (field === 'priority') {
          const { ProjectSelectionDialog } = await import(
            '@/shared/dialogs/command-bar/selections/ProjectSelectionDialog'
          );
          const result = await ProjectSelectionDialog.show({
            projectId,
            selection: { type: 'priority', issueIds: [], isCreateMode: true },
          });
          if (result && typeof result === 'object' && 'priority' in result) {
            const priority = (result as { priority: IssuePriority | null })
              .priority;
            updateIssueComposerDraft({ priority });
            dispatchFormState({
              type: 'patchCreateFormData',
              patch: { priority },
              fallback: createFormFallback,
            });
          }
          return;
        }

        // For assigneeIds, open the assignee selection dialog with callback
        if (field === 'assigneeIds') {
          const { AssigneeSelectionDialog } = await import(
            '@/shared/dialogs/kanban/AssigneeSelectionDialog'
          );
          await AssigneeSelectionDialog.show({
            projectId,
            issueIds: [],
            isCreateMode: true,
            createModeAssigneeIds: createFormData?.assigneeIds ?? [],
            onCreateModeAssigneesChange: (assigneeIds: string[]) => {
              updateIssueComposerDraft({ assigneeIds });
              dispatchFormState({
                type: 'setCreateAssigneeIds',
                assigneeIds,
              });
            },
          });
          return;
        }

        // For other fields, just update the form data
        dispatchFormState({
          type: 'patchCreateFormData',
          patch: { [field]: value } as Partial<IssueFormData>,
          fallback: createFormFallback,
        });
        updateIssueComposerDraft({ [field]: value } as Partial<IssueFormData>);
        if (field === 'createDraftWorkspace') {
          setCreateDraftWorkspaceByDefault(value as boolean);
        }
        return;
      }

      if (!selectedKanbanIssueId) {
        return;
      }

      // Edit mode: handle text fields vs dropdown fields differently
      if (field === 'title') {
        // Text field: update local state, then debounced save
        dispatchFormState({
          type: 'setEditTitle',
          title: value as string,
        });
        debouncedSaveTitle(value as string);
      } else if (field === 'description') {
        // Text field: update local state, then debounced save
        dispatchFormState({
          type: 'setEditDescription',
          description: value as string | null,
        });
        if (!hasPendingAttachments) {
          debouncedSaveDescription(value as string | null);
        }
      } else if (field === 'statusId') {
        // Status changes go through the command bar status selection
        openStatusSelection(projectId, [selectedKanbanIssueId]);
      } else if (field === 'priority') {
        // Priority changes go through the command bar priority selection
        openPrioritySelection(projectId, [selectedKanbanIssueId]);
      } else if (field === 'assigneeIds') {
        // Assignee changes go through the assignee selection dialog
        openAssigneeSelection(projectId, [selectedKanbanIssueId], false);
      } else if (field === 'tagIds') {
        // Handle tag changes via junction table
        const newTagIds = value as string[];
        const currentIssueTags = issueTags.filter(
          (it) => it.issue_id === selectedKanbanIssueId
        );
        const currentTagIdSet = new Set(
          currentIssueTags.map((it) => it.tag_id)
        );
        const newTagIdSet = new Set(newTagIds);

        // Remove tags that are no longer selected
        for (const issueTag of currentIssueTags) {
          if (!newTagIdSet.has(issueTag.tag_id)) {
            removeIssueTag(issueTag.id);
          }
        }

        // Add newly selected tags
        for (const tagId of newTagIds) {
          if (!currentTagIdSet.has(tagId)) {
            insertIssueTag({
              issue_id: selectedKanbanIssueId,
              tag_id: tagId,
            });
          }
        }
      }
    },
    [
      kanbanCreateMode,
      selectedKanbanIssueId,
      projectId,
      createFormFallback,
      createFormData,
      lockedCreateStatusId,
      hasPendingAttachments,
      debouncedSaveTitle,
      debouncedSaveDescription,
      openStatusSelection,
      openPrioritySelection,
      openAssigneeSelection,
      updateIssueComposerDraft,
      setCreateDraftWorkspaceByDefault,
      issueTags,
      insertIssueTag,
      removeIssueTag,
    ]
  );

  // Submit handler
  const handleSubmit = useCallback(async () => {
    if (!displayData.title.trim() || hasPendingAttachments) return;

    setIsSubmitting(true);
    try {
      if (mode === 'create') {
        // New issues always land in the locked "Unassigned" column when the
        // project has one, regardless of any stale draft status.
        const createStatusId = lockedCreateStatusId ?? displayData.statusId;

        // Create new issue at the top of the column
        const statusIssues = issues.filter(
          (i) => i.status_id === createStatusId
        );
        const minSortOrder =
          statusIssues.length > 0
            ? Math.min(...statusIssues.map((i) => i.sort_order))
            : 0;

        const { persisted } = insertIssue({
          project_id: projectId,
          status_id: createStatusId,
          title: displayData.title,
          description: displayData.description,
          priority: displayData.priority,
          sort_order: minSortOrder - 1,
          start_date: null,
          target_date: null,
          completed_at: null,
          parent_issue_id: kanbanCreateDefaultParentIssueId,
          parent_issue_sort_order: null,
          extension_metadata: null,
        });

        // Wait for the issue to be confirmed by the backend and get the synced entity
        const syncedIssue = await persisted;

        // Commit only attachments still referenced in the description
        const allUploadedIds = getAttachmentIds();
        if (allUploadedIds.length > 0) {
          const referencedIds = extractAttachmentIds(
            displayData.description ?? ''
          );
          const idsToCommit = allUploadedIds.filter((id) =>
            referencedIds.has(id)
          );
          const idsToDelete = allUploadedIds.filter(
            (id) => !referencedIds.has(id)
          );

          if (idsToCommit.length > 0) {
            await commitIssueAttachments(syncedIssue.id, {
              attachment_ids: idsToCommit,
            });
          }
          for (const id of idsToDelete) {
            deleteAttachment(id).catch((err) =>
              console.error('Failed to delete abandoned attachment:', err)
            );
          }
          clearAttachments();
        }

        // Create assignee records for all selected assignees
        displayData.assigneeIds.forEach((userId) => {
          insertIssueAssignee({
            issue_id: syncedIssue.id,
            user_id: userId,
          });
        });

        // Create tag records if tags were selected
        for (const tagId of displayData.tagIds) {
          insertIssueTag({
            issue_id: syncedIssue.id,
            tag_id: tagId,
          });
        }

        if (issueComposerKey) {
          closeKanbanIssueComposer(issueComposerKey);
        }

        if (displayData.createDraftWorkspace) {
          const initialPrompt = buildWorkspaceCreatePrompt(
            displayData.title,
            displayData.description
          );

          // Get defaults from most recent workspace
          const defaults = await getWorkspaceDefaults(
            workspaces,
            localWorkspaceIds,
            projectId
          );

          const createState = buildWorkspaceCreateInitialState({
            prompt: initialPrompt,
            defaults,
            linkedIssue: buildLinkedIssueCreateState(syncedIssue, projectId),
          });
          const draftId = await openWorkspaceCreateFromState(createState, {
            issueId: syncedIssue.id,
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
            onExpectIssueOpen?.(syncedIssue.id);
            openIssue(syncedIssue.id);
          }
          return; // Don't open issue panel since we're navigating away
        }

        // Open the newly created issue
        onExpectIssueOpen?.(syncedIssue.id);
        openIssue(syncedIssue.id);
      } else {
        // Update existing issue - would use update mutation
        // For now, just close the panel
        closeKanbanIssuePanel();
      }
    } catch (error) {
      console.error('Failed to save issue:', error);
    } finally {
      setIsSubmitting(false);
    }
  }, [
    mode,
    displayData,
    lockedCreateStatusId,
    projectId,
    issues,
    insertIssue,
    insertIssueAssignee,
    insertIssueTag,
    openIssue,
    kanbanCreateDefaultParentIssueId,
    openWorkspaceCreateFromState,
    workspaces,
    localWorkspaceIds,
    closeKanbanIssuePanel,
    issueComposerKey,
    getAttachmentIds,
    clearAttachments,
    hasPendingAttachments,
    onExpectIssueOpen,
    t,
  ]);

  const handleCmdEnterSubmit = useCallback(() => {
    if (mode !== 'create') return;
    void handleSubmit();
  }, [mode, handleSubmit]);

  const handleDeleteDraft = useCallback(() => {
    dispatchFormState({
      type: 'setCreateFormData',
      createFormData: createModeDefaults,
    });
    resetIssueComposerDraft();
  }, [createModeDefaults, resetIssueComposerDraft]);

  // Tag create callback - returns the new tag ID so it can be auto-selected
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

  // Copy link callback - copies issue URL to clipboard
  const handleCopyLink = useCallback(() => {
    if (!selectedKanbanIssueId || !projectId) return;
    const url = `${window.location.origin}/projects/${projectId}/issues/${selectedKanbanIssueId}`;
    navigator.clipboard.writeText(url);
  }, [projectId, selectedKanbanIssueId]);

  // More actions callback - opens command bar with issue actions
  const handleMoreActions = useCallback(async () => {
    if (!selectedKanbanIssueId || !projectId) return;
    await CommandBarDialog.show({
      page: 'issueActions',
      projectId,
      issueIds: [selectedKanbanIssueId],
    });
  }, [selectedKanbanIssueId, projectId]);

  // Run stage action - starts an agent workspace with the playbook prompt
  // prefilled, linked to the issue (user still picks repos/agent in the
  // create UI).
  const [isStartingStageAction, setIsStartingStageAction] = useState(false);
  const handleRunStageAction = useCallback(async () => {
    if (
      !selectedIssue ||
      !stagePlaybook ||
      !stagePlaybookArgs ||
      isStartingStageAction
    )
      return;

    setIsStartingStageAction(true);
    try {
      const context = await playbooksApi
        .getContext(projectId)
        .catch(() => ({}));
      const prompt =
        resolveStagePlaybook({ ...stagePlaybookArgs, context })?.prompt ??
        stagePlaybook.prompt;
      const defaults = await getWorkspaceDefaults(
        workspaces,
        localWorkspaceIds,
        projectId
      );
      const createState = buildWorkspaceCreateInitialState({
        prompt,
        defaults,
        linkedIssue: buildLinkedIssueCreateState(selectedIssue, projectId),
      });
      const draftId = await openWorkspaceCreateFromState(createState, {
        issueId: selectedIssue.id,
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
    } finally {
      setIsStartingStageAction(false);
    }
  }, [
    selectedIssue,
    stagePlaybook,
    stagePlaybookArgs,
    isStartingStageAction,
    workspaces,
    localWorkspaceIds,
    projectId,
    openWorkspaceCreateFromState,
    t,
  ]);

  // Review MR comments (Push MR stage): pull the MR's review comments and
  // hand the selected ones to the implementation workspace's agent as a
  // follow-up (same gh-comment block format the workspace chat inserts, so
  // the conversation renders them as comment cards).
  const handleReviewMrComments = useCallback(async () => {
    if (!implWorkspaceId || isStartingStageAction) return;
    setIsStartingStageAction(true);
    try {
      const repos = await workspacesApi.getRepos(implWorkspaceId);
      const repoId = repos[0]?.id;
      if (!repoId) return;

      const { PrCommentsDialog } = await import(
        '@/shared/dialogs/tasks/PrCommentsDialog'
      );
      const result = await PrCommentsDialog.show({
        workspaceId: implWorkspaceId,
        repoId,
      });
      if (result.comments.length === 0) return;

      const blocks = result.comments.map((comment) => {
        const payload = {
          id:
            comment.comment_type === 'general'
              ? comment.id
              : comment.id.toString(),
          comment_type: comment.comment_type,
          author: comment.author,
          body: comment.body,
          created_at: comment.created_at,
          url: comment.url,
          ...(comment.comment_type === 'review' && {
            path: comment.path,
            line: comment.line != null ? Number(comment.line) : null,
            diff_hunk: comment.diff_hunk,
          }),
        };
        return '```gh-comment\n' + JSON.stringify(payload, null, 2) + '\n```';
      });
      const mrUrl = linkedPrs[0]?.url;
      const prompt = [
        `Review comments were left on the MR${mrUrl ? ` (${mrUrl})` : ''}. Address each comment below: make the change and commit + push to update the MR, or reply on the MR explaining why you disagree (use glab, not gh).`,
        ...blocks,
      ].join('\n\n');

      const sessions = await sessionsApi.getByWorkspace(implWorkspaceId);
      const session =
        [...sessions].sort((a, b) =>
          a.created_at < b.created_at ? 1 : -1
        )[0] ?? (await sessionsApi.create({ workspace_id: implWorkspaceId }));
      const profile = config?.executor_profile;
      if (!profile) return;
      await sessionsApi.followUp(session.id, {
        prompt,
        executor_config: {
          executor: profile.executor,
          variant: profile.variant ?? null,
        },
        retry_process_id: null,
        force_when_dirty: null,
        perform_git_reset: null,
      });
      appNavigation.goToWorkspace(implWorkspaceId);
    } catch (error) {
      console.error('Failed to send MR comments to the agent:', error);
      await ConfirmDialog.show({
        title: t('common:error'),
        message: t(
          'kanban.mrCommentsError',
          'Failed to send the MR comments to the agent. Please try again.'
        ),
        confirmText: t('common:ok'),
        showCancelButton: false,
      });
    } finally {
      setIsStartingStageAction(false);
    }
  }, [
    implWorkspaceId,
    isStartingStageAction,
    linkedPrs,
    config?.executor_profile,
    appNavigation,
    t,
  ]);

  // --- Card facts for the panel header and the decision block -------------

  const selectedMeta = useMemo(
    () =>
      selectedIssue
        ? parseIssueExtMetadata(selectedIssue.extension_metadata)
        : null,
    [selectedIssue]
  );
  const selectedStatusName = selectedIssue
    ? (statuses.find((status) => status.id === selectedIssue.status_id)?.name ??
      null)
    : null;
  const selectedStage = workflowStageKey(selectedStatusName);
  const selectedWorkClass = useMemo(() => {
    if (!selectedIssue) return null;
    const tagNamesById = new Map(tags.map((tag) => [tag.id, tag.name]));
    return cardWorkClass(
      selectedMeta,
      getTagsForIssue(selectedIssue.id)
        .map((issueTag) => tagNamesById.get(issueTag.tag_id))
        .filter((name): name is string => name != null)
    );
  }, [selectedIssue, selectedMeta, tags, getTagsForIssue]);
  const selectedPullRequests = useMemo(
    () => (selectedIssue ? getPullRequestsForIssue(selectedIssue.id) : []),
    [selectedIssue, getPullRequestsForIssue]
  );

  const trackerName = t(
    `kanban.trackerNames.${config?.ticketing?.kind ?? 'jira'}`
  );
  const trackerRef = selectedMeta?.jira ?? null;
  const classChip = selectedWorkClass
    ? {
        name: t(`kanban.trackerImport.workClass.${selectedWorkClass}`),
        tone: workClassChipTone(selectedWorkClass),
      }
    : null;

  const implBranch = implWorkspaceId
    ? (activeWorkspaces.find((workspace) => workspace.id === implWorkspaceId)
        ?.branch ??
      archivedWorkspaces.find((workspace) => workspace.id === implWorkspaceId)
        ?.branch ??
      null)
    : null;

  const extraProperties = useMemo(() => {
    const properties: IssueExtraProperty[] = [];
    if (
      classChip &&
      selectedStatusName &&
      (selectedStage === 'plan' ||
        selectedStage === 'execute' ||
        selectedStage === 'review' ||
        selectedStage === 'shipped')
    ) {
      properties.push({
        key: 'playbook',
        label: t('kanban.panel.playbook'),
        value: t('kanban.panel.playbookValue', {
          workClass: classChip.name,
          stage: selectedStatusName,
        }),
      });
    }
    if (trackerRef?.key) {
      properties.push({
        key: 'tracker',
        label: t('kanban.panel.tracker'),
        value: `${trackerName} · ${trackerRef.key}`,
      });
    }
    // Where the card's text came from, when that was outside the team, and
    // whether its plan ran under the guarded profile because of it.
    const filedFrom = cardOrigin(selectedMeta);
    const guarded = selectedMeta?.planned_guarded === true;
    if (filedFrom || selectedMeta?.filed_externally || guarded) {
      properties.push({
        key: 'origin',
        label: t('kanban.panel.origin'),
        wide: true,
        value: (
          <>
            {filedFrom ? (
              <>
                {t('kanban.origin.filedBy', {
                  filer: t(`kanban.origin.kinds.${filedFrom.kind}`),
                })}{' '}
                {filedFrom.url ? (
                  <a
                    href={filedFrom.url}
                    target="_blank"
                    rel="noreferrer"
                    className="text-brand-secondary hover:underline"
                  >
                    {t('kanban.origin.thread')}
                  </a>
                ) : (
                  filedFrom.ref
                )}
              </>
            ) : selectedMeta?.filed_externally ? (
              t('kanban.origin.publicIssue')
            ) : null}
            {guarded &&
              `${filedFrom || selectedMeta?.filed_externally ? ' · ' : ''}${t(
                'kanban.origin.plannedGuarded'
              )}`}
          </>
        ),
      });
    }
    if (implBranch) {
      properties.push({
        key: 'branch',
        label: t('kanban.panel.branch'),
        value: implBranch,
        mono: true,
        wide: true,
      });
    }
    return properties;
  }, [
    classChip,
    selectedStatusName,
    selectedStage,
    trackerRef,
    trackerName,
    implBranch,
    selectedMeta,
    t,
  ]);

  const [isDecisionSubmitting, setIsDecisionSubmitting] = useState(false);
  useEffect(() => {
    setIsDecisionSubmitting(false);
  }, [selectedKanbanIssueId]);

  const showDecisionError = useCallback(
    (error: unknown, fallback: string) =>
      ConfirmDialog.show({
        title: t('common:error'),
        message:
          error instanceof Error && error.message ? error.message : fallback,
        confirmText: t('common:ok'),
        showCancelButton: false,
      }),
    [t]
  );

  // Approve and request changes go through the approvals endpoint
  // (docs/approvals.md): the same decision a tracker or card comment makes.
  const submitPlanDecision = useCallback(
    async (
      decision: 'approve' | 'request_changes',
      comment?: string
    ): Promise<boolean> => {
      if (!selectedKanbanIssueId) return false;
      setIsDecisionSubmitting(true);
      try {
        await postPlanDecision(selectedKanbanIssueId, decision, comment);
        return true;
      } catch (error) {
        await showDecisionError(error, t('kanban.needsYou.decisionFailed'));
        return false;
      } finally {
        setIsDecisionSubmitting(false);
      }
    },
    [selectedKanbanIssueId, showDecisionError, t]
  );

  // Moves the card on through a status change, as Move to… does.
  const moveSelectedIssueTo = useCallback(
    async (statusId: string) => {
      if (!selectedKanbanIssueId) return;
      setIsDecisionSubmitting(true);
      try {
        await updateIssue(selectedKanbanIssueId, { status_id: statusId })
          .persisted;
      } catch (error) {
        await showDecisionError(error, t('kanban.workflow.moveRejected'));
      } finally {
        setIsDecisionSubmitting(false);
      }
    },
    [selectedKanbanIssueId, updateIssue, showDecisionError, t]
  );

  // Moves the card to Clean up through Clean up's gate: an open pull
  // request is refused with the reason, and a closed, unmerged one is
  // discarded only once confirmed.
  const cleanUpSelectedIssue = useCallback(
    async (statusId: string) => {
      if (!selectedIssue) return;
      const gate = cardCleanupGate(selectedMeta, selectedPullRequests);
      if (gate.kind === 'refused') {
        await showDecisionError(null, t('kanban.cleanUpGate.prOpen'));
        return;
      }
      if (gate.kind === 'confirm' && !(await confirmDiscard(t))) return;
      setIsDecisionSubmitting(true);
      try {
        await updateIssue(selectedIssue.id, {
          status_id: statusId,
          ...(gate.kind === 'confirm'
            ? {
                extension_metadata: withDiscardConfirmed(
                  selectedIssue.extension_metadata
                ),
              }
            : {}),
        }).persisted;
      } catch (error) {
        await showDecisionError(error, t('kanban.workflow.moveRejected'));
      } finally {
        setIsDecisionSubmitting(false);
      }
    },
    [
      selectedIssue,
      selectedMeta,
      selectedPullRequests,
      updateIssue,
      showDecisionError,
      t,
    ]
  );

  const statusIdForStage = useCallback(
    (stage: 'close' | 'cleanUp') =>
      sortedStatuses.find((status) => workflowStageKey(status.name) === stage)
        ?.id ?? null,
    [sortedStatuses]
  );

  // "Open the RCA": the live review page when the gate is open, else the
  // session that wrote the plan.
  const openPlan = useMemo(() => {
    if (lavishSession?.url) {
      const url = lavishSession.url;
      return () => {
        window.open(url, '_blank', 'noopener,noreferrer');
      };
    }
    if (implWorkspaceId && selectedKanbanIssueId) {
      return () =>
        appNavigation.goToProjectIssueWorkspace(
          projectId,
          selectedKanbanIssueId,
          implWorkspaceId
        );
    }
    return null;
  }, [
    lavishSession?.url,
    implWorkspaceId,
    selectedKanbanIssueId,
    appNavigation,
    projectId,
  ]);

  const decision = useMemo((): KanbanIssuePanelDecision | null => {
    if (mode !== 'edit' || !selectedIssue) return null;
    const reason = needsYouReason(
      selectedStage,
      selectedMeta,
      selectedPullRequests
    );
    if (!reason) return null;
    if (reason.kind === 'plan') {
      return {
        kind: 'plan',
        label: needsYouLabel(t, reason),
        detail: planGateDetail(t, reason.gate) || null,
        onApprove: () => void submitPlanDecision('approve'),
        onRequestChanges: (comment) =>
          submitPlanDecision('request_changes', comment),
        onOpenPlan: openPlan,
        openPlanLabel:
          reason.gate.planKind === 'rca'
            ? t('kanban.needsYou.openRca')
            : t('kanban.needsYou.openPlan'),
        isSubmitting: isDecisionSubmitting,
      };
    }
    const cleanUpStatusId = statusIdForStage('cleanUp');
    if (!cleanUpStatusId) return null;
    const onClose = () => void cleanUpSelectedIssue(cleanUpStatusId);
    if (reason.kind === 'triaged') {
      return {
        kind: 'close',
        label: needsYouLabel(t, reason),
        statusLine: {
          tone: 'attention',
          text: t('kanban.card.status.triaged'),
        },
        closeLabel: t('kanban.triaged.close'),
        onClose,
        isSubmitting: isDecisionSubmitting,
      };
    }
    const merged = latestMergedPullRequest(selectedPullRequests);
    const closed =
      reason.kind === 'closedUnmerged'
        ? latestPullRequest(selectedPullRequests)
        : null;
    return {
      kind: 'close',
      label: needsYouLabel(t, reason),
      statusLine: merged
        ? {
            tone: 'merged',
            link: {
              label: pullRequestLabel(t, merged.number),
              url: merged.url,
            },
            text: t('kanban.card.status.mergedWhen', {
              when: formatRelativeTime(
                t,
                merged.merged_at ?? merged.created_at
              ),
            }),
          }
        : closed
          ? {
              tone: 'idle',
              icon: 'pullRequest',
              link: {
                label: pullRequestLabel(t, closed.number),
                url: closed.url,
              },
              text: t('kanban.card.status.prClosed'),
            }
          : null,
      closeLabel: t('kanban.cleanUpAction'),
      onClose,
      isSubmitting: isDecisionSubmitting,
    };
  }, [
    mode,
    selectedIssue,
    selectedStage,
    selectedMeta,
    selectedPullRequests,
    submitPlanDecision,
    openPlan,
    isDecisionSubmitting,
    statusIdForStage,
    cleanUpSelectedIssue,
    t,
  ]);

  // Footer "Close card": any workflow card can be closed by hand. An older
  // board closes it into its Close column (and a card already there moves on
  // to Clean up); a current board has no Close, so the card goes to Clean up
  // through its gate.
  const closeCardAction = useMemo(() => {
    if (mode !== 'edit' || !selectedIssue || decision?.kind === 'close') {
      return null;
    }
    if (!selectedStage || selectedStage === 'cleanUp') return null;
    const closeStatusId =
      selectedStage === 'close' ? null : statusIdForStage('close');
    const cleanUpStatusId = statusIdForStage('cleanUp');
    if (closeStatusId) {
      return {
        label: t('kanban.closeCard'),
        onClick: () => void moveSelectedIssueTo(closeStatusId),
        disabled: isDecisionSubmitting,
      };
    }
    if (!cleanUpStatusId) return null;
    return {
      label: t('kanban.closeCard'),
      onClick: () => void cleanUpSelectedIssue(cleanUpStatusId),
      disabled: isDecisionSubmitting,
    };
  }, [
    mode,
    selectedIssue,
    decision,
    selectedStage,
    statusIdForStage,
    moveSelectedIssueTo,
    cleanUpSelectedIssue,
    isDecisionSubmitting,
    t,
  ]);

  // Loading state
  const isLoading = projectLoading || orgLoading;
  const isResolvingExpectedIssue =
    mode === 'edit' &&
    selectedKanbanIssueId !== null &&
    issueResolution === 'resolving';
  const hasMissingIssueDataInEditMode =
    mode === 'edit' && selectedKanbanIssueId !== null && selectedIssue === null;

  if (isLoading || isResolvingExpectedIssue || hasMissingIssueDataInEditMode) {
    return (
      <div className="flex items-center justify-center h-full bg-secondary">
        <p className="text-low">{t('states.loading')}</p>
      </div>
    );
  }

  return (
    <KanbanIssuePanel
      mode={mode}
      displayId={displayId}
      formData={displayData}
      assigneeUsers={displayAssigneeUsers}
      onFormChange={handlePropertyChange}
      statuses={sortedStatuses}
      statusLocked={mode === 'create' && lockedCreateStatusId != null}
      tags={tags}
      issueId={selectedKanbanIssueId}
      classChip={mode === 'edit' ? classChip : null}
      priorityLabel={priorityChipLabel(displayData.priority)}
      trackerLink={
        mode === 'edit' && trackerRef?.url
          ? {
              label: t('kanban.panel.openInTracker', { tracker: trackerName }),
              url: trackerRef.url,
            }
          : null
      }
      statusDot={(() => {
        const stage = workflowStageKey(
          statuses.find((status) => status.id === displayData.statusId)?.name
        );
        return stage ? columnDotTone(stage) : undefined;
      })()}
      extraProperties={mode === 'edit' ? extraProperties : []}
      decision={decision}
      closeCardAction={closeCardAction}
      creatorUser={issueCreator}
      parentIssue={parentIssue}
      onParentIssueClick={handleParentIssueClick}
      onRemoveParentIssue={handleRemoveParentIssue}
      linkedPrs={linkedPrs}
      stageAction={
        mode === 'edit' && stagePlaybook
          ? {
              label:
                stagePlaybook.stage === 'planning'
                  ? t('kanban.runPlanningAgent')
                  : t('kanban.runExecutionAgent'),
              onRun: handleRunStageAction,
              disabled: isStartingStageAction,
            }
          : mode === 'edit' &&
              isPushMrStage &&
              implWorkspaceId &&
              linkedPrs.length > 0
            ? {
                label: t('kanban.reviewMrComments', 'Review MR comments'),
                onRun: handleReviewMrComments,
                disabled: isStartingStageAction,
              }
            : undefined
      }
      specStatus={mode === 'edit' ? specStatus : undefined}
      planningVerdict={mode === 'edit' ? planningVerdict : undefined}
      humanTakeover={mode === 'edit' ? humanTakeover : undefined}
      reviewUrl={mode === 'edit' ? (lavishSession?.url ?? null) : null}
      onReopenReview={
        mode === 'edit' &&
        !lavishSession?.url &&
        implWorkspaceId &&
        specStatus?.path &&
        !specStatus.approvedAt
          ? () => void handleReopenReview(specStatus.path!)
          : undefined
      }
      reopeningReview={isReopeningReview}
      approvedPlanUrl={
        mode === 'edit' && selectedKanbanIssueId && specStatus?.approvedAt
          ? `/api/remote/issues/${selectedKanbanIssueId}/spec`
          : null
      }
      onClose={closeKanbanIssuePanel}
      onSubmit={handleSubmit}
      onCmdEnterSubmit={handleCmdEnterSubmit}
      onCreateTag={handleCreateTag}
      renderAddTagControl={({
        tags,
        selectedTagIds,
        onTagToggle,
        onCreateTag,
        disabled,
        trigger,
      }) => (
        <SearchableTagDropdownContainer
          tags={tags}
          selectedTagIds={selectedTagIds}
          onTagToggle={onTagToggle}
          onCreateTag={onCreateTag}
          disabled={disabled}
          contentClassName=""
          trigger={trigger}
        />
      )}
      isSubmitting={isSubmitting}
      descriptionSaveStatus={
        mode === 'edit' ? descriptionSaveStatus : undefined
      }
      titleInputRef={titleInputRef}
      onDeleteDraft={
        mode === 'create' && isCreateDraftDirty ? handleDeleteDraft : undefined
      }
      onCopyLink={mode === 'edit' ? handleCopyLink : undefined}
      onMoreActions={mode === 'edit' ? handleMoreActions : undefined}
      onPasteFiles={onPasteFiles}
      localAttachments={localAttachments}
      dropzoneProps={{ getRootProps, getInputProps, isDragActive }}
      onBrowseAttachment={openFilePicker}
      isUploading={isUploading}
      attachmentError={uploadError}
      onDismissAttachmentError={clearUploadError}
      renderDescriptionEditor={(props) => (
        <WYSIWYGEditor {...props} localAttachments={localAttachments} />
      )}
      renderWorkspacesSection={(issueId) => (
        <IssueWorkspacesSectionContainer issueId={issueId} />
      )}
      renderRelationshipsSection={(issueId) => (
        <IssueRelationshipsSectionContainer issueId={issueId} />
      )}
      renderSubIssuesSection={(issueId) => (
        <IssueSubIssuesSectionContainer issueId={issueId} />
      )}
      renderCommentsSection={(issueId) => (
        <IssueCommentsSectionContainer issueId={issueId} />
      )}
    />
  );
}
