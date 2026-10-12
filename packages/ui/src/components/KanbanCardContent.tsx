'use client';

import type { MouseEvent, ReactNode } from 'react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  CircleDashedIcon,
  DotsThreeIcon,
  FileTextIcon,
  PlusIcon,
} from '@phosphor-icons/react';
import { cn } from '../lib/cn';
import { PriorityIcon, type PriorityLevel } from './PriorityIcon';
import { KanbanBadge } from './KanbanBadge';
import { KanbanAssignee, type KanbanAssigneeUser } from './KanbanAssignee';
import { RunningDots } from './RunningDots';
import { PrBadge, type PrBadgeStatus } from './PrBadge';
import {
  RelationshipBadge,
  type RelationshipDisplayType,
} from './RelationshipBadge';

export interface KanbanTag {
  id: string;
  name: string;
  color: string;
}

export interface KanbanRelationship {
  relationshipId: string;
  displayType: RelationshipDisplayType;
  relatedIssueDisplayId: string;
}

export interface KanbanPullRequest {
  id: string;
  number: number;
  url: string;
  status: PrBadgeStatus;
}

export interface TagEditRenderProps<TTag extends KanbanTag = KanbanTag> {
  allTags: TTag[];
  selectedTagIds: string[];
  onTagToggle: (tagId: string) => void;
  onCreateTag: (data: { name: string; color: string }) => string;
  trigger: ReactNode;
}

export interface TagEditProps<TTag extends KanbanTag = KanbanTag> {
  allTags: TTag[];
  selectedTagIds: string[];
  onTagToggle: (tagId: string) => void;
  onCreateTag: (data: { name: string; color: string }) => string;
  renderTagEditor?: (props: TagEditRenderProps<TTag>) => ReactNode;
}

const IMAGE_FILE_EXTENSION_REGEX =
  /\.(png|jpe?g|gif|webp|bmp|svg|avif|heic|heif)$/i;

function isImageLikeAttachmentName(name: string): boolean {
  const normalized = name.trim();
  if (!normalized) {
    return false;
  }

  return IMAGE_FILE_EXTENSION_REGEX.test(normalized);
}

function formatKanbanDescriptionPreview(
  markdown: string,
  options: {
    codeBlockLabel: string;
    imageLabel: string;
    imageWithNameLabel: (name: string) => string;
    fileLabel: string;
    fileWithNameLabel: (name: string) => string;
  }
): string {
  return markdown
    .replace(/```[\s\S]*?```/g, options.codeBlockLabel)
    .replace(
      /!\[([^\]]*)\]\(([^)]+)\)/g,
      (_match, altText: string, url: string) => {
        const normalizedAlt = altText.trim();
        const normalizedUrl = url.trim();
        const isImageAttachment =
          normalizedUrl.startsWith('attachment://') &&
          isImageLikeAttachmentName(normalizedAlt);

        if (isImageAttachment) {
          return normalizedAlt
            ? options.imageWithNameLabel(normalizedAlt)
            : options.imageLabel;
        }

        return normalizedAlt
          ? options.fileWithNameLabel(normalizedAlt)
          : options.fileLabel;
      }
    )
    .replace(
      /(?<!!)\[([^\]]*)\]\((attachment:\/\/[^)]+|\.vibe-attachments\/[^)]+)\)/g,
      (_match, label: string) => {
        const normalizedLabel = label.trim();
        return normalizedLabel
          ? options.fileWithNameLabel(normalizedLabel)
          : options.fileLabel;
      }
    )
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/^\s*>\s?/gm, '')
    .replace(/^\s*([-*+]|\d+\.)\s+/gm, '')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/__([^_]+)__/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/_([^_]+)_/g, '$1')
    .replace(/~~([^~]+)~~/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}

export type KanbanCardContentProps<TTag extends KanbanTag = KanbanTag> = {
  displayId: string;
  title: string;
  description?: string | null;
  priority: PriorityLevel | null;
  tags: KanbanTag[];
  assignees: KanbanAssigneeUser[];
  pullRequests?: KanbanPullRequest[];
  relationships?: KanbanRelationship[];
  isSubIssue?: boolean;
  isLoading?: boolean;
  className?: string;
  onPriorityClick?: (e: MouseEvent) => void;
  onAssigneeClick?: (e: MouseEvent) => void;
  onMoreActionsClick?: () => void;
  tagEditProps?: TagEditProps<TTag>;
  isMobile?: boolean;
  /** Jira key of the parent epic (subtask cards): renders an epic chip. */
  epicKey?: string | null;
  /** Deterministic accent color derived from the epic key. */
  epicColor?: string;
  /** Subtask completion for epic cards: renders a progress bar. */
  epicProgress?: { done: number; total: number };
  /** URL of the approved plan/spec artifact: renders a Plan chip. */
  specUrl?: string | null;
  /** MR publication state while a ticket sits in the Push MR column. */
  pushState?: 'pushing' | 'pushed' | 'failed' | null;
  /** MR URL once created — fallback Pushed link when no PR badge renders. */
  mrUrl?: string | null;
  /** Orchestrator automation is about to pick this ticket up: renders a
   * pulsing "Agent queued" chip so column moves give instant feedback. */
  agentQueued?: boolean;
};

export function KanbanCardContent<TTag extends KanbanTag = KanbanTag>({
  displayId,
  title,
  description,
  priority,
  tags,
  assignees,
  pullRequests = [],
  relationships = [],
  isSubIssue,
  isLoading = false,
  className,
  onPriorityClick,
  onAssigneeClick,
  onMoreActionsClick,
  tagEditProps,
  isMobile,
  epicKey,
  epicColor,
  epicProgress,
  specUrl,
  pushState,
  mrUrl,
  agentQueued,
}: KanbanCardContentProps<TTag>) {
  const { t } = useTranslation('common');
  const previewDescription = useMemo(() => {
    if (!description) {
      return null;
    }

    const formatted = formatKanbanDescriptionPreview(description, {
      codeBlockLabel: t('kanban.previewCodeBlock'),
      imageLabel: t('kanban.previewImage'),
      imageWithNameLabel: (name: string) =>
        t('kanban.previewImageWithName', { name }),
      fileLabel: t('kanban.previewFile'),
      fileWithNameLabel: (name: string) =>
        t('kanban.previewFileWithName', { name }),
    });
    return formatted.length > 0 ? formatted : null;
  }, [description, t]);

  const tagsDisplay = (
    <>
      {tags.slice(0, 2).map((tag) => (
        <KanbanBadge key={tag.id} name={tag.name} color={tag.color} />
      ))}
      {tags.length > 2 && (
        <span className="text-sm text-low">+{tags.length - 2}</span>
      )}
      {tagEditProps && tags.length === 0 && (
        <PlusIcon className="size-icon-xs text-low" weight="bold" />
      )}
    </>
  );
  const tagEditorTrigger = (
    <button
      type="button"
      onClick={(e) => e.stopPropagation()}
      className="flex items-center gap-half cursor-pointer hover:bg-secondary rounded-sm transition-colors"
    >
      {tagsDisplay}
    </button>
  );

  return (
    <div className={cn('flex flex-col gap-half min-w-0', className)}>
      {/* Row 1: Task ID + sub-issue indicator + loading dots + more actions */}
      <div className="flex items-center justify-between gap-half">
        <div className="flex items-center gap-half min-w-0">
          {isSubIssue && (
            <span className="text-sm text-low">
              {t('kanban.subIssueIndicator')}
            </span>
          )}
          <span className="font-ibm-plex-mono text-sm text-low truncate">
            {displayId}
          </span>
          {epicKey && (
            <span
              className="inline-flex items-center gap-1 rounded-sm px-1 text-xs text-low border border-border shrink-0"
              title={epicKey}
            >
              <span
                className="h-1.5 w-1.5 rounded-full shrink-0"
                style={epicColor ? { backgroundColor: epicColor } : undefined}
              />
              {epicKey}
            </span>
          )}
          {isLoading && <RunningDots />}
        </div>
        {onMoreActionsClick && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onMoreActionsClick();
            }}
            onMouseDown={(e) => e.stopPropagation()}
            className={cn(
              'p-half -m-half rounded-sm text-low hover:text-normal hover:bg-secondary shrink-0',
              isMobile
                ? ''
                : 'invisible opacity-0 group-hover:visible group-hover:opacity-100',
              'transition-[opacity,color,background-color]'
            )}
            aria-label="More actions"
            title="More actions"
          >
            <DotsThreeIcon className="size-icon-xs" weight="bold" />
          </button>
        )}
      </div>

      {/* Row 2: Title */}
      <span className="text-base text-normal truncate">{title}</span>

      {/* Row 3: Description (optional, truncated) */}
      {previewDescription && (
        <p
          className={cn(
            'text-sm text-low m-0',
            isMobile
              ? 'leading-tight line-clamp-2'
              : 'leading-relaxed line-clamp-4'
          )}
        >
          {previewDescription}
        </p>
      )}

      {/* Row 4: Priority + Assignee */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-half min-w-0">
          {onPriorityClick ? (
            <button
              type="button"
              onClick={onPriorityClick}
              onMouseDown={(e) => e.stopPropagation()}
              className="flex items-center cursor-pointer hover:bg-secondary rounded-sm transition-colors"
            >
              <PriorityIcon priority={priority} />
              {!priority && (
                <CircleDashedIcon
                  className="size-icon-xs text-low"
                  weight="bold"
                />
              )}
            </button>
          ) : (
            <PriorityIcon priority={priority} />
          )}
        </div>
        {onAssigneeClick ? (
          <button
            type="button"
            onClick={onAssigneeClick}
            onMouseDown={(e) => e.stopPropagation()}
            className="cursor-pointer hover:bg-secondary rounded-sm transition-colors"
          >
            <KanbanAssignee assignees={assignees} />
          </button>
        ) : (
          <KanbanAssignee assignees={assignees} />
        )}
      </div>

      {/* Row 5: Tags, Plan, Push state, PRs, Relationships (own row) */}
      {(tags.length > 0 ||
        tagEditProps ||
        specUrl ||
        pushState ||
        agentQueued ||
        pullRequests.length > 0 ||
        relationships.length > 0) && (
        <div className="flex items-center gap-half flex-wrap min-w-0">
          {tagEditProps ? (
            (tagEditProps.renderTagEditor?.({
              allTags: tagEditProps.allTags,
              selectedTagIds: tagEditProps.selectedTagIds,
              onTagToggle: tagEditProps.onTagToggle,
              onCreateTag: tagEditProps.onCreateTag,
              trigger: tagEditorTrigger,
            }) ?? tagEditorTrigger)
          ) : (
            <>
              {tags.slice(0, 2).map((tag) => (
                <KanbanBadge key={tag.id} name={tag.name} color={tag.color} />
              ))}
              {tags.length > 2 && (
                <span className="text-sm text-low">+{tags.length - 2}</span>
              )}
            </>
          )}
          {agentQueued && (
            <span
              className="flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-brand/10 text-brand animate-pulse shrink-0"
              title={t('kanban.agentQueuedHint')}
            >
              {t('kanban.agentQueued')}
            </span>
          )}
          {pushState === 'pushing' && (
            <span className="flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-brand/10 text-brand animate-pulse shrink-0">
              {t('kanban.pushStates.pushing')}
            </span>
          )}
          {/* Once the MR exists the PR badge is the status: only fall back
              to a Pushed chip when no PR badge will render the link. */}
          {pushState === 'pushed' && mrUrl && pullRequests.length === 0 && (
            <a
              href={mrUrl}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
              className="flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-success/10 text-success hover:bg-success/20 transition-colors shrink-0"
            >
              {t('kanban.pushStates.pushed')}
            </a>
          )}
          {pushState === 'failed' && (
            <span
              className="flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-error/10 text-error shrink-0"
              title={t('kanban.pushStates.failedHint')}
            >
              {t('kanban.pushStates.failed')}
            </span>
          )}
          {specUrl && (
            <a
              href={specUrl}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
              className="flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-brand/10 text-brand hover:bg-brand/20 transition-colors"
              title={t('kanban.openApprovedPlan')}
            >
              <FileTextIcon className="size-icon-2xs" weight="bold" />
              <span>{t('kanban.planChip')}</span>
            </a>
          )}
          {pullRequests.slice(0, 2).map((pr) => (
            <PrBadge
              key={pr.id}
              number={pr.number}
              url={pr.url}
              status={pr.status}
            />
          ))}
          {pullRequests.length > 2 && (
            <span className="text-sm text-low">+{pullRequests.length - 2}</span>
          )}
          {relationships.slice(0, 2).map((rel) => (
            <RelationshipBadge
              key={rel.relationshipId}
              displayType={rel.displayType}
              relatedIssueDisplayId={rel.relatedIssueDisplayId}
              compact
            />
          ))}
          {relationships.length > 2 && (
            <span className="text-sm text-low">
              +{relationships.length - 2}
            </span>
          )}
        </div>
      )}

      {/* Row 6: Epic subtask progress (epic cards only) */}
      {epicProgress && epicProgress.total > 0 && (
        <div className="flex items-center gap-half">
          <div className="flex-1 h-1 rounded-full bg-secondary overflow-hidden">
            <div
              className="h-full rounded-full transition-[width]"
              style={{
                width: `${Math.round((epicProgress.done / epicProgress.total) * 100)}%`,
                backgroundColor: epicColor ?? 'hsl(var(--brand))',
              }}
            />
          </div>
          <span className="text-xs text-low shrink-0">
            {epicProgress.done}/{epicProgress.total}
          </span>
        </div>
      )}
    </div>
  );
}
