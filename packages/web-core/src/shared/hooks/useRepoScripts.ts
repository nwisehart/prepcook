import { useQueries, useQuery } from '@tanstack/react-query';
import type { RepoScriptsResponse } from 'shared/types';
import { scriptsApi, secretsApi } from '@/shared/lib/api';

export const repoScriptsKeys = {
  all: ['repoScripts'] as const,
  byRepo: (repoId: string) => ['repoScripts', repoId] as const,
};

export const secretsKeys = {
  all: ['secrets'] as const,
};

/** A repository's parsed `.prepcook/scripts.toml`, or why it cannot be read. */
export function useRepoScripts(repoId: string | null | undefined) {
  return useQuery({
    queryKey: repoScriptsKeys.byRepo(repoId ?? ''),
    queryFn: () => scriptsApi.getRepoScripts(repoId!),
    enabled: !!repoId,
  });
}

/**
 * The scripts manifests of several repositories. `byRepoId` only holds the
 * repositories whose response has arrived, so a caller can tell "not loaded
 * yet" from "no manifest".
 */
export function useReposScripts(repoIds: string[]) {
  return useQueries({
    queries: repoIds.map((repoId) => ({
      queryKey: repoScriptsKeys.byRepo(repoId),
      queryFn: () => scriptsApi.getRepoScripts(repoId),
    })),
    combine: (results) => {
      const byRepoId = new Map<string, RepoScriptsResponse>();
      results.forEach((result, index) => {
        if (result.data) byRepoId.set(repoIds[index], result.data);
      });
      return {
        byRepoId,
        isLoading: results.some((result) => result.isLoading),
      };
    },
  });
}

/** Names in the secrets store and where the file lives; never values. */
export function useSecrets() {
  return useQuery({
    queryKey: secretsKeys.all,
    queryFn: () => secretsApi.list(),
  });
}
