import { create } from 'zustand';
import type { RunStep } from 'shared/types';

export interface RepoScriptRun {
  runId: string;
  step: RunStep;
}

type State = {
  /**
   * The latest test run started from Settings, per repository id. Kept
   * outside the component so a run that is still going survives switching
   * repositories or closing the Settings dialog.
   */
  runs: Record<string, RepoScriptRun | undefined>;
  setRun: (repoId: string, run: RepoScriptRun | null) => void;
};

export const useScriptRunStore = create<State>()((set) => ({
  runs: {},
  setRun: (repoId, run) =>
    set((state) => ({
      runs: { ...state.runs, [repoId]: run ?? undefined },
    })),
}));
