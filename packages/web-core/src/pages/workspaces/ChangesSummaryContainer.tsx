import { useCallback, useMemo, type ReactNode } from 'react';
import { Trans, useTranslation } from 'react-i18next';
import {
  ChangesSummaryCard,
  type ChangesSummaryFile,
} from '@prepcook/ui/components/ChangesPanel';
import type { Merge, RepoWithTargetBranch, Workspace } from 'shared/types';
import { useDiffs, useDiffStats } from '@/shared/stores/useWorkspaceDiffStore';
import { useChangesViewActions } from '@/shared/hooks/useChangesView';
import { useBranchStatus } from '@/shared/hooks/useBranchStatus';
import { sortDiffs } from '@/shared/lib/fileTreeUtils';
import {
  RIGHT_MAIN_PANEL_MODES,
  useUiPreferencesStore,
} from '@/shared/stores/useUiPreferencesStore';

// Rows listed in the card; the rest are counted and live in the full diff.
const MAX_LISTED_FILES = 8;

interface ChangesSummaryContainerProps {
  selectedWorkspace: Workspace | undefined;
  repos: RepoWithTargetBranch[];
}

function openPullRequest(merges: Merge[]) {
  const pr =
    merges.find((m) => m.type === 'pr' && m.pr_info.status === 'open') ??
    merges.find((m) => m.type === 'pr' && m.pr_info.status === 'merged');
  return pr?.type === 'pr' ? pr.pr_info : null;
}

/**
 * The Review tab's "Changes" card: what the session changed, where its
 * branch stands, and the way into the full diff.
 */
export function ChangesSummaryContainer({
  selectedWorkspace,
  repos,
}: ChangesSummaryContainerProps) {
  const { t } = useTranslation('common');
  const diffs = useDiffs();
  const diffStats = useDiffStats();
  const { viewFileInChanges } = useChangesViewActions();
  const setRightMainPanelMode = useUiPreferencesStore(
    (s) => s.setRightMainPanelMode
  );
  const workspaceId = selectedWorkspace?.id;
  const { data: branchStatus } = useBranchStatus(workspaceId);

  const files = useMemo<ChangesSummaryFile[]>(
    () =>
      sortDiffs(diffs).map((diff) => ({
        path: diff.newPath ?? diff.oldPath ?? '',
        additions: diff.additions,
        deletions: diff.deletions,
        isNew: diff.change === 'added',
      })),
    [diffs]
  );

  const handleOpenDiff = useCallback(() => {
    setRightMainPanelMode(RIGHT_MAIN_PANEL_MODES.REVIEW, workspaceId);
  }, [setRightMainPanelMode, workspaceId]);

  // Where the branch stands, for a single-repository session; with several
  // repositories each repository card below says it for itself.
  const summary = useMemo((): ReactNode => {
    const branch = selectedWorkspace?.branch;
    if (!branch) return null;
    const mono = <span className="font-mono text-normal" />;
    const status =
      repos.length === 1
        ? branchStatus?.find((s) => s.repo_id === repos[0].id)
        : undefined;
    if (!status) {
      return (
        <Trans
          t={t}
          i18nKey="workspaces.rightPanel.onBranch"
          values={{ branch }}
          components={{ mono }}
        />
      );
    }

    const target = status.target_branch_name;
    const behind = status.commits_behind ?? 0;
    const pr = openPullRequest(status.merges);
    const prLine = pr
      ? pr.status === 'merged'
        ? t('workspaces.rightPanel.prMerged', { number: Number(pr.number) })
        : t('workspaces.rightPanel.prOpen', { number: Number(pr.number) })
      : t('workspaces.rightPanel.noPrYet');

    return (
      <>
        {behind > 0 ? (
          <Trans
            t={t}
            i18nKey="workspaces.rightPanel.branchBehind"
            values={{ branch, target, count: behind }}
            components={{ mono }}
          />
        ) : (
          <Trans
            t={t}
            i18nKey="workspaces.rightPanel.branchRebased"
            values={{ branch, target }}
            components={{ mono }}
          />
        )}{' '}
        {prLine}
      </>
    );
  }, [selectedWorkspace?.branch, repos, branchStatus, t]);

  const hidden = Math.max(0, files.length - MAX_LISTED_FILES);

  return (
    <ChangesSummaryCard
      title={t('sections.changes')}
      filesLabel={t('workspaces.rightPanel.fileCount', {
        count: diffStats.files_changed,
      })}
      linesAdded={diffStats.lines_added}
      linesRemoved={diffStats.lines_removed}
      summary={summary}
      files={files.slice(0, MAX_LISTED_FILES)}
      hiddenFilesLabel={
        hidden > 0
          ? t('workspaces.rightPanel.moreFiles', { count: hidden })
          : undefined
      }
      newLabel={t('workspaces.rightPanel.newFile')}
      emptyLabel={t('workspaces.rightPanel.noChanges')}
      onOpenFile={viewFileInChanges}
      openDiffLabel={t('workspaces.rightPanel.openDiff')}
      onOpenDiff={handleOpenDiff}
    />
  );
}
