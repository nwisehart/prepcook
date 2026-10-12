import { memo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/cn';
import { FileTreeNode, type FileTreeNodeItem } from './FileTreeNode';

export interface FileTreeViewNode extends FileTreeNodeItem {
  children?: FileTreeViewNode[];
}

interface FileTreeProps {
  nodes: FileTreeViewNode[];
  collapsedPaths: Set<string>;
  onToggleExpand: (path: string) => void;
  selectedPath?: string | null;
  onSelectFile?: (path: string) => void;
  renderFileIcon?: (fileName: string) => ReactNode;
  /** Active filter; selects the empty-state message */
  searchQuery: string;
  className?: string;
  /** Whether to show GitHub comment count badges */
  showGitHubComments?: boolean;
  /** Function to get comment count for a file path (handles prefixed paths) */
  getGitHubCommentCountForFile?: (filePath: string) => number;
  scrollContainerRef?: (el: HTMLDivElement | null) => void;
}

const FILE_NODE_STYLE = {
  contentVisibility: 'auto' as const,
  containIntrinsicSize: 'auto 26px',
};

export const FileTree = memo(function FileTree({
  nodes,
  collapsedPaths,
  onToggleExpand,
  selectedPath,
  onSelectFile,
  searchQuery,
  className,
  showGitHubComments,
  getGitHubCommentCountForFile,
  renderFileIcon,
  scrollContainerRef,
}: FileTreeProps) {
  const { t } = useTranslation(['tasks', 'common']);
  const renderNodes = (nodeList: FileTreeViewNode[], depth = 0) => {
    return nodeList.map((node) => (
      <div
        key={node.id}
        style={node.type === 'file' ? FILE_NODE_STYLE : undefined}
      >
        <FileTreeNode
          node={node}
          depth={depth}
          isExpanded={!collapsedPaths.has(node.path)}
          isSelected={selectedPath === node.path}
          onToggle={node.type === 'folder' ? onToggleExpand : undefined}
          onSelect={node.type === 'file' ? onSelectFile : undefined}
          renderFileIcon={renderFileIcon}
          commentCount={
            showGitHubComments
              ? getGitHubCommentCountForFile?.(node.path)
              : undefined
          }
          showCommentBadge={showGitHubComments}
        />
        {node.type === 'folder' &&
          node.children &&
          !collapsedPaths.has(node.path) &&
          renderNodes(node.children, depth + 1)}
      </div>
    ));
  };

  return (
    <div className={cn('flex-1 w-full bg-secondary flex flex-col', className)}>
      <div
        ref={scrollContainerRef}
        className="p-base flex-1 min-h-0 overflow-auto scrollbar-thin scrollbar-thumb-panel scrollbar-track-transparent"
        style={{ contain: 'layout style paint' }}
      >
        {nodes.length > 0 ? (
          renderNodes(nodes)
        ) : (
          <div className="p-base text-low text-sm">
            {searchQuery ? t('common:fileTree.noResults') : 'No changed files'}
          </div>
        )}
      </div>
    </div>
  );
});
