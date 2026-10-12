import { useCallback, useMemo } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  BaseAgentCapability,
  Config,
  Environment,
  ExecutorProfile,
  UserSystemInfo,
} from 'shared/types';
import type { UserSystemContextType } from './useUserSystem';

interface UseUserSystemControllerOptions {
  queryKey: readonly unknown[];
  enabled?: boolean;
  load: () => Promise<UserSystemInfo>;
  save: (config: Config) => Promise<Config>;
  loading?: boolean;
}

export function useUserSystemController({
  queryKey,
  enabled = true,
  load,
  save,
  loading: loadingOverride,
}: UseUserSystemControllerOptions): {
  value: UserSystemContextType;
  userSystemInfo: UserSystemInfo | undefined;
  isLoading: boolean;
} {
  const queryClient = useQueryClient();

  const { data: userSystemInfo, isLoading } = useQuery({
    queryKey,
    queryFn: load,
    enabled,
    staleTime: 5 * 60 * 1000,
    // Only polled while a launch pause is showing, so the banner counts down
    // and clears on its own. /api/info is otherwise cache-served: it resolves
    // login status and rebuilds the executor profiles, which is not worth
    // repeating on a timer for every open tab.
    refetchInterval: (query) =>
      query.state.data?.launch_pause_minutes == null ? false : 60 * 1000,
  });

  const config = userSystemInfo?.config || null;
  const appVersion = userSystemInfo?.version || null;
  const previewProxyPort = userSystemInfo?.preview_proxy_port ?? null;
  const environment = userSystemInfo?.environment || null;
  const machineId = userSystemInfo?.machine_id || null;
  const boardStatus = userSystemInfo?.board_status || null;
  const remoteAuthDegraded = userSystemInfo?.remote_auth_degraded || null;
  const claudeSeatDegraded = userSystemInfo?.claude_seat_degraded ?? false;
  const launchPauseMinutes = userSystemInfo?.launch_pause_minutes ?? null;
  const profiles =
    (userSystemInfo?.executors as Record<string, ExecutorProfile> | null) ||
    null;
  const capabilities =
    (userSystemInfo?.capabilities as Record<
      string,
      BaseAgentCapability[]
    > | null) || null;
  const loading = loadingOverride ?? isLoading;

  const updateConfig = useCallback(
    (updates: Partial<Config>) => {
      queryClient.setQueryData<UserSystemInfo>(queryKey, (old) => {
        if (!old) return old;
        return {
          ...old,
          config: { ...old.config, ...updates },
        };
      });
    },
    [queryClient, queryKey]
  );

  const saveConfig = useCallback(async (): Promise<boolean> => {
    if (!config) return false;
    try {
      await save(config);
      return true;
    } catch (err) {
      console.error('Error saving config:', err);
      return false;
    }
  }, [config, save]);

  const updateAndSaveConfig = useCallback(
    async (updates: Partial<Config>): Promise<boolean> => {
      if (!config) return false;

      const newConfig = { ...config, ...updates };
      updateConfig(updates);

      try {
        const saved = await save(newConfig);
        queryClient.setQueryData<UserSystemInfo>(queryKey, (old) => {
          if (!old) return old;
          return {
            ...old,
            config: saved,
          };
        });
        return true;
      } catch (err) {
        console.error('Error saving config:', err);
        queryClient.invalidateQueries({ queryKey });
        return false;
      }
    },
    [config, queryClient, queryKey, save, updateConfig]
  );

  const reloadSystem = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  const setEnvironment = useCallback(
    (env: Environment | null) => {
      queryClient.setQueryData<UserSystemInfo>(queryKey, (old) => {
        if (!old || !env) return old;
        return { ...old, environment: env };
      });
    },
    [queryClient, queryKey]
  );

  const setProfiles = useCallback(
    (newProfiles: Record<string, ExecutorProfile> | null) => {
      queryClient.setQueryData<UserSystemInfo>(queryKey, (old) => {
        if (!old || !newProfiles) return old;
        return {
          ...old,
          executors: newProfiles as unknown as UserSystemInfo['executors'],
        };
      });
    },
    [queryClient, queryKey]
  );

  const setCapabilities = useCallback(
    (newCapabilities: Record<string, BaseAgentCapability[]> | null) => {
      queryClient.setQueryData<UserSystemInfo>(queryKey, (old) => {
        if (!old || !newCapabilities) return old;
        return { ...old, capabilities: newCapabilities };
      });
    },
    [queryClient, queryKey]
  );

  const value = useMemo<UserSystemContextType>(
    () => ({
      system: {
        appVersion,
        previewProxyPort,
        config,
        environment,
        profiles,
        capabilities,
        machineId,
        boardStatus,
        remoteAuthDegraded,
        claudeSeatDegraded,
        launchPauseMinutes,
      },
      appVersion,
      previewProxyPort,
      config,
      environment,
      profiles,
      capabilities,
      machineId,
      boardStatus,
      remoteAuthDegraded,
      claudeSeatDegraded,
      launchPauseMinutes,
      updateConfig,
      saveConfig,
      updateAndSaveConfig,
      setEnvironment,
      setProfiles,
      setCapabilities,
      reloadSystem,
      loading,
    }),
    [
      machineId,
      appVersion,
      capabilities,
      config,
      environment,
      loading,
      previewProxyPort,
      boardStatus,
      remoteAuthDegraded,
      claudeSeatDegraded,
      launchPauseMinutes,
      profiles,
      reloadSystem,
      saveConfig,
      setCapabilities,
      setEnvironment,
      setProfiles,
      updateAndSaveConfig,
      updateConfig,
    ]
  );

  return {
    value,
    userSystemInfo,
    isLoading,
  };
}
