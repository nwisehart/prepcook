import { useMemo } from 'react';
import {
  useMutation,
  useMutationState,
  useQueryClient,
} from '@tanstack/react-query';
import { workspacesApi, executionProcessesApi } from '@/shared/lib/api';
import { useWorkspaceExecution } from '@/shared/hooks/useWorkspaceExecution';
import {
  filterRunningDevServers,
  filterDevServerProcesses,
  deduplicateDevServersByWorkingDir,
} from '@/shared/lib/devServerUtils';
import { workspaceSummaryKeys } from '@/shared/hooks/workspaceSummaryKeys';

/** Why the last attempt to start the preview's dev server failed. */
export interface PreviewStartError {
  message: string;
  /**
   * The server found no `[app] start` in any repository's
   * `.prepcook/scripts.toml`, which is what the preview runs.
   */
  noStartScript: boolean;
}

function toStartError(error: unknown): PreviewStartError {
  const message = error instanceof Error ? error.message : String(error);
  return { message, noStartScript: message.includes('.prepcook/scripts.toml') };
}

interface UsePreviewDevServerOptions {
  onStartSuccess?: () => void;
  onStartError?: (err: unknown) => void;
  onStopSuccess?: () => void;
  onStopError?: (err: unknown) => void;
}

export function usePreviewDevServer(
  workspaceId: string | undefined,
  options?: UsePreviewDevServerOptions
) {
  const queryClient = useQueryClient();
  const { attemptData } = useWorkspaceExecution(workspaceId);

  const runningDevServers = useMemo(
    () => filterRunningDevServers(attemptData.processes),
    [attemptData.processes]
  );

  const devServerProcesses = useMemo(
    () =>
      deduplicateDevServersByWorkingDir(
        filterDevServerProcesses(attemptData.processes)
      ),
    [attemptData.processes]
  );

  // Read across every component using this hook for the workspace, so the
  // preview pane and its controls report the same failed start.
  const startStates = useMutationState({
    filters: { mutationKey: ['startDevServer', workspaceId] },
    select: (mutation) => mutation.state,
  });
  const latestStart = startStates[startStates.length - 1];
  const startError = useMemo(
    () =>
      latestStart?.status === 'error' && runningDevServers.length === 0
        ? toStartError(latestStart.error)
        : null,
    [latestStart, runningDevServers.length]
  );

  const startMutation = useMutation({
    mutationKey: ['startDevServer', workspaceId],
    mutationFn: async () => {
      if (!workspaceId) return;
      await workspacesApi.startDevServer(workspaceId);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ['executionProcesses', workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
      options?.onStartSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to start dev server:', err);
      options?.onStartError?.(err);
    },
  });

  const stopMutation = useMutation({
    mutationKey: ['stopDevServer', workspaceId],
    mutationFn: async () => {
      if (runningDevServers.length === 0) return;
      await Promise.all(
        runningDevServers.map((ds) =>
          executionProcessesApi.stopExecutionProcess(ds.id)
        )
      );
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ['executionProcesses', workspaceId],
      });
      for (const ds of runningDevServers) {
        queryClient.invalidateQueries({
          queryKey: ['processDetails', ds.id],
        });
      }
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
      options?.onStopSuccess?.();
    },
    onError: (err) => {
      console.error('Failed to stop dev server:', err);
      options?.onStopError?.(err);
    },
  });

  return {
    start: startMutation.mutate,
    stop: stopMutation.mutate,
    isStarting: startMutation.isPending,
    isStopping: stopMutation.isPending,
    startError,
    runningDevServers,
    devServerProcesses,
  };
}
