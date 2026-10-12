import { useCallback, useMemo, useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { PreviewControls } from '@prepcook/ui/components/PreviewControls';
import { usePreviewDevServer } from '@/features/workspace/model/hooks/usePreviewDevServer';
import { useLogStream } from '@/shared/hooks/useLogStream';
import {
  useUiPreferencesStore,
  RIGHT_MAIN_PANEL_MODES,
} from '@/shared/stores/useUiPreferencesStore';
import { cn } from '@/shared/lib/utils';
import { useLogsPanel } from '@/shared/hooks/useLogsPanel';
import { VirtualizedProcessLogs } from '@/shared/components/VirtualizedProcessLogs';
import { getDevServerWorkingDir } from '@/shared/lib/devServerUtils';

interface PreviewControlsContainerProps {
  workspaceId: string;
  className: string;
}

export function PreviewControlsContainer({
  workspaceId,
  className,
}: PreviewControlsContainerProps) {
  const { t } = useTranslation(['tasks', 'common']);
  const { viewProcessInPanel } = useLogsPanel();
  const setRightMainPanelMode = useUiPreferencesStore(
    (s) => s.setRightMainPanelMode
  );

  const { isStarting, startError, runningDevServers, devServerProcesses } =
    usePreviewDevServer(workspaceId);

  const [activeProcessId, setActiveProcessId] = useState<string | null>(null);

  useEffect(() => {
    if (devServerProcesses.length > 0 && !activeProcessId) {
      setActiveProcessId(devServerProcesses[0].id);
    }
  }, [devServerProcesses, activeProcessId]);

  const activeProcess =
    devServerProcesses.find((p) => p.id === activeProcessId) ??
    devServerProcesses[0];

  const processTabs = useMemo(
    () =>
      devServerProcesses.map((process) => ({
        id: process.id,
        label:
          getDevServerWorkingDir(process) ??
          t('preview.browser.devServerFallback'),
      })),
    [devServerProcesses, t]
  );

  const { logs, error: logsError } = useLogStream(activeProcess?.id ?? '');

  const handleViewFullLogs = useCallback(() => {
    const targetId = activeProcess?.id;
    if (targetId) {
      viewProcessInPanel(targetId);
    } else {
      setRightMainPanelMode(RIGHT_MAIN_PANEL_MODES.LOGS);
    }
  }, [activeProcess?.id, viewProcessInPanel, setRightMainPanelMode]);

  const handleTabChange = useCallback((processId: string) => {
    setActiveProcessId(processId);
  }, []);

  // The dev server is each repository's `[app] start` from its
  // `.prepcook/scripts.toml`; whether there is one is the start endpoint's call.
  if (devServerProcesses.length === 0 && !isStarting) {
    if (!startError) return null;
    return (
      <div
        className={cn(
          'w-full bg-secondary px-base py-half space-y-half',
          className
        )}
      >
        <p className="text-sm font-medium text-normal">
          {startError.noStartScript
            ? t('preview.noServer.noStartScript')
            : t('preview.noServer.startFailed')}
        </p>
        <p className="text-sm text-low whitespace-pre-wrap break-words">
          {startError.message}
        </p>
      </div>
    );
  }

  return (
    <PreviewControls
      processTabs={processTabs}
      activeProcessId={activeProcess?.id ?? null}
      logsContent={
        <VirtualizedProcessLogs
          logs={logs}
          error={logsError}
          searchQuery=""
          matchIndices={[]}
          currentMatchIndex={-1}
        />
      }
      onViewFullLogs={handleViewFullLogs}
      onTabChange={handleTabChange}
      isLoading={isStarting || runningDevServers.length > 0}
      className={className}
    />
  );
}
