import type { ReactNode } from 'react';
import { PlusIcon, HashIcon, GitPullRequest } from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { PRESET_COLORS } from './ColorPicker';
import { PrBadge, type PrBadgeStatus } from './PrBadge';
import { TAG_COLORS } from './SearchableTagDropdown';

// Re-export for backwards compatibility.
export { PRESET_COLORS, TAG_COLORS };

export interface IssueTagBase {
  id: string;
  name: string;
  color: string;
}

export interface LinkedPullRequest {
  id: string;
  number: number;
  url: string;
  status: PrBadgeStatus;
}

export interface LinkedIssue {
  id: string;
  displayId: string;
  title: string;
}

export interface IssueTagsRowProps<TTag extends IssueTagBase = IssueTagBase> {
  selectedTagIds: string[];
  availableTags: TTag[];
  linkedPrs?: LinkedPullRequest[];
  linkedIssues?: LinkedIssue[];
  onTagsChange: (tagIds: string[]) => void;
  onCreateTag?: (data: { name: string; color: string }) => string;
  renderAddTagControl?: (
    props: IssueTagsRowAddTagControlProps<TTag>
  ) => ReactNode;
  onLinkPr?: () => void;
  disabled?: boolean;
  className?: string;
}

export interface IssueTagsRowAddTagControlProps<
  TTag extends IssueTagBase = IssueTagBase,
> {
  tags: TTag[];
  selectedTagIds: string[];
  onTagToggle: (tagId: string) => void;
  onCreateTag: (data: { name: string; color: string }) => string;
  disabled: boolean;
  trigger: ReactNode;
}

export function IssueTagsRow<TTag extends IssueTagBase>({
  selectedTagIds,
  availableTags,
  linkedPrs = [],
  linkedIssues = [],
  onTagsChange,
  onCreateTag,
  renderAddTagControl,
  onLinkPr,
  disabled,
  className,
}: IssueTagsRowProps<TTag>) {
  const selectedTags = availableTags.filter((tag) =>
    selectedTagIds.includes(tag.id)
  );

  const handleTagToggle = (tagId: string) => {
    if (selectedTagIds.includes(tagId)) {
      onTagsChange(selectedTagIds.filter((id) => id !== tagId));
    } else {
      onTagsChange([...selectedTagIds, tagId]);
    }
  };

  const handleCreateTag = (data: { name: string; color: string }): string => {
    return onCreateTag?.(data) ?? '';
  };

  const addTagTrigger = (
    <button
      type="button"
      className="flex size-[22px] items-center justify-center rounded-full border border-dashed border-border text-low transition-colors hover:border-low/60 hover:text-high disabled:opacity-50"
      disabled={disabled}
      aria-label="Add tag"
    >
      <PlusIcon className="size-3" weight="bold" />
    </button>
  );

  return (
    <div className={cn('flex flex-wrap items-center gap-1.5', className)}>
      {/* Selected Tags - clickable to remove on hover */}
      {selectedTags.map((tag) => (
        <button
          key={tag.id}
          type="button"
          onClick={() => handleTagToggle(tag.id)}
          disabled={disabled}
          className={cn(
            'inline-flex items-center justify-center',
            'h-[22px] gap-1.5 px-2',
            'rounded-full bg-panel',
            'text-xs font-medium leading-none text-normal',
            'whitespace-nowrap',
            'transition-colors',
            !disabled &&
              'cursor-pointer hover:bg-error-soft hover:text-error hover:line-through',
            disabled && 'cursor-default'
          )}
        >
          <span
            className="w-2 h-2 rounded-full shrink-0"
            style={{ backgroundColor: `hsl(${tag.color})` }}
          />
          {tag.name}
        </button>
      ))}

      {/* Linked PRs */}
      {linkedPrs.map((pr) => (
        <PrBadge
          key={pr.id}
          number={pr.number}
          url={pr.url}
          status={pr.status}
        />
      ))}

      {/* Link PR button */}
      {onLinkPr && (
        <button
          type="button"
          onClick={onLinkPr}
          disabled={disabled}
          className="flex size-[22px] items-center justify-center rounded-full border border-dashed border-border text-low transition-colors hover:border-low/60 hover:text-high disabled:opacity-50"
          aria-label="Link pull request"
        >
          <GitPullRequest className="size-3" weight="bold" />
        </button>
      )}

      {/* Linked Issues */}
      {linkedIssues.map((issue) => (
        <button
          key={issue.id}
          type="button"
          className="inline-flex h-[22px] items-center gap-1 rounded-full bg-panel px-2 text-xs font-medium text-normal transition-colors hover:text-high"
          title={issue.title}
        >
          <HashIcon className="size-3" weight="bold" />
          <span>{issue.displayId}</span>
        </button>
      ))}

      {/* Add Tag Dropdown */}
      {onCreateTag &&
        (renderAddTagControl?.({
          tags: availableTags,
          selectedTagIds,
          onTagToggle: handleTagToggle,
          onCreateTag: handleCreateTag,
          disabled: disabled ?? false,
          trigger: addTagTrigger,
        }) ??
          addTagTrigger)}
    </div>
  );
}
