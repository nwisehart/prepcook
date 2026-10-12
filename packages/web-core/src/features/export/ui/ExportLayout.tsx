import { useState } from 'react';
import {
  ExportChooseProjects,
  type ExportProject,
} from './ExportChooseProjects';
import { ExportDownload, type ExportRequest } from './ExportDownload';

interface ExportLayoutProps {
  exportFn: (request: ExportRequest) => Promise<Response>;
  orgsLoading: boolean;
  projects: ExportProject[];
  projectsLoading: boolean;
  selectedOrgId: string | null;
}

interface ExportData {
  orgId: string;
  projectIds: string[];
  includeAttachments: boolean;
}

export function ExportLayout({
  exportFn,
  orgsLoading,
  projects,
  projectsLoading,
  selectedOrgId,
}: ExportLayoutProps) {
  const [exportData, setExportData] = useState<ExportData | null>(null);

  const handleChooseProjectsContinue = (
    orgId: string,
    projectIds: string[],
    includeAttachments: boolean
  ) => {
    setExportData({
      orgId,
      projectIds,
      includeAttachments,
    });
  };

  if (exportData) {
    return (
      <ExportDownload
        orgId={exportData.orgId}
        projectIds={exportData.projectIds}
        includeAttachments={exportData.includeAttachments}
        onExportMore={() => setExportData(null)}
        exportFn={exportFn}
      />
    );
  }

  return (
    <ExportChooseProjects
      orgsLoading={orgsLoading}
      projects={projects}
      projectsLoading={projectsLoading}
      selectedOrgId={selectedOrgId}
      onContinue={handleChooseProjectsContinue}
    />
  );
}
