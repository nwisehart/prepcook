import { useMemo, useCallback } from 'react';
import { useParams } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';
import { LinkIcon, PlusIcon } from '@phosphor-icons/react';
import { useProjectContext } from '@/shared/hooks/useProjectContext';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useOrgContext } from '@/shared/hooks/useOrgContext';
import { useUserContext } from '@/shared/hooks/useUserContext';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useProjectWorkspaceCreateDraft } from '@/shared/hooks/useProjectWorkspaceCreateDraft';
import { workspacesApi } from '@/shared/lib/api';
import { getWorkspaceDefaults } from '@/shared/lib/workspaceDefaults';
import {
  buildLinkedIssueCreateState,
  buildLocalWorkspaceIdSet,
  buildWorkspaceCreateInitialState,
  buildWorkspaceCreatePrompt,
} from '@/shared/lib/workspaceCreateState';
import { ConfirmDialog } from '@prepcook/ui/components/ConfirmDialog';
import { DeleteWorkspaceDialog } from '@prepcook/ui/components/DeleteWorkspaceDialog';
import type { WorkspaceWithStats } from '@prepcook/ui/components/IssueWorkspaceCard';
import { IssueWorkspacesSection } from '@prepcook/ui/components/IssueWorkspacesSection';
import type { SectionAction } from '@prepcook/ui/components/CollapsibleSectionHeader';
import {
  isReviewSession,
  workflowStageKey,
} from '@/features/kanban/model/cardSignals';

interface IssueWorkspacesSectionContainerProps {
  issueId: string;
}

/**
 * Container component for the workspaces section.
 * Fetches workspace data from ProjectContext and transforms it for display.
 */
export function IssueWorkspacesSectionContainer({
  issueId,
}: IssueWorkspacesSectionContainerProps) {
  const { t } = useTranslation('common');
  const { projectId } = useParams({ strict: false });
  const appNavigation = useAppNavigation();
  const { openWorkspaceCreateFromState } = useProjectWorkspaceCreateDraft();
  const { userId } = useAuth();
  const { workspaces } = useUserContext();

  const {
    pullRequests,
    getIssue,
    getWorkspacesForIssue,
    issues,
    statuses,
    isLoading: projectLoading,
  } = useProjectContext();
  const { activeWorkspaces, archivedWorkspaces } = useWorkspaceContext();
  const { membersWithProfilesById, isLoading: orgLoading } = useOrgContext();

  const localWorkspacesById = useMemo(() => {
    const map = new Map<string, (typeof activeWorkspaces)[number]>();

    for (const workspace of activeWorkspaces) {
      map.set(workspace.id, workspace);
    }

    for (const workspace of archivedWorkspaces) {
      map.set(workspace.id, workspace);
    }

    return map;
  }, [activeWorkspaces, archivedWorkspaces]);

  // The stage chip on each session row. Independent reviews run in their
  // own "Review: …" sessions; planning and execution share the card's
  // implementation session, which is planning while the card is in Plan
  // and executing (with its fix rounds) from Execute on.
  const stageLabels = useMemo(() => {
    const nameOf = (stage: string) =>
      statuses.find((status) => workflowStageKey(status.name) === stage)
        ?.name ?? null;
    const issueStatusName = statuses.find(
      (status) => status.id === getIssue(issueId)?.status_id
    )?.name;
    const cardStage = workflowStageKey(issueStatusName);
    return {
      review: nameOf('review'),
      implementation:
        cardStage === 'plan'
          ? nameOf('plan')
          : cardStage && cardStage !== 'todo'
            ? nameOf('execute')
            : null,
    };
  }, [statuses, getIssue, issueId]);

  // Get workspaces for the issue, with PR info, newest first
  const workspacesWithStats: WorkspaceWithStats[] = useMemo(() => {
    const rawWorkspaces = [...getWorkspacesForIssue(issueId)].sort(
      (a, b) =>
        new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
    );

    return rawWorkspaces.map((workspace) => {
      const localWorkspace = workspace.local_workspace_id
        ? localWorkspacesById.get(workspace.local_workspace_id)
        : undefined;

      // Find all linked PRs for this workspace
      const linkedPrs = pullRequests
        .filter((pr) => pr.workspace_id === workspace.id)
        .map((pr) => ({
          number: pr.number,
          url: pr.url,
          status: pr.status as 'open' | 'merged' | 'closed',
        }));

      // Get owner
      const owner =
        membersWithProfilesById.get(workspace.owner_user_id) ?? null;

      return {
        id: workspace.id,
        localWorkspaceId: workspace.local_workspace_id,
        name: workspace.name,
        archived: workspace.archived,
        filesChanged: workspace.files_changed ?? 0,
        linesAdded: workspace.lines_added ?? 0,
        linesRemoved: workspace.lines_removed ?? 0,
        prs: linkedPrs,
        owner,
        updatedAt: workspace.updated_at,
        isOwnedByCurrentUser: workspace.owner_user_id === userId,
        isRunning: localWorkspace?.isRunning,
        hasPendingApproval: localWorkspace?.hasPendingApproval,
        hasRunningDevServer: localWorkspace?.hasRunningDevServer,
        hasUnseenActivity: localWorkspace?.hasUnseenActivity,
        latestProcessCompletedAt: localWorkspace?.latestProcessCompletedAt,
        latestProcessStatus: localWorkspace?.latestProcessStatus,
        stageLabel: isReviewSession(workspace)
          ? stageLabels.review
          : stageLabels.implementation,
      };
    });
  }, [
    issueId,
    getWorkspacesForIssue,
    pullRequests,
    membersWithProfilesById,
    userId,
    localWorkspacesById,
    stageLabels,
  ]);

  const isLoading = projectLoading || orgLoading;
  const shouldAnimateCreateButton = useMemo(() => {
    if (issues.length !== 1) {
      return false;
    }

    return issues.every(
      (issue) => getWorkspacesForIssue(issue.id).length === 0
    );
  }, [issues, getWorkspacesForIssue]);

  // Handle clicking '+' to create and link a new workspace directly
  const handleAddWorkspace = useCallback(async () => {
    if (!projectId) {
      return;
    }

    const issue = getIssue(issueId);
    const initialPrompt = buildWorkspaceCreatePrompt(
      issue?.title ?? null,
      issue?.description ?? null
    );
    const localWorkspaceIds = buildLocalWorkspaceIdSet(
      activeWorkspaces,
      archivedWorkspaces
    );

    const defaults = await getWorkspaceDefaults(
      workspaces,
      localWorkspaceIds,
      projectId
    );
    const createState = buildWorkspaceCreateInitialState({
      prompt: initialPrompt,
      defaults,
      linkedIssue: buildLinkedIssueCreateState(issue, projectId),
    });

    const draftId = await openWorkspaceCreateFromState(createState, {
      issueId,
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
  }, [
    projectId,
    openWorkspaceCreateFromState,
    getIssue,
    issueId,
    activeWorkspaces,
    archivedWorkspaces,
    workspaces,
    t,
  ]);

  // Handle clicking link action to link an existing workspace
  const handleLinkWorkspace = useCallback(async () => {
    if (!projectId) {
      return;
    }

    const { WorkspaceSelectionDialog } = await import(
      '@/shared/dialogs/command-bar/WorkspaceSelectionDialog'
    );
    await WorkspaceSelectionDialog.show({ projectId, issueId });
  }, [projectId, issueId]);

  // Handle clicking a workspace card to open it
  const handleWorkspaceClick = useCallback(
    (localWorkspaceId: string | null) => {
      if (projectId && localWorkspaceId) {
        appNavigation.goToProjectIssueWorkspace(
          projectId,
          issueId,
          localWorkspaceId
        );
      }
    },
    [projectId, issueId, appNavigation]
  );

  // Handle unlinking a workspace from the issue
  const handleUnlinkWorkspace = useCallback(
    async (localWorkspaceId: string) => {
      const result = await ConfirmDialog.show({
        title: t('workspaces.unlinkFromIssue'),
        message: t('workspaces.unlinkConfirmMessage'),
        confirmText: t('workspaces.unlink'),
        variant: 'destructive',
      });

      if (result === 'confirmed') {
        try {
          await workspacesApi.unlinkFromIssue(localWorkspaceId);
        } catch (error) {
          ConfirmDialog.show({
            title: t('common:error'),
            message:
              error instanceof Error
                ? error.message
                : t('workspaces.unlinkError'),
            confirmText: t('common:ok'),
            showCancelButton: false,
          });
        }
      }
    },
    [t]
  );

  // Handle deleting a workspace (unlinks first, then deletes local)
  const handleDeleteWorkspace = useCallback(
    async (localWorkspaceId: string) => {
      const localWorkspace = localWorkspacesById.get(localWorkspaceId);
      if (!localWorkspace) {
        ConfirmDialog.show({
          title: t('common:error'),
          message: t('workspaces.deleteError'),
          confirmText: t('common:ok'),
          showCancelButton: false,
        });
        return;
      }

      const result = await DeleteWorkspaceDialog.show({
        branchName: localWorkspace.branch,
        hasOpenPR:
          workspacesWithStats
            .find(
              (workspace) => workspace.localWorkspaceId === localWorkspaceId
            )
            ?.prs.some((pr) => pr.status === 'open') ?? false,
        isLinkedToIssue: true,
        linkedIssueSimpleId: getIssue(issueId)?.simple_id,
      });

      if (result.action !== 'confirmed') {
        return;
      }

      try {
        // Delete local workspace first
        await workspacesApi.delete(localWorkspaceId, result.deleteBranches);
        // Unlink from remote after successful deletion
        if (result.unlinkFromIssue) {
          await workspacesApi.unlinkFromIssue(localWorkspaceId);
        }
      } catch (error) {
        ConfirmDialog.show({
          title: t('common:error'),
          message:
            error instanceof Error
              ? error.message
              : t('workspaces.deleteError'),
          confirmText: t('common:ok'),
          showCancelButton: false,
        });
      }
    },
    [localWorkspacesById, workspacesWithStats, t, issueId, getIssue]
  );

  // Actions for the section header
  const actions: SectionAction[] = useMemo(
    () => [
      {
        icon: PlusIcon,
        onClick: handleAddWorkspace,
      },
      {
        icon: LinkIcon,
        onClick: handleLinkWorkspace,
      },
    ],
    [handleAddWorkspace, handleLinkWorkspace]
  );

  return (
    <IssueWorkspacesSection
      workspaces={workspacesWithStats}
      isLoading={isLoading}
      actions={actions}
      onWorkspaceClick={handleWorkspaceClick}
      onCreateWorkspace={handleAddWorkspace}
      onUnlinkWorkspace={handleUnlinkWorkspace}
      onDeleteWorkspace={handleDeleteWorkspace}
      shouldAnimateCreateButton={shouldAnimateCreateButton}
    />
  );
}
