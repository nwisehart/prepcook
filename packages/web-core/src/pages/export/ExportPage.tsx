import { useCallback, useMemo } from 'react';
import { ThemeMode } from 'shared/types';
import { useTheme } from '@/shared/hooks/useTheme';
import { ExportLayout } from '@/features/export/ui/ExportLayout';
import type { ExportRequest } from '@/features/export/ui/ExportDownload';
import type { ExportProject } from '@/features/export/ui/ExportChooseProjects';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useOrganizationProjects } from '@/shared/hooks/useOrganizationProjects';
import { makeRequest as makeRemoteRequest } from '@/shared/lib/remoteApi';
import { BoardRequiredPrompt } from '@/shared/dialogs/shared/BoardRequiredPrompt';

function resolveTheme(theme: ThemeMode): 'light' | 'dark' {
  if (theme === ThemeMode.SYSTEM) {
    return window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
  }
  return theme === ThemeMode.DARK ? 'dark' : 'light';
}

interface ExportPageProps {
  exportFn: (request: ExportRequest) => Promise<Response>;
  orgsLoading: boolean;
  projects: ExportProject[];
  projectsLoading: boolean;
  selectedOrgId: string | null;
}

export function ExportPage({
  exportFn,
  orgsLoading,
  projects,
  projectsLoading,
  selectedOrgId,
}: ExportPageProps) {
  const { theme } = useTheme();

  const logoSrc =
    resolveTheme(theme) === 'dark'
      ? '/prepcook-logo-dark.svg'
      : '/prepcook-logo.svg';

  return (
    <div className="h-full overflow-auto bg-primary">
      <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col justify-center px-base py-double">
        <div className="rounded-sm border border-border bg-secondary p-double space-y-double">
          <header className="space-y-double text-center">
            <div className="flex justify-center">
              <img src={logoSrc} alt="Prepcook" className="h-8 w-auto logo" />
            </div>
            <p className="text-sm text-low">
              Download your project and issue data to CSV files.
            </p>
          </header>
          <ExportLayout
            exportFn={exportFn}
            orgsLoading={orgsLoading}
            projects={projects}
            projectsLoading={projectsLoading}
            selectedOrgId={selectedOrgId}
          />
        </div>
      </div>
    </div>
  );
}

export function ExportPageContainer() {
  const { isLoaded, hasBoardAccess } = useAuth();
  const { data: orgsData, isLoading: orgsLoading } = useUserOrganizations();
  // The board has one organization; every signed-in user belongs to it.
  const selectedOrgId = orgsData?.organizations[0]?.id ?? null;

  const { data: projectData = [], isLoading: projectsLoading } =
    useOrganizationProjects(selectedOrgId);
  const projects = useMemo<ExportProject[]>(
    () =>
      projectData.map((project) => ({
        id: project.id,
        name: project.name,
      })),
    [projectData]
  );

  const exportFn = useCallback(async (request: ExportRequest) => {
    return makeRemoteRequest('/v1/export', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
  }, []);

  if (!isLoaded) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-primary">
        <p className="text-sm text-low">Loading...</p>
      </div>
    );
  }

  if (!hasBoardAccess) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-primary p-base">
        <BoardRequiredPrompt
          className="max-w-md"
          title="No board to export from"
          description="Exports read the board's projects, and this server cannot reach a board."
        />
      </div>
    );
  }

  return (
    <ExportPage
      exportFn={exportFn}
      orgsLoading={orgsLoading}
      projects={projects}
      projectsLoading={projectsLoading}
      selectedOrgId={selectedOrgId}
    />
  );
}
