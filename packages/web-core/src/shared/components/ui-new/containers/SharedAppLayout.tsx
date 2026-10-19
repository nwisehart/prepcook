import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { DropResult } from '@hello-pangea/dnd';
import { Outlet } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';
import { SyncErrorProvider } from '@/shared/providers/SyncErrorProvider';
import { useIsMobile } from '@/shared/hooks/useIsMobile';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import { cn } from '@/shared/lib/utils';
import { isTauriMac } from '@/shared/lib/platform';

import { NavbarContainer } from './NavbarContainer';
import { AppSidebarContainer } from './AppSidebarContainer';
import { MobileDrawer } from '@prepcook/ui/components/MobileDrawer';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
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
  const { boardStatus, claudeSeatDegraded, launchPauseMinutes } =
    useUserSystem();
  const boardNoticeReason =
    isAuthLoaded && !hasBoardAccess
      ? boardStatus?.status === 'no_board'
        ? 'no_board'
        : 'no_secret'
      : null;
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [isAppBarHovered, setIsAppBarHovered] = useState(false);
  const openDrawer = useCallback(() => setIsDrawerOpen(true), []);
  const closeDrawer = useCallback(() => setIsDrawerOpen(false), []);

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

  // Sidebar state - organizations and projects
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

  const projectDestination = useMemo(
    () => getProjectDestination(currentDestination),
    [currentDestination]
  );
  const isWorkspacesActive = isWorkspacesDestination(currentDestination);
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

  const sidebarProps = {
    projects: orderedProjects,
    isLoadingProjects: isLoading,
    isSavingProjectOrder,
    onProjectsDragEnd: handleProjectsDragEnd,
    onCreateProject: handleCreateProject,
  };

  // Seat, launch-pause and board notices sit at the top of the content
  // column, above the page, in the order of how much they block.
  const hasNotices =
    claudeSeatDegraded ||
    launchPauseMinutes !== null ||
    boardNoticeReason !== null;
  const notices = hasNotices ? (
    <div
      className={cn(
        'flex shrink-0 flex-col gap-2',
        isMobile ? 'px-3 pt-3' : 'px-7 pt-4'
      )}
    >
      {claudeSeatDegraded && <SeatDegradedBanner />}
      {launchPauseMinutes !== null && (
        <LaunchPausedBanner minutes={launchPauseMinutes} />
      )}
      {boardNoticeReason && <BoardNotice reason={boardNoticeReason} />}
    </div>
  ) : null;

  return (
    <SyncErrorProvider>
      <div
        className={cn(
          'bg-primary text-normal',
          isMobile
            ? 'flex fixed inset-0 pb-[env(safe-area-inset-bottom)]'
            : 'grid h-screen grid-cols-[232px_minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)]'
        )}
      >
        {!isMobile && (
          <>
            {/* Desktop sidebar, full height beside the navbar and content. */}
            <div className="row-span-2 min-h-0">
              <AppSidebarContainer
                {...sidebarProps}
                onHoverStart={() => setIsAppBarHovered(true)}
                onHoverEnd={() => setIsAppBarHovered(false)}
                windowControlsInset={isTauriMac()}
              />
            </div>
            {/* Desktop navbar; it is also the window drag region. */}
            <NavbarContainer onOpenDrawer={openDrawer} />
            {/* Desktop content. */}
            <div className="relative min-h-0 overflow-hidden">
              {isWorkspaceSidebarPreviewEnabled && (
                <div className="absolute inset-y-0 left-0 z-20 flex items-center">
                  <WorkspacesSidebarReopenTag
                    active={sidebarPreview.isPreviewOpen}
                    onHoverStart={sidebarPreview.handleHandleHoverStart}
                    onHoverEnd={sidebarPreview.handleHandleHoverEnd}
                    ariaLabel={t('appBar.nav.sessions')}
                    className="ml-1 focus-visible:ring-offset-primary"
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
                  <div className="h-full w-full overflow-hidden border-r border-border bg-secondary shadow-overlay">
                    <WorkspacesSidebarContainer />
                  </div>
                </div>
              )}

              <div className="flex h-full min-h-0 flex-col">
                {notices}
                <div className="min-h-0 flex-1">
                  <Outlet />
                </div>
              </div>
            </div>
          </>
        )}

        {isMobile && (
          <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
            <NavbarContainer mobileMode={isMobile} onOpenDrawer={openDrawer} />
            {notices}
            <div className="flex-1 min-h-0 overflow-hidden">
              <Outlet />
            </div>
          </div>
        )}

        {/* Mobile navigation drawer: the same sidebar as desktop. */}
        {isMobile && (
          <MobileDrawer open={isDrawerOpen} onClose={closeDrawer}>
            <AppSidebarContainer
              {...sidebarProps}
              onNavigate={closeDrawer}
              onClose={closeDrawer}
            />
          </MobileDrawer>
        )}
      </div>
    </SyncErrorProvider>
  );
}
