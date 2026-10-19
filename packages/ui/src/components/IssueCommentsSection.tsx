import type { Ref, ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { LocalAttachmentMetadata } from './WorkspaceContext';
import { cn } from '../lib/cn';
import {
  DotsThreeIcon,
  SmileyIcon,
  PencilSimpleIcon,
  TrashIcon,
  ArrowBendUpLeftIcon,
  PaperclipIcon,
} from '@phosphor-icons/react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './RadixTooltip';
import { ErrorAlert } from './ErrorAlert';
import { UserAvatar, type UserAvatarUser } from './UserAvatar';
import { IssuePanelSection } from './IssuePanelSection';
import { formatRelativeTime } from './KanbanRelativeTime';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from './Dropdown';
import { EmojiPicker } from './EmojiPicker';

export interface IssueCommentData {
  id: string;
  authorId: string | null;
  authorName: string;
  message: string;
  createdAt: string;
  author?: UserAvatarUser | null;
  canModify: boolean;
}

export interface ReactionGroup {
  emoji: string;
  count: number;
  hasReacted: boolean;
  reactionId: string | undefined;
  userNames: string[];
}

interface DropzoneProps {
  getRootProps: () => Record<string, unknown>;
  getInputProps: () => Record<string, unknown>;
  isDragActive: boolean;
}

export interface IssueCommentsEditorProps {
  value: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  localAttachments?: LocalAttachmentMetadata[];
  onCmdEnter?: () => void;
  onPasteFiles?: (files: File[]) => void;
  editorRef?: Ref<unknown>;
}

interface IssueCommentsSectionProps {
  comments: IssueCommentData[];
  commentInput: string;
  onCommentInputChange: (value: string) => void;
  onSubmitComment: () => void;
  editingCommentId: string | null;
  editingValue: string;
  onEditingValueChange: (value: string) => void;
  onStartEdit: (commentId: string) => void;
  onSaveEdit: () => void;
  onCancelEdit: () => void;
  onDeleteComment: (id: string) => void;
  reactionsByCommentId: Map<string, ReactionGroup[]>;
  onToggleReaction: (commentId: string, emoji: string) => void;
  onReply: (authorName: string, message: string) => void;
  isLoading?: boolean;
  commentEditorRef?: Ref<unknown>;
  onPasteFiles?: (files: File[]) => void;
  localAttachments?: LocalAttachmentMetadata[];
  dropzoneProps?: DropzoneProps;
  onBrowseAttachment?: () => void;
  isUploading?: boolean;
  attachmentError?: string | null;
  onDismissAttachmentError?: () => void;
  renderEditor: (props: IssueCommentsEditorProps) => ReactNode;
}

export function IssueCommentsSection({
  comments,
  commentInput,
  onCommentInputChange,
  onSubmitComment,
  editingCommentId,
  editingValue,
  onEditingValueChange,
  onStartEdit,
  onSaveEdit,
  onCancelEdit,
  onDeleteComment,
  reactionsByCommentId,
  onToggleReaction,
  onReply,
  isLoading,
  commentEditorRef,
  onPasteFiles,
  localAttachments,
  dropzoneProps,
  onBrowseAttachment,
  isUploading,
  attachmentError,
  onDismissAttachmentError,
  renderEditor,
}: IssueCommentsSectionProps) {
  const { t } = useTranslation('common');

  return (
    <IssuePanelSection
      label={t('kanban.panel.activity')}
      persistKey="kanban-issue-comments"
      collapsible
    >
      <div className="flex flex-col gap-1">
        {/* Timeline */}
        {isLoading ? (
          <div className="flex animate-pulse flex-col gap-2 py-1.5">
            <div className="h-3.5 w-3/4 rounded bg-panel" />
            <div className="h-3.5 w-1/2 rounded bg-panel" />
          </div>
        ) : comments.length === 0 ? (
          <p className="m-0 py-1.5 text-[13px] text-low">
            {t('kanban.noCommentsYet')}
          </p>
        ) : (
          comments.map((comment) => (
            <CommentItem
              key={comment.id}
              comment={comment}
              isEditing={editingCommentId === comment.id}
              editValue={editingCommentId === comment.id ? editingValue : ''}
              onEditValueChange={onEditingValueChange}
              onStartEdit={() => onStartEdit(comment.id)}
              onSaveEdit={onSaveEdit}
              onCancelEdit={onCancelEdit}
              onDelete={() => onDeleteComment(comment.id)}
              reactions={reactionsByCommentId.get(comment.id) ?? []}
              onToggleReaction={(emoji) => onToggleReaction(comment.id, emoji)}
              onReply={() => onReply(comment.authorName, comment.message)}
              renderEditor={renderEditor}
            />
          ))
        )}

        {/* Composer, with file drop */}
        <div
          {...dropzoneProps?.getRootProps()}
          className="relative mt-1 flex flex-col gap-2 rounded border border-border bg-surface-raised px-3 py-2 focus-within:border-brand"
        >
          <input {...dropzoneProps?.getInputProps()} />
          {renderEditor({
            value: commentInput,
            onChange: onCommentInputChange,
            placeholder: t('kanban.panel.commentPlaceholder'),
            className: 'min-h-[20px] text-[13px]',
            localAttachments,
            onCmdEnter: onSubmitComment,
            onPasteFiles,
            autoFocus: false,
            editorRef: commentEditorRef,
          })}
          {attachmentError && (
            <ErrorAlert
              message={attachmentError}
              onDismiss={onDismissAttachmentError}
              dismissLabel={t('buttons.close')}
            />
          )}
          <div className="flex items-center justify-end gap-1">
            {onBrowseAttachment && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={onBrowseAttachment}
                      className="flex size-7 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high"
                      aria-label={t('kanban.attachFile')}
                    >
                      <PaperclipIcon className="size-3.5" />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>{t('kanban.attachFileHint')}</TooltipContent>
                </Tooltip>
              </TooltipProvider>
            )}
            <button
              type="button"
              onClick={onSubmitComment}
              disabled={!commentInput.trim() || isUploading}
              className="h-7 rounded-sm px-2 text-[13px] font-semibold text-brand-secondary transition-colors hover:bg-panel disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t('kanban.panel.post')}
            </button>
          </div>
          {dropzoneProps?.isDragActive && (
            <div className="absolute inset-0 z-50 flex items-center justify-center rounded border-2 border-dashed border-brand bg-primary/80 backdrop-blur-sm">
              <p className="text-sm font-medium text-high">
                {t('kanban.dropFilesHere')}
              </p>
            </div>
          )}
        </div>
      </div>
    </IssuePanelSection>
  );
}

interface CommentItemProps {
  comment: IssueCommentData;
  isEditing: boolean;
  editValue: string;
  onEditValueChange: (value: string) => void;
  onStartEdit: () => void;
  onSaveEdit: () => void;
  onCancelEdit: () => void;
  onDelete: () => void;
  reactions: ReactionGroup[];
  onToggleReaction: (emoji: string) => void;
  onReply: () => void;
  renderEditor: (props: IssueCommentsEditorProps) => ReactNode;
}

function CommentItem({
  comment,
  isEditing,
  editValue,
  onEditValueChange,
  onStartEdit,
  onSaveEdit,
  onCancelEdit,
  onDelete,
  reactions,
  onToggleReaction,
  onReply,
  renderEditor,
}: CommentItemProps) {
  const { t } = useTranslation('common');
  const timeAgo = formatRelativeTime(t, comment.createdAt);

  return (
    <div className="group/comment flex items-start gap-2.5 py-1.5 text-[13px]">
      {comment.author ? (
        <UserAvatar user={comment.author} className="mt-px size-5 text-[9px]" />
      ) : (
        <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-full bg-panel text-[10px] font-bold text-low">
          {comment.authorName.charAt(0).toUpperCase()}
        </span>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {/* Who and when */}
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate font-semibold text-high">
            {comment.authorName}
          </span>
          <span className="shrink-0 text-low">
            {t('kanban.panel.commented')}
          </span>
          <span className="ml-auto shrink-0 pl-2 text-xs text-low">
            {timeAgo}
          </span>
          {comment.canModify && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  className="flex size-5 shrink-0 items-center justify-center rounded-sm text-low hover:bg-panel hover:text-high"
                  aria-label={t('kanban.moreActions')}
                >
                  <DotsThreeIcon className="size-3.5" weight="bold" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  icon={PencilSimpleIcon}
                  onSelect={onStartEdit}
                >
                  {t('buttons.edit')}
                </DropdownMenuItem>
                <DropdownMenuItem
                  icon={TrashIcon}
                  variant="destructive"
                  onSelect={onDelete}
                >
                  {t('buttons.delete')}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>

        {/* Message - editable or read-only */}
        {isEditing ? (
          <div className="flex flex-col gap-2 rounded border border-border bg-secondary px-3 py-2">
            {renderEditor({
              value: editValue,
              onChange: onEditValueChange,
              autoFocus: true,
              onCmdEnter: onSaveEdit,
              className: 'min-h-[40px] text-[13px]',
            })}
            <div className="flex justify-end gap-1.5">
              <button
                type="button"
                onClick={onCancelEdit}
                className="h-7 rounded-sm px-2.5 text-[13px] font-semibold text-normal hover:bg-panel"
              >
                {t('buttons.cancel')}
              </button>
              <button
                type="button"
                onClick={onSaveEdit}
                disabled={!editValue.trim()}
                className="h-7 rounded-sm bg-brand px-2.5 text-[13px] font-semibold text-on-brand hover:bg-brand-hover disabled:opacity-50"
              >
                {t('buttons.save')}
              </button>
            </div>
          </div>
        ) : (
          renderEditor({
            value: comment.message,
            disabled: true,
            className: 'text-[13px] text-normal',
          })
        )}

        {/* Reactions and reply */}
        <div className="flex flex-wrap items-center gap-1.5">
          <TooltipProvider>
            {reactions.map((reaction) => (
              <Tooltip key={reaction.emoji}>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    onClick={() => onToggleReaction(reaction.emoji)}
                    className={cn(
                      'inline-flex h-[22px] items-center gap-1 rounded-full border px-2 text-xs transition-colors',
                      reaction.hasReacted
                        ? 'border-brand/40 bg-brand-soft text-brand-soft-foreground'
                        : 'border-border bg-secondary text-low hover:text-high'
                    )}
                  >
                    <span className="color-emoji">{reaction.emoji}</span>
                    <span>{reaction.count}</span>
                  </button>
                </TooltipTrigger>
                <TooltipContent>{reaction.userNames.join(', ')}</TooltipContent>
              </Tooltip>
            ))}
          </TooltipProvider>

          <span
            className={cn(
              'flex items-center gap-1 transition-opacity',
              reactions.length === 0 &&
                'opacity-0 focus-within:opacity-100 group-hover/comment:opacity-100'
            )}
          >
            <EmojiPicker onSelect={onToggleReaction}>
              <button
                type="button"
                className="flex size-6 items-center justify-center rounded-sm text-low transition-colors hover:bg-panel hover:text-high"
                aria-label={t('kanban.panel.react')}
              >
                <SmileyIcon className="size-3.5" />
              </button>
            </EmojiPicker>
            <button
              type="button"
              onClick={onReply}
              className="flex h-6 items-center gap-1 rounded-sm px-1.5 text-xs text-low transition-colors hover:bg-panel hover:text-high"
            >
              <ArrowBendUpLeftIcon className="size-3.5" />
              {t('buttons.reply')}
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}
