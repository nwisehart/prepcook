import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  CaretDownIcon,
  CaretUpIcon,
  GithubLogoIcon,
  TreeStructureIcon,
  type Icon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { Tooltip } from './Tooltip';
import { FileTreeSearchBar } from './FileTreeSearchBar';

export interface ReviewToolbarAction {
  id: string;
  icon: Icon;
  tooltip: string;
  shortcut?: string;
  isActive?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

interface ToolbarButtonProps {
  icon: Icon;
  label: string;
  shortcut?: string;
  isActive?: boolean;
  disabled?: boolean;
  className?: string;
  iconWeight?: 'regular' | 'fill';
  onClick: () => void;
}

function ToolbarButton({
  icon: IconComponent,
  label,
  shortcut,
  isActive = false,
  disabled = false,
  className,
  iconWeight,
  onClick,
}: ToolbarButtonProps) {
  return (
    <Tooltip content={label} shortcut={shortcut}>
      <button
        type="button"
        onClick={onClick}
        disabled={disabled}
        aria-label={label}
        aria-pressed={isActive}
        className={cn(
          'p-1 rounded hover:bg-panel transition-colors shrink-0',
          isActive ? 'text-normal' : 'text-low hover:text-normal',
          disabled && 'opacity-40 cursor-not-allowed hover:bg-transparent',
          className
        )}
      >
        <IconComponent
          className="size-icon-sm"
          weight={iconWeight ?? (isActive ? 'fill' : 'regular')}
        />
      </button>
    </Tooltip>
  );
}

interface ReviewToolbarProps {
  isNavigatorVisible: boolean;
  onToggleNavigator: () => void;
  searchQuery: string;
  onSearchChange: (value: string) => void;
  isAllFoldersExpanded: boolean;
  onToggleExpandAllFolders: () => void;
  showGitHubComments?: boolean;
  onToggleGitHubComments?: (show: boolean) => void;
  isGitHubCommentsLoading?: boolean;
  onNavigateComments?: (direction: 'prev' | 'next') => void;
  hasFilesWithComments?: boolean;
  /** Diff display controls, rendered at the trailing end */
  actions: ReviewToolbarAction[];
  className?: string;
}

/**
 * The single toolbar of the Review tab: file-tree navigator controls on the
 * leading side, diff display controls on the trailing side.
 */
export const ReviewToolbar = memo(function ReviewToolbar({
  isNavigatorVisible,
  onToggleNavigator,
  searchQuery,
  onSearchChange,
  isAllFoldersExpanded,
  onToggleExpandAllFolders,
  showGitHubComments,
  onToggleGitHubComments,
  isGitHubCommentsLoading,
  onNavigateComments,
  hasFilesWithComments,
  actions,
  className,
}: ReviewToolbarProps) {
  const { t } = useTranslation('common');
  const githubCommentsLabel = showGitHubComments
    ? t('fileTree.hideGitHubComments')
    : t('fileTree.showGitHubComments');

  return (
    <div
      role="toolbar"
      className={cn(
        'flex items-center gap-half px-base py-half border-b bg-secondary shrink-0 min-w-0',
        className
      )}
    >
      <ToolbarButton
        icon={TreeStructureIcon}
        label={
          isNavigatorVisible
            ? t('fileTree.hideNavigator')
            : t('fileTree.showNavigator')
        }
        isActive={isNavigatorVisible}
        onClick={onToggleNavigator}
      />
      {isNavigatorVisible && (
        <div className="flex-1 min-w-0 max-w-[240px]">
          <FileTreeSearchBar
            searchQuery={searchQuery}
            onSearchChange={onSearchChange}
            isAllExpanded={isAllFoldersExpanded}
            onToggleExpandAll={onToggleExpandAllFolders}
          />
        </div>
      )}
      {showGitHubComments && onNavigateComments && hasFilesWithComments && (
        <>
          <ToolbarButton
            icon={CaretUpIcon}
            label={t('fileTree.prevGitHubComment')}
            onClick={() => onNavigateComments('prev')}
          />
          <ToolbarButton
            icon={CaretDownIcon}
            label={t('fileTree.nextGitHubComment')}
            onClick={() => onNavigateComments('next')}
          />
        </>
      )}
      {onToggleGitHubComments && (
        <ToolbarButton
          icon={GithubLogoIcon}
          label={githubCommentsLabel}
          isActive={showGitHubComments}
          iconWeight="fill"
          className={cn(isGitHubCommentsLoading && 'opacity-50 animate-pulse')}
          onClick={() => onToggleGitHubComments(!showGitHubComments)}
        />
      )}
      <div className="flex-1" />
      {actions.map((action) => (
        <ToolbarButton
          key={action.id}
          icon={action.icon}
          label={action.tooltip}
          shortcut={action.shortcut}
          isActive={action.isActive}
          disabled={action.disabled}
          onClick={action.onClick}
        />
      ))}
    </div>
  );
});
