import { useQuery } from '@tanstack/react-query';
import { workspacesApi } from '@/shared/lib/api';

/**
 * Polls for an active lavish-axi review session inside the workspace's
 * worktree. Planning agents open the review gate headless, so this is what
 * lets the UI surface an "Open spec review" entry point the moment the
 * artifact is ready.
 */
export function useLavishSession(workspaceId: string | null | undefined) {
  return useQuery({
    queryKey: ['lavishSession', workspaceId],
    queryFn: () => workspacesApi.getLavishSession(workspaceId!),
    enabled: !!workspaceId,
    refetchInterval: 5000,
  });
}
