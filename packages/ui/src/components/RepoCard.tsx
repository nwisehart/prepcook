import { useMemo, type ReactNode } from 'react';
import {
  GitBranchIcon,
  GitPullRequestIcon,
  ArrowsClockwiseIcon,
  ArrowUpIcon,
  ArrowDownIcon,
  CrosshairIcon,
  ArrowSquareOutIcon,
  GitMergeIcon,
  CheckCircleIcon,
  SpinnerGapIcon,
  WarningCircleIcon,
  DotsThreeIcon,
  LinkIcon,
} from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import {
  DropdownMenu,
  DropdownMenuTriggerButton,
  DropdownMenuContent,
  DropdownMenuItem,
} from './Dropdown';
import { SplitButton, type SplitButtonOption } from './SplitButton';

export type RepoAction =
  | 'pull-request'
  | 'link-pr'
  | 'merge'
  | 'change-target'
  | 'rebase'
  | 'push';

const repoActionOptions: SplitButtonOption<RepoAction>[] = [
  {
    value: 'pull-request',
    label: 'Open pull request',
    icon: GitPullRequestIcon,
  },
  { value: 'link-pr', label: 'Link pull request', icon: LinkIcon },
  { value: 'merge', label: 'Merge', icon: GitMergeIcon },
];

interface RepoCardProps {
  repoId: string;
  name: string;
  targetBranch: string;
  commitsAhead?: number;
  commitsBehind?: number;
  prNumber?: number;
  prUrl?: string;
  prStatus?: 'open' | 'merged' | 'closed' | 'unknown';
  showPushButton?: boolean;
  isPushPending?: boolean;
  isPushSuccess?: boolean;
  isPushError?: boolean;
  isTargetRemote?: boolean;
  branchDropdownContent?: ReactNode;
  selectedAction?: RepoAction;
  onSelectedActionChange?: (action: RepoAction) => void;
  onChangeTarget?: () => void;
  onRebase?: () => void;
  onActionsClick?: (action: RepoAction) => void;
  onPushClick?: () => void;
  onMoreClick?: () => void;
}

export function RepoCard({
  name,
  targetBranch,
  commitsAhead = 0,
  commitsBehind = 0,
  prNumber,
  prUrl,
  prStatus,
  showPushButton = false,
  isPushPending = false,
  isPushSuccess = false,
  isPushError = false,
  isTargetRemote = false,
  branchDropdownContent,
  selectedAction = 'pull-request',
  onSelectedActionChange,
  onChangeTarget,
  onRebase,
  onActionsClick,
  onPushClick,
  onMoreClick,
}: RepoCardProps) {
  const { t } = useTranslation('tasks');
  const { t: tCommon } = useTranslation('common');

  // Hide "Open pull request" and "Link pull request" when PR is already open
  // Hide "Link pull request" when any PR is already linked (open or merged)
  // Hide "merge" option when PR is already open or target branch is remote
  const hasPrOpen = prStatus === 'open';
  const hasPrLinked = !!prNumber;
  const availableActionOptions = useMemo(
    () =>
      repoActionOptions.filter((opt) => {
        if (opt.value === 'pull-request' && hasPrOpen) return false;
        if (opt.value === 'link-pr' && hasPrLinked) return false;
        if (opt.value === 'merge' && (hasPrOpen || isTargetRemote))
          return false;
        return true;
      }),
    [hasPrOpen, hasPrLinked, isTargetRemote]
  );

  // If current selection is unavailable, fall back to the first available option.
  const effectiveSelectedAction = useMemo(() => {
    const selectedOption = availableActionOptions.find(
      (option) => option.value === selectedAction
    );
    return (
      selectedOption?.value ??
      availableActionOptions[0]?.value ??
      selectedAction
    );
  }, [availableActionOptions, selectedAction]);

  return (
    <div className="my-base flex flex-col gap-2.5 rounded-lg border border-border bg-secondary px-3.5 py-3">
      <div className="truncate text-sm font-semibold text-high">{name}</div>
      {/* Branch row */}
      <div className="flex items-center gap-base">
        <div className="min-w-0 flex-1">
          <DropdownMenu>
            <DropdownMenuTriggerButton
              icon={GitBranchIcon}
              label={targetBranch}
              className="max-w-full"
            />
            <DropdownMenuContent>
              {branchDropdownContent ?? (
                <>
                  <DropdownMenuItem
                    icon={CrosshairIcon}
                    onClick={onChangeTarget}
                  >
                    {t('git.actions.changeTarget')}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    icon={ArrowsClockwiseIcon}
                    onClick={onRebase}
                  >
                    {t('rebase.common.action')}
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* Commits ahead/behind indicators */}
        {commitsAhead > 0 && (
          <span className="inline-flex items-center gap-0.5 text-xs text-success shrink-0">
            <ArrowUpIcon className="size-3.5" />
            <span className="font-mono font-medium">{commitsAhead}</span>
          </span>
        )}
        {commitsBehind > 0 && (
          <span className="inline-flex items-center gap-0.5 text-xs text-error shrink-0">
            <ArrowDownIcon className="size-3.5" />
            <span className="font-mono font-medium">{commitsBehind}</span>
          </span>
        )}

        <button
          onClick={onMoreClick}
          className="flex size-8 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand shrink-0"
          title={tCommon('workspaces.more')}
        >
          <DotsThreeIcon className="size-4" weight="bold" />
        </button>
      </div>

      {/* PR status row */}
      {prNumber && (
        <div className="flex flex-wrap items-center gap-2">
          {prStatus === 'merged' ? (
            prUrl ? (
              <button
                onClick={() => window.open(prUrl, '_blank')}
                className="inline-flex h-7 items-center gap-1.5 rounded px-2.5 bg-success-soft text-success hover:bg-success/15 text-sm font-semibold transition-colors"
              >
                <CheckCircleIcon className="size-icon-xs" weight="fill" />
                {t('git.pr.merged', { prNumber })}
                <ArrowSquareOutIcon className="size-icon-xs" weight="bold" />
              </button>
            ) : (
              <span className="inline-flex h-7 items-center gap-1.5 rounded px-2.5 bg-success-soft text-success text-sm font-semibold">
                <CheckCircleIcon className="size-icon-xs" weight="fill" />
                {t('git.pr.merged', { prNumber })}
              </span>
            )
          ) : prUrl ? (
            <button
              onClick={() => window.open(prUrl, '_blank')}
              className="inline-flex h-7 items-center gap-1.5 rounded px-2.5 bg-panel text-high hover:bg-border text-sm font-semibold transition-colors"
            >
              <GitPullRequestIcon className="size-icon-xs" weight="fill" />
              {t('git.pr.open', { number: prNumber })}
              <ArrowSquareOutIcon className="size-icon-xs" weight="bold" />
            </button>
          ) : (
            <span className="inline-flex h-7 items-center gap-1.5 rounded px-2.5 bg-panel text-high text-sm font-semibold">
              <GitPullRequestIcon className="size-icon-xs" weight="fill" />
              {t('git.pr.open', { number: prNumber })}
            </span>
          )}
          {/* Push button - shows loading/success/error state */}
          {(showPushButton ||
            isPushPending ||
            isPushSuccess ||
            isPushError) && (
            <button
              onClick={onPushClick}
              disabled={isPushPending || isPushSuccess || isPushError}
              className={`inline-flex h-7 items-center gap-1.5 rounded px-2.5 text-sm font-semibold transition-colors disabled:cursor-not-allowed ${
                isPushSuccess
                  ? 'bg-success-soft text-success'
                  : isPushError
                    ? 'bg-error-soft text-error'
                    : 'bg-panel text-high hover:bg-border disabled:opacity-50'
              }`}
            >
              {isPushPending ? (
                <SpinnerGapIcon className="size-icon-xs animate-spin" />
              ) : isPushSuccess ? (
                <CheckCircleIcon className="size-icon-xs" weight="fill" />
              ) : isPushError ? (
                <WarningCircleIcon className="size-icon-xs" weight="fill" />
              ) : (
                <ArrowUpIcon className="size-icon-xs" weight="bold" />
              )}
              {isPushPending
                ? t('git.states.pushing')
                : isPushSuccess
                  ? t('git.states.pushed')
                  : isPushError
                    ? t('git.states.pushFailed')
                    : t('git.states.push')}
            </button>
          )}
        </div>
      )}

      {/* Actions row - only show when there are available actions */}
      {availableActionOptions.length > 0 && (
        <div>
          <SplitButton
            options={availableActionOptions}
            selectedValue={effectiveSelectedAction}
            onSelectionChange={(action) => onSelectedActionChange?.(action)}
            onAction={(action) => onActionsClick?.(action)}
          />
        </div>
      )}
    </div>
  );
}
