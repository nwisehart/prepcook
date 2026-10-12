import { useCallback } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import type { RunStep } from 'shared/types';
import { scriptsApi } from '@/shared/lib/api';
import { useRawLogStream } from '@/shared/hooks/useLogStream';
import { useScriptRunStore } from '@/shared/stores/useScriptRunStore';

const RUN_POLL_MS = 1000;

/**
 * Test runs of a repository's `.prepcook/scripts.toml` steps, one at a time.
 * Output streams over the run's raw-logs WebSocket; the report is polled
 * from `GET /api/script-runs/{id}` until the run has a `finished_at`.
 */
export function useRepoScriptRun(repoId: string) {
  const run = useScriptRunStore((s) => s.runs[repoId]);
  const setRun = useScriptRunStore((s) => s.setRun);

  const stateQuery = useQuery({
    queryKey: ['scriptRun', run?.runId ?? ''],
    queryFn: () => scriptsApi.getRun(run!.runId),
    enabled: !!run,
    refetchInterval: (query) =>
      query.state.data?.finished_at || query.state.error ? false : RUN_POLL_MS,
  });

  const { logs, error: logsError } = useRawLogStream(
    run ? `/api/script-runs/${run.runId}/raw-logs/ws` : ''
  );

  const state =
    stateQuery.data && stateQuery.data.run_id === run?.runId
      ? stateQuery.data
      : null;
  const finished = !!state?.finished_at;
  // A run the server no longer knows (evicted from its history, or the
  // server restarted) is not live: it cannot finish any more.
  const isLive = !!run && !finished && !stateQuery.isError;

  const startMutation = useMutation({
    mutationFn: (step: RunStep) => scriptsApi.run(repoId, step),
    onSuccess: (response, step) => {
      setRun(repoId, { runId: response.run_id, step });
    },
  });

  const start = useCallback(
    (step: RunStep) => {
      if (isLive || startMutation.isPending) return;
      startMutation.mutate(step);
    },
    [isLive, startMutation]
  );

  const dismiss = useCallback(() => {
    if (isLive) return;
    setRun(repoId, null);
    startMutation.reset();
  }, [isLive, repoId, setRun, startMutation]);

  return {
    run: run ?? null,
    state,
    stateError: stateQuery.error,
    logs,
    logsError,
    isLive,
    isStarting: startMutation.isPending,
    startError: startMutation.error,
    start,
    dismiss,
  };
}
