import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { DropResult } from '@hello-pangea/dnd';
import { Outlet, useNavigate } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';
import {
  XIcon,
  PlusIcon,
  LayoutIcon,
  KanbanIcon,
  DownloadSimpleIcon,
} from '@phosphor-icons/react';
import { SyncErrorProvider } from '@/shared/providers/SyncErrorProvider';
import { useIsMobile } from '@/shared/hooks/useIsMobile';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import { cn } from '@/shared/lib/utils';
import { isTauriMac } from '@/shared/lib/platform';

import { NavbarContainer } from './NavbarContainer';
import { AppBar } from '@prepcook/ui/components/AppBar';
import { MobileDrawer } from '@prepcook/ui/components/MobileDrawer';
import { AppBarUserPopoverContainer } from './AppBarUserPopoverContainer';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useAppUpdateStore } from '@/shared/stores/useAppUpdateStore';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';
import { useCurrentAppDestination } from '@/shared/hooks/useCurrentAppDestination';
import {
  getProjectDestination,
  isWorkspacesDestination,
} from '@/shared/lib/routes/appNavigation';
import {
  CreateRemoteProjectDialog,
  type CreateRemoteProjectResult,
} from '@/shared/dialogs/org/CreateRemoteProjectDialog';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';
import { CommandBarDialog } from '@/shared/dialogs/command-bar/CommandBarDialog';
import { useCommandBarShortcut } from '@/shared/hooks/useCommandBarShortcut';
import { useWorkspaceSidebarPreviewController } from '@/shared/hooks/useWorkspaceSidebarPreviewController';
import { useShape } from '@/shared/integrations/electric/hooks';
import { sortProjectsByOrder } from '@/shared/lib/projectOrder';
import {
  PROJECT_MUTATION,
  PROJECTS_SHAPE,
  type Project as RemoteProject,
} from 'shared/remote-types';
import { WorkspacesSidebarContainer } from '@/pages/workspaces/WorkspacesSidebarContainer';
import { WorkspacesSidebarReopenTag } from '@prepcook/ui/components/WorkspacesSidebar';
import { BoardNotice } from '@/shared/components/BoardNotice';
import { LaunchPausedBanner } from '@/shared/components/LaunchPausedBanner';
import { SeatDegradedBanner } from '@/shared/components/SeatDegradedBanner';

export function SharedAppLayout() {
  const { t } = useTranslation('common');
  const appNavigation = useAppNavigation();
  const currentDestination = useCurrentAppDestination();
  const isMobile = useIsMobile();
  const mobileFontScale = useUiPreferencesStore((s) => s.mobileFontScale);
  const isLeftSidebarVisible = useUiPreferencesStore(
    (s) => s.isLeftSidebarVisible
  );
  const { hasBoardAccess, isLoaded: isAuthLoaded } = useAuth();
  const { appVersion, boardStatus, claudeSeatDegraded, launchPauseMinutes } =
    useUserSystem();
  const boardNoticeReason =
    isAuthLoaded && !hasBoardAccess
      ? boardStatus?.status === 'no_board'
        ? 'no_board'
        : 'no_secret'
      : null;
  const updateVersion = useAppUpdateStore((s) => s.updateVersion);
  const restartForUpdate = useAppUpdateStore((s) => s.restart);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [isAppBarHovered, setIsAppBarHovered] = useState(false);
  const navigate = useNavigate();

  // Register CMD+K shortcut globally for all routes under SharedAppLayout
  useCommandBarShortcut(() => CommandBarDialog.show());

  // Apply mobile font scale CSS variable
  useEffect(() => {
    if (!isMobile) {
      document.documentElement.style.removeProperty('--mobile-font-scale');
      return;
    }
    const scaleMap = { default: '1', small: '0.9', smaller: '0.8' } as const;
    document.documentElement.style.setProperty(
      '--mobile-font-scale',
      scaleMap[mobileFontScale]
    );
    return () => {
      document.documentElement.style.removeProperty('--mobile-font-scale');
    };
  }, [isMobile, mobileFontScale]);

  // AppBar state - organizations and projects
  const { data: orgsData } = useUserOrganizations();
  const organizations = useMemo(
    () => orgsData?.organizations ?? [],
    [orgsData?.organizations]
  );

  const selectedOrgId = useOrganizationStore((s) => s.selectedOrgId);
  const setSelectedOrgId = useOrganizationStore((s) => s.setSelectedOrgId);
  const prevOrgIdRef = useRef<string | null>(null);

  // Auto-select first org if none selected or selection is invalid
  useEffect(() => {
    if (organizations.length === 0) return;

    const hasValidSelection = selectedOrgId
      ? organizations.some((org) => org.id === selectedOrgId)
      : false;

    if (!selectedOrgId || !hasValidSelection) {
      setSelectedOrgId(organizations[0].id);
    }
  }, [organizations, selectedOrgId, setSelectedOrgId]);

  const projectParams = useMemo(
    () => ({ organization_id: selectedOrgId || '' }),
    [selectedOrgId]
  );
  const {
    data: orgProjects = [],
    isLoading,
    updateMany: updateManyProjects,
  } = useShape(PROJECTS_SHAPE, projectParams, {
    enabled: hasBoardAccess && !!selectedOrgId,
    mutation: PROJECT_MUTATION,
  });
  const sortedProjects = useMemo(
    () => sortProjectsByOrder(orgProjects),
    [orgProjects]
  );
  const [orderedProjects, setOrderedProjects] =
    useState<RemoteProject[]>(sortedProjects);
  const [isSavingProjectOrder, setIsSavingProjectOrder] = useState(false);

  useEffect(() => {
    if (isSavingProjectOrder) {
      return;
    }
    setOrderedProjects(sortedProjects);
  }, [isSavingProjectOrder, sortedProjects]);

  // Navigate to the first ordered project when org changes
  useEffect(() => {
    if (
      prevOrgIdRef.current !== null &&
      prevOrgIdRef.current !== selectedOrgId &&
      selectedOrgId &&
      !isLoading
    ) {
      if (sortedProjects.length > 0) {
        appNavigation.goToProject(sortedProjects[0].id);
      } else {
        appNavigation.goToWorkspaces();
      }
      prevOrgIdRef.current = selectedOrgId;
    } else if (prevOrgIdRef.current === null && selectedOrgId) {
      prevOrgIdRef.current = selectedOrgId;
    }
  }, [selectedOrgId, sortedProjects, isLoading, appNavigation]);

  // Navigation state for AppBar active indicators
  const projectDestination = useMemo(
    () => getProjectDestination(currentDestination),
    [currentDestination]
  );
  const isWorkspacesActive = isWorkspacesDestination(currentDestination);
  const isExportActive = currentDestination?.kind === 'export';
  // Any full-width strip above the navbar occupies one extra grid row,
  // regardless of how many banners stack inside it.
  const showTopBanner = claudeSeatDegraded || launchPauseMinutes !== null;
  const isWorkspaceSidebarPreviewEnabled =
    !isMobile && isWorkspacesActive && !isLeftSidebarVisible;
  const activeProjectId = projectDestination?.projectId ?? null;
  const sidebarPreview = useWorkspaceSidebarPreviewController({
    enabled: isWorkspaceSidebarPreviewEnabled,
    isAppBarHovered,
  });

  // Persist last selected project to scratch store
  const setSelectedProjectId = useUiPreferencesStore(
    (s) => s.setSelectedProjectId
  );
  useEffect(() => {
    if (activeProjectId) {
      setSelectedProjectId(activeProjectId);
    }
  }, [activeProjectId, setSelectedProjectId]);

  const handleWorkspacesClick = useCallback(() => {
    void navigate({ to: '/workspaces' });
  }, [navigate]);

  const handleExportClick = useCallback(() => {
    appNavigation.goToExport();
  }, [appNavigation]);

  // Knowledge targets the active project, falling back to the first project.
  const knowledgeProjectId = activeProjectId ?? sortedProjects[0]?.id ?? null;
  const isKnowledgeActive = currentDestination?.kind === 'project-knowledge';
  const handleKnowledgeClick = useCallback(() => {
    if (knowledgeProjectId) {
      appNavigation.goToProjectKnowledge(knowledgeProjectId);
    }
  }, [appNavigation, knowledgeProjectId]);

  const handleProjectClick = useCallback(
    (projectId: string) => {
      appNavigation.goToProject(projectId);
    },
    [appNavigation]
  );

  const handleProjectsDragEnd = useCallback(
    async ({ source, destination }: DropResult) => {
      if (isSavingProjectOrder) {
        return;
      }
      if (!destination || source.index === destination.index) {
        return;
      }

      const previousOrder = orderedProjects;
      const reordered = [...orderedProjects];
      const [moved] = reordered.splice(source.index, 1);

      if (!moved) {
        return;
      }

      reordered.splice(destination.index, 0, moved);
      setOrderedProjects(reordered);
      setIsSavingProjectOrder(true);

      try {
        await updateManyProjects(
          reordered.map((project, index) => ({
            id: project.id,
            changes: { sort_order: index },
          }))
        ).persisted;
      } catch (error) {
        console.error('Failed to reorder projects:', error);
        setOrderedProjects(previousOrder);
      } finally {
        setIsSavingProjectOrder(false);
      }
    },
    [isSavingProjectOrder, orderedProjects, updateManyProjects]
  );

  const handleCreateProject = useCallback(async () => {
    if (!selectedOrgId) return;

    try {
      const result: CreateRemoteProjectResult =
        await CreateRemoteProjectDialog.show({ organizationId: selectedOrgId });

      if (result.action === 'created' && result.project) {
        appNavigation.goToProject(result.project.id);
      }
    } catch {
      // Dialog cancelled
    }
  }, [selectedOrgId, appNavigation]);

  const handleOpenBoardSettings = useCallback(async () => {
    await SettingsDialog.show({ initialSection: 'board' });
  }, []);

  return (
    <SyncErrorProvider>
      <div
        className={cn(
          'bg-primary',
          isMobile
            ? 'flex fixed inset-0 pb-[env(safe-area-inset-bottom)]'
            : cn(
                'grid grid-cols-[auto_1fr] h-screen',
                showTopBanner
                  ? 'grid-rows-[auto_auto_1fr]'
                  : 'grid-rows-[auto_1fr]'
              )
        )}
      >
        {!isMobile && (
          <>
            {showTopBanner && (
              <div className="col-span-2">
                {claudeSeatDegraded && <SeatDegradedBanner />}
                {launchPauseMinutes !== null && (
                  <LaunchPausedBanner minutes={launchPauseMinutes} />
                )}
              </div>
            )}
            {/* Desktop corner spacer. */}
            <div
              data-tauri-drag-region
              className="bg-secondary"
              style={isTauriMac() ? { minWidth: 56 } : undefined}
            />
            {/* Desktop navbar. */}
            <NavbarContainer onOpenDrawer={() => setIsDrawerOpen(true)} />
            {/* Desktop AppBar sidebar. */}
            <AppBar
              projects={orderedProjects}
              onCreateProject={handleCreateProject}
              onExportClick={handleExportClick}
              onWorkspacesClick={handleWorkspacesClick}
              onProjectClick={handleProjectClick}
              onProjectsDragEnd={handleProjectsDragEnd}
              isSavingProjectOrder={isSavingProjectOrder}
              isWorkspacesActive={isWorkspacesActive}
              isExportActive={isExportActive}
              onKnowledgeClick={
                knowledgeProjectId ? handleKnowledgeClick : undefined
              }
              isKnowledgeActive={isKnowledgeActive}
              activeProjectId={activeProjectId}
              hasBoardAccess={hasBoardAccess}
              isLoadingProjects={isLoading}
              onOpenBoardSettings={handleOpenBoardSettings}
              onHoverStart={() => setIsAppBarHovered(true)}
              onHoverEnd={() => setIsAppBarHovered(false)}
              userPopover={<AppBarUserPopoverContainer />}
              appVersion={appVersion}
              updateVersion={updateVersion}
              onUpdateClick={restartForUpdate ?? undefined}
            />
            {/* Desktop content. */}
            <div className="relative min-h-0 overflow-hidden">
              {isWorkspaceSidebarPreviewEnabled && (
                <div className="absolute inset-y-0 left-0 z-20 flex items-center">
                  <WorkspacesSidebarReopenTag
                    active={sidebarPreview.isPreviewOpen}
                    onHoverStart={sidebarPreview.handleHandleHoverStart}
                    onHoverEnd={sidebarPreview.handleHandleHoverEnd}
                    ariaLabel="Workspaces"
                  />
                </div>
              )}

              {isWorkspaceSidebarPreviewEnabled && (
                <div
                  className={cn(
                    'absolute left-0 top-0 z-30 h-full w-[300px] transition-transform duration-150 ease-out',
                    sidebarPreview.isPreviewOpen
                      ? 'translate-x-0 pointer-events-auto'
                      : '-translate-x-full pointer-events-none'
                  )}
                  onMouseEnter={sidebarPreview.handlePreviewHoverStart}
                  onMouseLeave={sidebarPreview.handlePreviewHoverEnd}
                >
                  <div className="h-full w-full overflow-hidden border-r border-border bg-secondary shadow-lg">
                    <WorkspacesSidebarContainer />
                  </div>
                </div>
              )}

              <div className="flex h-full min-h-0 flex-col">
                {boardNoticeReason && (
                  <BoardNotice reason={boardNoticeReason} />
                )}
                <div className="min-h-0 flex-1">
                  <Outlet />
                </div>
              </div>
            </div>
          </>
        )}

        {isMobile && (
          <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
            {claudeSeatDegraded && <SeatDegradedBanner />}
            {launchPauseMinutes !== null && (
              <LaunchPausedBanner minutes={launchPauseMinutes} />
            )}
            <NavbarContainer
              mobileMode={isMobile}
              onOpenDrawer={() => setIsDrawerOpen(true)}
            />
            {boardNoticeReason && <BoardNotice reason={boardNoticeReason} />}
            <div className="flex-1 min-h-0 overflow-hidden">
              <Outlet />
            </div>
          </div>
        )}

        {/* Mobile project navigation drawer */}
        <MobileDrawer
          open={isDrawerOpen && isMobile}
          onClose={() => setIsDrawerOpen(false)}
        >
          <div className="flex flex-col h-full">
            {/* Header: close button */}
            <div className="flex items-center justify-end p-4 border-b border-border">
              <button
                type="button"
                onClick={() => setIsDrawerOpen(false)}
                className="p-1 rounded-sm text-low hover:text-normal cursor-pointer"
              >
                <XIcon className="h-4 w-4" weight="bold" />
              </button>
            </div>

            {/* Workspaces link */}
            <button
              type="button"
              onClick={() => {
                void navigate({ to: '/workspaces' });
                setIsDrawerOpen(false);
              }}
              className="flex items-center gap-2 px-4 py-3 text-sm text-normal hover:bg-secondary cursor-pointer"
            >
              <LayoutIcon className="h-4 w-4" />
              Workspaces
            </button>

            {/* Divider */}
            <div className="border-t border-border mx-4" />

            {/* Export link */}
            {hasBoardAccess && (
              <div className="px-4 py-3">
                <p className="mb-2 text-xs font-medium text-low">Export</p>
                <button
                  type="button"
                  onClick={() => {
                    handleExportClick();
                    setIsDrawerOpen(false);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-3 py-2.5 text-sm text-normal hover:bg-secondary cursor-pointer"
                >
                  <DownloadSimpleIcon className="h-4 w-4" />
                  Export data
                </button>
              </div>
            )}

            {/* Divider */}
            {hasBoardAccess && <div className="border-t border-border mx-4" />}

            {/* Project list */}
            <div className="flex-1 overflow-y-auto p-2">
              {hasBoardAccess ? (
                orderedProjects.map((project) => (
                  <button
                    type="button"
                    key={project.id}
                    onClick={() => {
                      handleProjectClick(project.id);
                      setIsDrawerOpen(false);
                    }}
                    className={cn(
                      'flex items-center gap-3 w-full px-3 py-2.5 rounded-md text-sm text-left cursor-pointer',
                      'transition-colors',
                      project.id === activeProjectId
                        ? 'bg-brand/10 text-high'
                        : 'text-normal hover:bg-secondary'
                    )}
                  >
                    <span
                      className="h-2.5 w-2.5 rounded-full shrink-0"
                      style={{ backgroundColor: `hsl(${project.color})` }}
                    />
                    <span className="truncate">{project.name}</span>
                  </button>
                ))
              ) : (
                <div className="px-4 py-6 text-center">
                  <KanbanIcon
                    className="h-8 w-8 mx-auto text-low"
                    weight="bold"
                  />
                  <p className="mt-3 text-sm font-medium text-high">
                    {t('appBar.kanban.title')}
                  </p>
                  <p className="mt-1 text-xs text-low">
                    {t('appBar.kanban.description')}
                  </p>
                  <div className="mt-4">
                    <button
                      type="button"
                      onClick={() => {
                        handleOpenBoardSettings();
                        setIsDrawerOpen(false);
                      }}
                      className="w-full px-3 py-2 rounded-md text-sm font-medium bg-brand text-on-brand hover:bg-brand-hover cursor-pointer"
                    >
                      {t('appBar.kanban.action')}
                    </button>
                  </div>
                </div>
              )}
            </div>

            {/* Create Project button */}
            {hasBoardAccess && (
              <div className="p-3 border-t border-border">
                <button
                  type="button"
                  onClick={() => {
                    handleCreateProject();
                    setIsDrawerOpen(false);
                  }}
                  className="flex items-center gap-2 w-full px-3 py-2.5 rounded-md text-sm text-low hover:text-normal hover:bg-secondary cursor-pointer"
                >
                  <PlusIcon className="h-4 w-4" />
                  Create Project
                </button>
              </div>
            )}
          </div>
        </MobileDrawer>
      </div>
    </SyncErrorProvider>
  );
}
