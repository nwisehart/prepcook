import { useCallback, useMemo } from 'react';
import type { DropResult } from '@hello-pangea/dnd';
import {
  AppBar,
  type AppBarBoardStatus,
  type OrchestratorModeName,
} from '@prepcook/ui/components/AppBar';
import type { Project as RemoteProject } from 'shared/remote-types';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useCurrentAppDestination } from '@/shared/hooks/useCurrentAppDestination';
import { useAppUpdateStore } from '@/shared/stores/useAppUpdateStore';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import {
  getProjectDestination,
  isWorkspacesDestination,
} from '@/shared/lib/routes/appNavigation';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';

interface AppSidebarContainerProps {
  projects: RemoteProject[];
  isLoadingProjects: boolean;
  isSavingProjectOrder: boolean;
  onProjectsDragEnd: (result: DropResult) => void;
  onCreateProject: () => void;
  onHoverStart?: () => void;
  onHoverEnd?: () => void;
  /** Runs after any item navigates or opens a dialog (closes the drawer). */
  onNavigate?: () => void;
  /** Shows a close button in the brand row (mobile drawer). */
  onClose?: () => void;
  windowControlsInset?: boolean;
}

/**
 * Feeds the app sidebar: nav targets and active states from the current
 * route, the running-session count, the orchestrator mode from the global
 * config, and the version / board status for the footer.
 */
export function AppSidebarContainer({
  projects,
  isLoadingProjects,
  isSavingProjectOrder,
  onProjectsDragEnd,
  onCreateProject,
  onHoverStart,
  onHoverEnd,
  onNavigate,
  onClose,
  windowControlsInset,
}: AppSidebarContainerProps) {
  const appNavigation = useAppNavigation();
  const currentDestination = useCurrentAppDestination();
  const { hasBoardAccess } = useAuth();
  const { appVersion, boardStatus, config } = useUserSystem();
  const { activeWorkspaces } = useWorkspaceContext();
  const updateVersion = useAppUpdateStore((s) => s.updateVersion);
  const restartForUpdate = useAppUpdateStore((s) => s.restart);
  const lastSelectedProjectId = useUiPreferencesStore(
    (s) => s.selectedProjectId
  );

  const projectDestination = getProjectDestination(currentDestination);
  const activeProjectId = projectDestination?.projectId ?? null;
  const isBoardActive =
    projectDestination !== null &&
    projectDestination.kind !== 'project-knowledge';
  const isKnowledgeActive = projectDestination?.kind === 'project-knowledge';
  const isWorkspacesActive = isWorkspacesDestination(currentDestination);
  const isExportActive = currentDestination?.kind === 'export';

  // Board and Knowledge open the project on screen, else the last one
  // visited, else the first in the user's order.
  const currentProjectId = useMemo(() => {
    if (activeProjectId) return activeProjectId;
    if (
      lastSelectedProjectId &&
      projects.some((project) => project.id === lastSelectedProjectId)
    ) {
      return lastSelectedProjectId;
    }
    return projects[0]?.id ?? null;
  }, [activeProjectId, lastSelectedProjectId, projects]);

  const runningSessionsCount = useMemo(
    () => activeWorkspaces.filter((workspace) => workspace.isRunning).length,
    [activeWorkspaces]
  );

  // Chef mode is autonomy off (also the default before the orchestrator is
  // configured); Prep mode is autonomy on. Unknown until the config loads.
  const mode: OrchestratorModeName | null = config
    ? config.orchestrator?.autonomy === 'full'
      ? 'prep'
      : 'chef'
    : null;

  const sidebarBoardStatus: AppBarBoardStatus | null = boardStatus
    ? boardStatus.status === 'connected'
      ? 'up'
      : 'down'
    : null;

  const handleProjectClick = useCallback(
    (projectId: string) => {
      appNavigation.goToProject(projectId);
      onNavigate?.();
    },
    [appNavigation, onNavigate]
  );

  const handleBoardClick = useCallback(() => {
    if (!currentProjectId) return;
    appNavigation.goToProject(currentProjectId);
    onNavigate?.();
  }, [appNavigation, onNavigate, currentProjectId]);

  const handleWorkspacesClick = useCallback(() => {
    appNavigation.goToWorkspaces();
    onNavigate?.();
  }, [appNavigation, onNavigate]);

  const handleKnowledgeClick = useCallback(() => {
    if (!currentProjectId) return;
    appNavigation.goToProjectKnowledge(currentProjectId);
    onNavigate?.();
  }, [appNavigation, onNavigate, currentProjectId]);

  const handleExportClick = useCallback(() => {
    appNavigation.goToExport();
    onNavigate?.();
  }, [appNavigation, onNavigate]);

  const handleCreateProject = useCallback(() => {
    onNavigate?.();
    onCreateProject();
  }, [onNavigate, onCreateProject]);

  const handleSettingsClick = useCallback(() => {
    onNavigate?.();
    void SettingsDialog.show();
  }, [onNavigate]);

  const handleOpenBoardSettings = useCallback(() => {
    onNavigate?.();
    void SettingsDialog.show({ initialSection: 'board' });
  }, [onNavigate]);

  // The sidebar never flips autonomy itself: the Orchestrator settings
  // section explains what the mode does and saves it with the rest of the
  // orchestrator config.
  const handleSwitchMode = useCallback(() => {
    onNavigate?.();
    void SettingsDialog.show({ initialSection: 'orchestrator' });
  }, [onNavigate]);

  const canOpenProject = hasBoardAccess && currentProjectId !== null;

  return (
    <AppBar
      projects={projects}
      currentProjectId={currentProjectId}
      onProjectClick={handleProjectClick}
      onProjectsDragEnd={onProjectsDragEnd}
      isSavingProjectOrder={isSavingProjectOrder}
      onCreateProject={handleCreateProject}
      hasBoardAccess={hasBoardAccess}
      isLoadingProjects={isLoadingProjects}
      onOpenBoardSettings={handleOpenBoardSettings}
      onExportClick={handleExportClick}
      isExportActive={isExportActive}
      onBoardClick={canOpenProject ? handleBoardClick : undefined}
      isBoardActive={isBoardActive}
      onWorkspacesClick={handleWorkspacesClick}
      isWorkspacesActive={isWorkspacesActive}
      runningSessionsCount={runningSessionsCount}
      onKnowledgeClick={canOpenProject ? handleKnowledgeClick : undefined}
      isKnowledgeActive={isKnowledgeActive}
      onSettingsClick={handleSettingsClick}
      mode={mode}
      onSwitchMode={handleSwitchMode}
      appVersion={appVersion}
      updateVersion={updateVersion}
      onUpdateClick={restartForUpdate ?? undefined}
      boardStatus={sidebarBoardStatus}
      onHoverStart={onHoverStart}
      onHoverEnd={onHoverEnd}
      onClose={onClose}
      windowControlsInset={windowControlsInset}
    />
  );
}
