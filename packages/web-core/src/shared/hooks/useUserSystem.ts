import { useContext } from 'react';
import { createHmrContext } from '@/shared/lib/hmrContext';
import type {
  Config,
  Environment,
  BaseAgentCapability,
  BoardStatus,
} from 'shared/types';
import type { ExecutorProfile } from 'shared/types';

export interface UserSystemState {
  appVersion: string | null;
  previewProxyPort: number | null;
  config: Config | null;
  environment: Environment | null;
  profiles: Record<string, ExecutorProfile> | null;
  capabilities: Record<string, BaseAgentCapability[]> | null;
  machineId: string | null;
  boardStatus: BoardStatus | null;
  remoteAuthDegraded: string | null;
  /** Claude subscription seat probe last failed — show a re-auth warning banner. */
  claudeSeatDegraded: boolean;
  /**
   * Minutes until agent launches resume, while the seat's usage window is
   * exhausted; null when launches are running. Separate from
   * `claudeSeatDegraded`: the seat works, its window does not.
   */
  launchPauseMinutes: number | null;
}

export interface UserSystemContextType {
  // Full system state
  system: UserSystemState;

  // Hot path - config helpers (most frequently used)
  appVersion: string | null;
  previewProxyPort: number | null;
  config: Config | null;
  updateConfig: (updates: Partial<Config>) => void;
  updateAndSaveConfig: (updates: Partial<Config>) => Promise<boolean>;
  saveConfig: () => Promise<boolean>;

  // System data access
  environment: Environment | null;
  profiles: Record<string, ExecutorProfile> | null;
  capabilities: Record<string, BaseAgentCapability[]> | null;
  machineId: string | null;
  boardStatus: BoardStatus | null;
  remoteAuthDegraded: string | null;
  claudeSeatDegraded: boolean;
  launchPauseMinutes: number | null;
  setEnvironment: (env: Environment | null) => void;
  setProfiles: (profiles: Record<string, ExecutorProfile> | null) => void;
  setCapabilities: (caps: Record<string, BaseAgentCapability[]> | null) => void;

  // Reload system data
  reloadSystem: () => Promise<void>;

  // State
  loading: boolean;
}

export const UserSystemContext = createHmrContext<
  UserSystemContextType | undefined
>('UserSystemContext', undefined);

export function useUserSystem() {
  const context = useContext(UserSystemContext);
  if (context === undefined) {
    throw new Error('useUserSystem must be used within a UserSystemProvider');
  }
  return context;
}
