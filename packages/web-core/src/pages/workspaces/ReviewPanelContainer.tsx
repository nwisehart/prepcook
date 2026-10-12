import { useState, useMemo, useCallback, useEffect, useRef } from 'react';

import { FileTree } from '@prepcook/ui/components/FileTree';
import {
  ReviewToolbar,
  type ReviewToolbarAction,
} from '@prepcook/ui/components/ReviewToolbar';
import {
  buildFileTree,
  filterFileTree,
  getExpandedPathsForSearch,
  getAllFolderPaths,
  sortDiffs,
} from '@/shared/lib/fileTreeUtils';
import {
  PERSIST_KEYS,
  usePersistedCollapsedPaths,
  usePersistedExpanded,
} from '@/shared/stores/useUiPreferencesStore';
import { useFileInView } from '@/shared/stores/useFileInViewStore';
import {
  useDiffs,
  useShowGitHubComments,
  useSetShowGitHubComments,
  useGetGitHubCommentCountForFile,
  useGetFilesWithGitHubComments,
  useGetFirstCommentLineForFile,
  useIsGitHubCommentsLoading,
} from '@/shared/stores/useWorkspaceDiffStore';
import { useChangesView } from '@/shared/hooks/useChangesView';
import { useActions } from '@/shared/hooks/useActions';
import { useActionVisibilityContext } from '@/shared/hooks/useActionVisibilityContext';
import { useIsMobile } from '@/shared/hooks/useIsMobile';
import { ReviewToolbarActions } from '@/shared/actions';
import {
  getActionIcon,
  getActionTooltip,
  isActionActive,
  isActionEnabled,
  isSpecialIcon,
} from '@/shared/types/actions';
import { getFileIcon } from '@/shared/lib/fileTypeIcon';
import { useTheme } from '@/shared/hooks/useTheme';
import { getActualTheme } from '@/shared/lib/theme';
import { cn } from '@/shared/lib/utils';
import type { Diff } from 'shared/types';
import { ChangesPanelContainer } from './ChangesPanelContainer';

/**
 * State for the file-tree navigator: search, folder collapse, GitHub comment
 * navigation, and keeping the file in view highlighted and scrolled to.
 */
function useFileTreeNavigator(workspaceId: string, diffs: Diff[]) {
  const [searchQuery, setSearchQuery] = useState('');
  const [collapsedPaths, setCollapsedPaths] =
    usePersistedCollapsedPaths(workspaceId);
  const showGitHubComments = useShowGitHubComments();
  const setShowGitHubComments = useSetShowGitHubComments();
  const getGitHubCommentCountForFile = useGetGitHubCommentCountForFile();
  const getFilesWithGitHubComments = useGetFilesWithGitHubComments();
  const getFirstCommentLineForFile = useGetFirstCommentLineForFile();
  const isGitHubCommentsLoading = useIsGitHubCommentsLoading();

  const { selectedFilePath, scrollToFile } = useChangesView();
  const fileInView = useFileInView();
  const activeFilePath = fileInView ?? selectedFilePath;
  const treeScrollRef = useRef<HTMLDivElement | null>(null);
  const treeScrollCallbackRef = useCallback((el: HTMLDivElement | null) => {
    treeScrollRef.current = el;
  }, []);

  useEffect(() => {
    if (!activeFilePath || !treeScrollRef.current) return;
    const container = treeScrollRef.current;
    const selector = `[data-tree-path="${CSS.escape(activeFilePath)}"]`;
    const node = container.querySelector(selector);
    if (!(node instanceof HTMLElement)) return;

    const scrollNodeIntoView = () => {
      const cRect = container.getBoundingClientRect();
      const nRect = node.getBoundingClientRect();
      if (nRect.top < cRect.top) {
        container.scrollTop += nRect.top - cRect.top - 4;
      } else if (nRect.bottom > cRect.bottom) {
        container.scrollTop += nRect.bottom - cRect.bottom + 4;
      }
    };

    scrollNodeIntoView();
    // Retry once after rAF — content-visibility reflow may shift positions
    const rafId = requestAnimationFrame(scrollNodeIntoView);
    return () => cancelAnimationFrame(rafId);
  }, [activeFilePath]);

  const fullTree = useMemo(() => buildFileTree(diffs), [diffs]);
  const allFolderPaths = useMemo(() => getAllFolderPaths(fullTree), [fullTree]);

  // All folders are expanded when none are in the collapsed set
  const isAllExpanded = collapsedPaths.size === 0;

  const filteredTree = useMemo(
    () => filterFileTree(fullTree, searchQuery),
    [fullTree, searchQuery]
  );

  // Auto-expand folders when searching (remove from collapsed set)
  const collapsedPathsRef = useRef(collapsedPaths);
  collapsedPathsRef.current = collapsedPaths;

  useEffect(() => {
    if (searchQuery) {
      const pathsToExpand = getExpandedPathsForSearch(fullTree, searchQuery);
      const next = new Set(collapsedPathsRef.current);
      pathsToExpand.forEach((p) => next.delete(p));
      setCollapsedPaths(next);
    }
  }, [searchQuery, fullTree, setCollapsedPaths]);

  const handleToggleExpand = useCallback(
    (path: string) => {
      setCollapsedPaths((prev) => {
        const next = new Set(prev);
        if (next.has(path)) {
          next.delete(path);
        } else {
          next.add(path);
        }
        return next;
      });
    },
    [setCollapsedPaths]
  );

  const handleToggleExpandAll = useCallback(() => {
    setCollapsedPaths(isAllExpanded ? new Set(allFolderPaths) : new Set());
  }, [isAllExpanded, allFolderPaths, setCollapsedPaths]);

  const handleSelectFile = useCallback(
    (path: string) => {
      const diff = diffs.find((d) => d.newPath === path || d.oldPath === path);

      if (diff) {
        const targetPath = diff.newPath || diff.oldPath || path;
        scrollToFile(targetPath);
      }
    },
    [diffs, scrollToFile]
  );

  // Diff paths that have GitHub comments, in the diff list's visual order
  const filesWithComments = useMemo(() => {
    const ghFiles = getFilesWithGitHubComments();
    return sortDiffs(diffs)
      .map((d) => d.newPath || d.oldPath || '')
      .filter((diffPath) =>
        ghFiles.some(
          (ghPath) => diffPath === ghPath || diffPath.endsWith('/' + ghPath)
        )
      );
  }, [getFilesWithGitHubComments, diffs]);

  const handleNavigateComments = useCallback(
    (direction: 'prev' | 'next') => {
      if (filesWithComments.length === 0) return;

      const currentIndex = activeFilePath
        ? filesWithComments.indexOf(activeFilePath)
        : -1;
      let nextIndex: number;

      if (direction === 'next') {
        nextIndex =
          currentIndex < filesWithComments.length - 1 ? currentIndex + 1 : 0;
      } else {
        nextIndex =
          currentIndex > 0 ? currentIndex - 1 : filesWithComments.length - 1;
      }

      const targetPath = filesWithComments[nextIndex];
      const lineNumber = getFirstCommentLineForFile(targetPath);

      scrollToFile(targetPath, lineNumber ?? undefined);
    },
    [
      filesWithComments,
      activeFilePath,
      getFirstCommentLineForFile,
      scrollToFile,
    ]
  );

  return {
    searchQuery,
    setSearchQuery,
    collapsedPaths,
    filteredTree,
    isAllExpanded,
    activeFilePath,
    treeScrollCallbackRef,
    handleToggleExpand,
    handleToggleExpandAll,
    handleSelectFile,
    showGitHubComments,
    setShowGitHubComments,
    getGitHubCommentCountForFile,
    isGitHubCommentsLoading,
    handleNavigateComments,
    hasFilesWithComments: filesWithComments.length > 0,
  };
}

/** Diff display controls for the toolbar, resolved through the action system */
function useReviewToolbarActions(): ReviewToolbarAction[] {
  const { executeAction } = useActions();
  const actionCtx = useActionVisibilityContext();

  // Not filtered by isVisible: those predicates key off the desktop panel
  // mode, while this toolbar only renders where the Review tab is showing
  // (including the mobile Review tab, which does not set that mode).
  return useMemo(
    () =>
      ReviewToolbarActions.flatMap((action) => {
        const icon = getActionIcon(action, actionCtx);
        if (isSpecialIcon(icon)) return [];
        return [
          {
            id: action.id,
            icon,
            tooltip: getActionTooltip(action, actionCtx),
            shortcut: action.shortcut,
            isActive: isActionActive(action, actionCtx),
            disabled: !isActionEnabled(action, actionCtx),
            onClick: () => executeAction(action),
          },
        ];
      }),
    [actionCtx, executeAction]
  );
}

interface ReviewPanelContainerProps {
  workspaceId: string;
  className?: string;
}

/**
 * The Review tab: one toolbar over the workspace's diff list, with the
 * changed-file tree as its fileTree. On desktop the tree sits beside the
 * diffs; on mobile it replaces them until a file is picked.
 */
export function ReviewPanelContainer({
  workspaceId,
  className,
}: ReviewPanelContainerProps) {
  const isMobile = useIsMobile();
  const diffs = useDiffs();
  const { theme } = useTheme();
  const actualTheme = getActualTheme(theme);
  const fileTree = useFileTreeNavigator(workspaceId, diffs);
  const toolbarActions = useReviewToolbarActions();

  const [isDesktopNavigatorVisible, setDesktopNavigatorVisible] =
    usePersistedExpanded(PERSIST_KEYS.reviewNavigator, true);
  const [isMobileNavigatorVisible, setMobileNavigatorVisible] = useState(false);
  const isNavigatorVisible = isMobile
    ? isMobileNavigatorVisible
    : isDesktopNavigatorVisible;

  const handleToggleNavigator = useCallback(() => {
    if (isMobile) {
      setMobileNavigatorVisible((visible) => !visible);
    } else {
      setDesktopNavigatorVisible();
    }
  }, [isMobile, setDesktopNavigatorVisible]);

  const { handleSelectFile } = fileTree;
  const handleSelectFileFromTree = useCallback(
    (path: string) => {
      if (!isMobile) {
        handleSelectFile(path);
        return;
      }
      // The diff list is display:none while the mobile navigator is open and
      // cannot be scrolled; reveal it first, then scroll once it has laid out.
      setMobileNavigatorVisible(false);
      requestAnimationFrame(() => handleSelectFile(path));
    },
    [handleSelectFile, isMobile]
  );

  const renderFileIcon = useCallback(
    (fileName: string) => {
      const FileIcon = getFileIcon(fileName, actualTheme);
      return FileIcon ? <FileIcon size={14} /> : null;
    },
    [actualTheme]
  );

  return (
    <div className={cn('flex flex-col h-full min-h-0 bg-secondary', className)}>
      <ReviewToolbar
        isNavigatorVisible={isNavigatorVisible}
        onToggleNavigator={handleToggleNavigator}
        searchQuery={fileTree.searchQuery}
        onSearchChange={fileTree.setSearchQuery}
        isAllFoldersExpanded={fileTree.isAllExpanded}
        onToggleExpandAllFolders={fileTree.handleToggleExpandAll}
        showGitHubComments={fileTree.showGitHubComments}
        onToggleGitHubComments={fileTree.setShowGitHubComments}
        isGitHubCommentsLoading={fileTree.isGitHubCommentsLoading}
        onNavigateComments={fileTree.handleNavigateComments}
        hasFilesWithComments={fileTree.hasFilesWithComments}
        actions={toolbarActions}
      />
      <div className="flex flex-1 min-h-0">
        {isNavigatorVisible && (
          <div
            className={cn(
              'flex min-h-0 overflow-hidden',
              isMobile ? 'flex-1' : 'w-[260px] shrink-0 border-r'
            )}
          >
            <FileTree
              nodes={fileTree.filteredTree}
              collapsedPaths={fileTree.collapsedPaths}
              onToggleExpand={fileTree.handleToggleExpand}
              selectedPath={fileTree.activeFilePath}
              onSelectFile={handleSelectFileFromTree}
              searchQuery={fileTree.searchQuery}
              renderFileIcon={renderFileIcon}
              scrollContainerRef={fileTree.treeScrollCallbackRef}
              showGitHubComments={fileTree.showGitHubComments}
              getGitHubCommentCountForFile={
                fileTree.getGitHubCommentCountForFile
              }
            />
          </div>
        )}
        {/* Hidden rather than unmounted on mobile so scroll position and
            mounted diffs survive opening the fileTree. */}
        <div
          className={cn(
            'flex-1 min-w-0 min-h-0',
            isMobile && isNavigatorVisible && 'hidden'
          )}
        >
          <ChangesPanelContainer className="" workspaceId={workspaceId} />
        </div>
      </div>
    </div>
  );
}
