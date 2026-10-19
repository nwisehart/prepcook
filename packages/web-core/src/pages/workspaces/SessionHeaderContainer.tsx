import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  WorkspacesMainHeader,
  type WorkspacesMainBreadcrumb,
} from '@prepcook/ui/components/WorkspacesMain';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { useAllOrganizationProjects } from '@/shared/hooks/useAllOrganizationProjects';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useWorkspaceExecution } from '@/shared/hooks/useWorkspaceExecution';
import { useLavishSession } from '@/shared/hooks/useLavishSession';
import { getLatestConfigFromProcesses } from '@/shared/lib/executor';
import { useSessionCards } from '@/features/workspace/model/hooks/useSessionCards';
import {
  deriveSessionStatus,
  resolvePlanReady,
} from '@/features/workspace/model/sessionStatus';
import { formatModelLabel } from '@/features/workspace/model/modelLabel';
import { SessionHeaderActionsContainer } from './ContextBarContainer';

/**
 * The selected session's header: where it sits (project / card / column),
 * its title, whether it needs you, its branch and model, and its actions.
 */
export function SessionHeaderContainer() {
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const { workspace, activeWorkspaces, archivedWorkspaces } =
    useWorkspaceContext();
  const workspaceId = workspace?.id;

  const sessionCards = useSessionCards();
  const card = workspaceId ? sessionCards.get(workspaceId) : undefined;
  const { data: projects } = useAllOrganizationProjects({ enabled: !!card });
  const projectName = card
    ? projects.find((project) => project.id === card.projectId)?.name
    : undefined;

  const summary = useMemo(
    () =>
      activeWorkspaces.find((ws) => ws.id === workspaceId) ??
      archivedWorkspaces.find((ws) => ws.id === workspaceId),
    [activeWorkspaces, archivedWorkspaces, workspaceId]
  );

  const { processes } = useWorkspaceExecution(workspaceId);
  const modelLabel = useMemo(
    () => formatModelLabel(getLatestConfigFromProcesses(processes)),
    [processes]
  );

  const { data: lavishSession } = useLavishSession(workspaceId);

  const stage = card?.stage ?? null;
  const status = deriveSessionStatus({
    isRunning: summary?.isRunning,
    hasPendingApproval: summary?.hasPendingApproval,
    hasUnseenActivity: summary?.hasUnseenActivity,
    latestProcessStatus: summary?.latestProcessStatus,
    prStatus: summary?.prStatus,
    stage,
    planReady: resolvePlanReady(card?.metadata, stage),
  });
  const needsYou = status.needsYou || Boolean(lavishSession?.url);

  const breadcrumbs = useMemo((): WorkspacesMainBreadcrumb[] => {
    if (!card) {
      return [{ label: t('workspaces.sessionsTitle') }];
    }
    const items: WorkspacesMainBreadcrumb[] = [];
    if (projectName) {
      items.push({
        label: projectName,
        onClick: () => appNavigation.goToProject(card.projectId),
      });
    }
    items.push({
      label: card.key,
      mono: true,
      onClick: () =>
        appNavigation.goToProjectIssue(card.projectId, card.issueId),
    });
    if (card.stageName) {
      items.push({ label: card.stageName });
    }
    return items;
  }, [card, projectName, appNavigation, t]);

  if (!workspace) return null;

  return (
    <WorkspacesMainHeader
      breadcrumbs={breadcrumbs}
      title={workspace.name || workspace.branch}
      needsYouLabel={needsYou ? t('workspaces.groups.needsYou') : undefined}
      branch={workspace.branch}
      modelLabel={modelLabel ?? undefined}
      actions={<SessionHeaderActionsContainer />}
    />
  );
}
