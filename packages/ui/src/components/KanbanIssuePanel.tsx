import {
  useState,
  useEffect,
  useRef,
  useCallback,
  type ReactNode,
  type RefObject,
} from 'react';
import { useTranslation } from 'react-i18next';
import type { LocalAttachmentMetadata } from './WorkspaceContext';
import { cn } from '../lib/cn';
import {
  XIcon,
  LinkIcon,
  DotsThreeIcon,
  TrashIcon,
  PaperclipIcon,
  ImageIcon,
  StampIcon,
} from '@phosphor-icons/react';
import {
  IssueTagsRow,
  type IssueTagBase,
  type IssueTagsRowAddTagControlProps,
  type LinkedPullRequest as IssueTagsLinkedPullRequest,
} from './IssueTagsRow';
import { PrimaryButton } from './PrimaryButton';
import { Toggle } from './Toggle';
import {
  IssuePropertyRow,
  type IssuePropertyRowProps,
} from './IssuePropertyRow';
import { IconButton } from './IconButton';
import { AutoResizeTextarea } from './AutoResizeTextarea';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './RadixTooltip';
import { ErrorAlert } from './ErrorAlert';

export type IssuePanelMode = 'create' | 'edit';
type IssuePriority = IssuePropertyRowProps['priority'];
type IssueStatus = IssuePropertyRowProps['statuses'][number];
type IssueAssignee = NonNullable<
  IssuePropertyRowProps['assigneeUsers']
>[number];
type IssueCreator = Exclude<IssuePropertyRowProps['creatorUser'], undefined>;
export interface KanbanIssueTag extends IssueTagBase {
  project_id: string;
}

export interface IssueFormData {
  title: string;
  description: string | null;
  statusId: string;
  priority: IssuePriority | null;
  assigneeIds: string[];
  tagIds: string[];
  createDraftWorkspace: boolean;
}

export interface LinkedPullRequest extends IssueTagsLinkedPullRequest {}

export interface KanbanIssueDescriptionEditorProps {
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  onCmdEnter?: () => void;
  onPasteFiles?: (files: File[]) => void;
  disabled?: boolean;
  autoFocus?: boolean;
  className?: string;
  localAttachments?: LocalAttachmentMetadata[];
  showStaticToolbar?: boolean;
  saveStatus?: 'idle' | 'saved';
  staticToolbarActions?: ReactNode;
  onRequestEdit?: () => void;
  hideActions?: boolean;
}

export interface KanbanIssuePanelProps {
  mode: IssuePanelMode;
  displayId: string;

  // Form data
  formData: IssueFormData;
  onFormChange: <K extends keyof IssueFormData>(
    field: K,
    value: IssueFormData[K]
  ) => void;

  // Options for dropdowns
  statuses: IssueStatus[];
  tags: KanbanIssueTag[];

  // When true, the status selector is disabled (e.g. create mode forces new
  // issues into a fixed column)
  statusLocked?: boolean;

  // Resolved assignee profiles for avatar display
  assigneeUsers?: IssueAssignee[];

  // Edit mode data
  issueId?: string | null;
  creatorUser?: IssueCreator;
  parentIssue?: { id: string; simpleId: string } | null;
  onParentIssueClick?: () => void;
  onRemoveParentIssue?: () => void;
  linkedPrs?: LinkedPullRequest[];
  onLinkPr?: () => void;

  // Actions
  onClose: () => void;
  onSubmit: () => void;
  onCmdEnterSubmit?: () => void;
  onDeleteDraft?: () => void;

  // Tag create callback - returns the new tag ID
  onCreateTag?: (data: { name: string; color: string }) => string;
  renderAddTagControl?: (
    props: IssueTagsRowAddTagControlProps<KanbanIssueTag>
  ) => ReactNode;
  renderDescriptionEditor: (
    props: KanbanIssueDescriptionEditorProps
  ) => ReactNode;

  // Loading states
  isSubmitting?: boolean;

  // Save status for description field
  descriptionSaveStatus?: 'idle' | 'saved';

  // Ref for title input (created in container)
  titleInputRef: RefObject<HTMLTextAreaElement>;

  // Copy link callback (edit mode only)
  onCopyLink?: () => void;

  // More actions callback (edit mode only) - opens command bar with issue actions
  onMoreActions?: () => void;

  // Image attachment upload
  onPasteFiles?: (files: File[]) => void;
  localAttachments?: LocalAttachmentMetadata[];
  dropzoneProps?: {
    getRootProps: () => Record<string, unknown>;
    getInputProps: () => Record<string, unknown>;
    isDragActive: boolean;
  };
  onBrowseAttachment?: () => void;
  isUploading?: boolean;
  attachmentError?: string | null;
  onDismissAttachmentError?: () => void;

  // Stage playbook action (edit mode only): starts an agent workspace with
  // the playbook prompt for the issue's work-class and current column.
  stageAction?: {
    label: string;
    onRun: () => void;
    disabled?: boolean;
  };

  // Spec/RCA artifact state from the issue's extension_metadata (edit mode
  // only): shows the artifact path and whether the Lavish review gate has
  // approved it.
  specStatus?: {
    path: string | null;
    approvedAt: string | null;
  };

  // Planning verdict recorded by the agent (edit mode only): kind (spec/rca),
  // confidence, open questions, summary. The other half of the executing
  // gate next to the Lavish approval.
  planningVerdict?: {
    kind: string | null;
    confidence: string | null;
    openQuestions: number | null;
    summary: string | null;
    // Agent-recorded reason confidence stayed below high (one sentence).
    confidenceReason: string | null;
  };

  // Human-takeover pause marker (edit mode only): a teammate assigned
  // themselves the Jira ticket, so automation is parked on this one.
  humanTakeover?: {
    assignee: string | null;
  };

  // Live Lavish review-gate URL for the issue's implementation workspace
  // (edit mode only): renders a prominent "Review plan" button while the
  // gate is open.
  reviewUrl?: string | null;

  // Re-opens a dead review gate for the unapproved spec (edit mode only,
  // offered when no live session exists). `reopeningReview` keeps the
  // button disabled until the new session shows up.
  onReopenReview?: () => void;
  reopeningReview?: boolean;

  // Edit-mode section renderers
  renderWorkspacesSection?: (issueId: string) => ReactNode;
  renderRelationshipsSection?: (issueId: string) => ReactNode;
  renderSubIssuesSection?: (issueId: string) => ReactNode;
  renderCommentsSection?: (issueId: string) => ReactNode;
}

export function KanbanIssuePanel({
  mode,
  displayId,
  formData,
  onFormChange,
  statuses,
  tags,
  statusLocked,
  assigneeUsers,
  issueId,
  creatorUser,
  parentIssue,
  onParentIssueClick,
  onRemoveParentIssue,
  linkedPrs = [],
  onLinkPr,
  onClose,
  onSubmit,
  onCmdEnterSubmit,
  onDeleteDraft,
  onCreateTag,
  renderAddTagControl,
  renderDescriptionEditor,
  isSubmitting,
  descriptionSaveStatus,
  titleInputRef,
  onCopyLink,
  onMoreActions,
  onPasteFiles,
  localAttachments,
  dropzoneProps,
  onBrowseAttachment,
  isUploading,
  attachmentError,
  onDismissAttachmentError,
  stageAction,
  specStatus,
  planningVerdict,
  humanTakeover,
  reviewUrl,
  onReopenReview,
  reopeningReview,
  renderWorkspacesSection,
  renderRelationshipsSection,
  renderSubIssuesSection,
  renderCommentsSection,
}: KanbanIssuePanelProps) {
  const { t } = useTranslation('common');
  const isCreateMode = mode === 'create';
  const breadcrumbTextClass =
    'min-w-0 text-sm text-normal truncate rounded-sm px-1 py-0.5 hover:bg-panel hover:text-high transition-colors';
  const creatorName =
    creatorUser?.first_name?.trim() || creatorUser?.username?.trim() || null;
  const showCreator = !isCreateMode && Boolean(creatorName);

  // Description edit state: in edit mode, show preview by default; in create mode, always editable
  const [isDescriptionEditing, setIsDescriptionEditing] =
    useState(isCreateMode);
  const descriptionContainerRef = useRef<HTMLDivElement>(null);

  // Reset description editing state when switching between create/edit mode or when issue changes
  useEffect(() => {
    setIsDescriptionEditing(isCreateMode);
  }, [isCreateMode, issueId]);

  // Click outside the description area to exit editing
  const handleDescriptionBlur = useCallback(() => {
    if (!isCreateMode) {
      setIsDescriptionEditing(false);
    }
  }, [isCreateMode]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      const target = e.target as HTMLElement;
      const isEditable =
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable;
      if (isEditable) {
        // If editing description, exit edit mode first
        if (
          isDescriptionEditing &&
          !isCreateMode &&
          descriptionContainerRef.current?.contains(target)
        ) {
          setIsDescriptionEditing(false);
        }
        target.blur();
        (e.currentTarget as HTMLElement).focus();
        e.stopPropagation();
      } else {
        onClose();
      }
    }
  };

  const handleTitleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      onCmdEnterSubmit?.();
    }
  };

  // Approved, yet the ticket cannot advance. The approval and the agent's
  // verdict render as two separate rows, so a reviewer sees a green "Spec
  // approved" beside an amber verdict and has to know the rule connecting
  // them — which reads as an Approve button that did nothing. Mirrors
  // `executing_gate_passes` in the orchestrator: an RCA needs high
  // confidence, a spec needs zero open questions.
  const gateBlockedReason = (() => {
    if (!specStatus?.approvedAt || !planningVerdict) return null;
    const kind =
      planningVerdict.kind ?? (planningVerdict.confidence ? 'rca' : 'spec');
    if (kind === 'rca') {
      return planningVerdict.confidence === 'high'
        ? null
        : t('kanban.gateBlockedConfidence', {
            confidence: planningVerdict.confidence ?? '—',
          });
    }
    return planningVerdict.openQuestions === 0
      ? null
      : t('kanban.gateBlockedOpenQuestions', {
          count: planningVerdict.openQuestions ?? 0,
        });
  })();

  return (
    <div
      className="flex flex-col h-full overflow-hidden outline-none"
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-base py-half border-b shrink-0">
        <div className="flex items-center gap-half min-w-0 font-ibm-plex-mono">
          <span className={`${breadcrumbTextClass} shrink-0`}>{displayId}</span>
          {!isCreateMode && onCopyLink && (
            <button
              type="button"
              onClick={onCopyLink}
              className="p-half rounded-sm text-low hover:text-normal hover:bg-panel transition-colors"
              aria-label={t('kanban.copyLink')}
            >
              <LinkIcon className="size-icon-sm" weight="bold" />
            </button>
          )}
        </div>
        <div className="flex items-center gap-half">
          {!isCreateMode && onMoreActions && (
            <button
              type="button"
              onClick={onMoreActions}
              className="p-half rounded-sm text-low hover:text-normal hover:bg-panel transition-colors"
              aria-label={t('kanban.moreActions')}
            >
              <DotsThreeIcon className="size-icon-sm" weight="bold" />
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            className="p-half rounded-sm text-low hover:text-normal hover:bg-panel transition-colors"
            aria-label={t('kanban.closePanel')}
          >
            <XIcon className="size-icon-sm" weight="bold" />
          </button>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto">
        {/* Property Row */}
        <div className="px-base py-base border-b">
          <IssuePropertyRow
            statusId={formData.statusId}
            priority={formData.priority}
            assigneeIds={formData.assigneeIds}
            assigneeUsers={assigneeUsers}
            statuses={statuses}
            creatorUser={showCreator ? creatorUser : undefined}
            parentIssue={parentIssue}
            onParentIssueClick={onParentIssueClick}
            onRemoveParentIssue={onRemoveParentIssue}
            onStatusClick={() => onFormChange('statusId', formData.statusId)}
            statusDisabled={statusLocked}
            onPriorityClick={() => onFormChange('priority', formData.priority)}
            onAssigneeClick={() =>
              onFormChange('assigneeIds', formData.assigneeIds)
            }
            disabled={isSubmitting}
          />
        </div>

        {/* Tags Row */}
        <div className="px-base py-base border-b">
          <IssueTagsRow
            selectedTagIds={formData.tagIds}
            availableTags={tags}
            linkedPrs={isCreateMode ? [] : linkedPrs}
            onTagsChange={(tagIds) => onFormChange('tagIds', tagIds)}
            onCreateTag={onCreateTag}
            renderAddTagControl={renderAddTagControl}
            onLinkPr={!isCreateMode ? onLinkPr : undefined}
            disabled={isSubmitting}
          />
        </div>

        {/* Title and Description */}
        <div className="rounded-sm">
          {/* Title Input */}
          <div className="w-full mt-base">
            <AutoResizeTextarea
              ref={titleInputRef}
              value={formData.title}
              onChange={(value) => onFormChange('title', value)}
              onKeyDown={handleTitleKeyDown}
              placeholder="Issue Title..."
              autoFocus={isCreateMode}
              aria-label="Issue title"
              disabled={isSubmitting}
              className={cn(
                'px-base text-lg font-medium text-high',
                'placeholder:text-high/50',
                isSubmitting && 'opacity-50 pointer-events-none'
              )}
            />

            <div
              className={cn(
                'pointer-events-none absolute inset-0 px-base',
                'text-high/50 font-medium text-lg',
                'hidden',
                "[[data-empty='true']_+_&]:block" // show placeholder when previous sibling data-empty=true
              )}
            >
              {t('kanban.issueTitlePlaceholder')}
            </div>
          </div>

          {/* Description WYSIWYG Editor with image dropzone */}
          <div
            ref={descriptionContainerRef}
            {...(isDescriptionEditing ? dropzoneProps?.getRootProps() : {})}
            className={cn(
              'relative mt-base',
              !isDescriptionEditing && !isCreateMode && 'cursor-text'
            )}
            onClick={() => {
              if (!isDescriptionEditing && !isCreateMode && !isSubmitting) {
                // Don't enter edit mode if the user was selecting text
                const selection = window.getSelection();
                if (selection && selection.toString().length > 0) return;
                setIsDescriptionEditing(true);
              }
            }}
            onBlur={(e) => {
              // Exit edit mode when focus leaves the description container
              if (
                descriptionContainerRef.current &&
                !descriptionContainerRef.current.contains(
                  e.relatedTarget as Node
                )
              ) {
                handleDescriptionBlur();
              }
            }}
          >
            {isDescriptionEditing && (
              <input
                {...(dropzoneProps?.getInputProps() as React.InputHTMLAttributes<HTMLInputElement>)}
                data-dropzone-input
              />
            )}
            {renderDescriptionEditor({
              placeholder: isDescriptionEditing
                ? t('kanban.issueDescriptionPlaceholder')
                : formData.description
                  ? ''
                  : t('kanban.issueDescriptionPlaceholder'),
              value: formData.description ?? '',
              onChange: (value) => onFormChange('description', value || null),
              onCmdEnter: onCmdEnterSubmit,
              onPasteFiles: isDescriptionEditing ? onPasteFiles : undefined,
              disabled: !isDescriptionEditing || isSubmitting,
              autoFocus: false,
              className: cn(
                'px-base',
                isDescriptionEditing ? 'min-h-[100px]' : 'min-h-[2rem]',
                !isDescriptionEditing && !formData.description && 'text-low'
              ),
              localAttachments,
              showStaticToolbar: !isCreateMode || isDescriptionEditing,
              hideActions: true,
              saveStatus: descriptionSaveStatus,
              onRequestEdit: !isCreateMode
                ? () => setIsDescriptionEditing(true)
                : undefined,
              staticToolbarActions: (
                <>
                  {isDescriptionEditing && onBrowseAttachment && (
                    <TooltipProvider>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            onMouseDown={(e) => {
                              e.preventDefault();
                              if (!isSubmitting && !isUploading) {
                                onBrowseAttachment();
                              }
                            }}
                            disabled={isSubmitting || isUploading}
                            className={cn(
                              'p-half rounded-sm transition-colors',
                              'text-low hover:text-normal hover:bg-panel/50',
                              'disabled:opacity-50 disabled:cursor-not-allowed'
                            )}
                            title={t('kanban.attachFile')}
                            aria-label={t('kanban.attachFile')}
                          >
                            <PaperclipIcon className="size-icon-sm" />
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>
                          {t('kanban.attachFileHint')}
                        </TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  )}
                </>
              ),
            })}
            {attachmentError && (
              <div className="px-base">
                <ErrorAlert
                  message={attachmentError}
                  className="mt-half mb-half"
                  onDismiss={onDismissAttachmentError}
                  dismissLabel={t('buttons.close')}
                />
              </div>
            )}
            {dropzoneProps?.isDragActive && (
              <div className="absolute inset-0 z-50 bg-primary/80 backdrop-blur-sm border-2 border-dashed border-brand rounded flex items-center justify-center pointer-events-none animate-in fade-in-0 duration-150">
                <div className="text-center">
                  <div className="mx-auto mb-2 w-10 h-10 rounded-full bg-brand/10 flex items-center justify-center">
                    <ImageIcon className="h-5 w-5 text-brand" />
                  </div>
                  <p className="text-sm font-medium text-high">
                    {t('kanban.dropFilesHere')}
                  </p>
                  <p className="text-xs text-low mt-0.5">
                    {t('kanban.fileDropHint')}
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Create Draft Workspace Toggle (Create mode only) */}
        {isCreateMode && (
          <div className="p-base border-t">
            <Toggle
              checked={formData.createDraftWorkspace}
              onCheckedChange={(checked) =>
                onFormChange('createDraftWorkspace', checked)
              }
              label={t('kanban.createDraftWorkspaceImmediately')}
              description={t('kanban.createDraftWorkspaceDescription')}
              disabled={isSubmitting}
            />
          </div>
        )}

        {/* Create Issue Button (Create mode only) */}
        {isCreateMode && (
          <div className="px-base pb-base flex items-center gap-half">
            <PrimaryButton
              value={t('kanban.createIssue')}
              onClick={onSubmit}
              disabled={isSubmitting || isUploading || !formData.title.trim()}
              actionIcon={isSubmitting ? 'spinner' : undefined}
              variant="default"
            />
            {onDeleteDraft && (
              <IconButton
                icon={TrashIcon}
                onClick={onDeleteDraft}
                disabled={isSubmitting}
                aria-label="Delete draft"
                title="Delete draft"
                className="hover:text-error hover:bg-error/10"
              />
            )}
          </div>
        )}

        {/* Stage Playbook Action (Edit mode only) */}
        {!isCreateMode && issueId && stageAction && (
          <div className="px-base py-base border-t">
            <PrimaryButton
              value={stageAction.label}
              onClick={stageAction.onRun}
              disabled={stageAction.disabled}
              actionIcon={stageAction.disabled ? 'spinner' : undefined}
              variant="default"
            />
          </div>
        )}

        {/* Live Lavish review gate (Edit mode only): open the plan/RCA
            review session in the browser */}
        {!isCreateMode && issueId && reviewUrl && (
          <div className="px-base py-base border-t">
            <a
              href={reviewUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center justify-center gap-half w-full h-8 rounded-sm bg-brand/15 text-brand hover:bg-brand/25 transition-colors text-sm font-medium"
            >
              <StampIcon className="size-icon-xs" weight="bold" />
              {t('kanban.reviewPlan')}
            </a>
          </div>
        )}

        {/* Human takeover (Edit mode only): automation is paused. */}
        {!isCreateMode && issueId && humanTakeover && (
          <div className="px-base py-base border-t flex items-center gap-2 text-sm">
            <span className="inline-flex items-center rounded-sm px-1.5 py-0.5 text-xs bg-warning/15 text-warning uppercase">
              {t('kanban.takeoverPaused')}
            </span>
            <span className="text-low truncate">
              {humanTakeover.assignee
                ? t('kanban.takeoverBy', { name: humanTakeover.assignee })
                : t('kanban.takeoverByUnknown')}
            </span>
          </div>
        )}

        {/* Planning verdict (Edit mode only): the gate state the agent
            recorded — RCA confidence must be high / specs need 0 open
            questions before the ticket can advance to Executing. */}
        {!isCreateMode && issueId && planningVerdict && (
          <div className="px-base py-base border-t text-sm space-y-1">
            <div className="flex items-center gap-2">
              <span
                className={
                  planningVerdict.confidence === 'high' ||
                  planningVerdict.openQuestions === 0
                    ? 'inline-flex items-center rounded-sm px-1.5 py-0.5 text-xs bg-success/15 text-success uppercase'
                    : 'inline-flex items-center rounded-sm px-1.5 py-0.5 text-xs bg-warning/15 text-warning uppercase'
                }
              >
                {planningVerdict.kind ?? 'plan'}
                {planningVerdict.confidence
                  ? ` · ${planningVerdict.confidence}`
                  : ''}
              </span>
              {planningVerdict.openQuestions != null && (
                <span className="text-low shrink-0">
                  {t('kanban.verdictOpenQuestions', {
                    count: planningVerdict.openQuestions,
                  })}
                </span>
              )}
              {planningVerdict.summary && (
                <span
                  className="text-low truncate"
                  title={planningVerdict.summary}
                >
                  {planningVerdict.summary}
                </span>
              )}
            </div>
            {planningVerdict.confidenceReason && (
              <div
                className="text-xs text-warning"
                title={planningVerdict.confidenceReason}
              >
                {t('kanban.verdictWhyNotHigher')}{' '}
                {planningVerdict.confidenceReason}
              </div>
            )}
          </div>
        )}

        {/* Spec/RCA artifact + Lavish approval state (Edit mode only) */}
        {!isCreateMode && issueId && specStatus && (
          <div className="px-base py-base border-t flex items-center gap-2 text-sm">
            <span
              className={
                specStatus.approvedAt
                  ? 'inline-flex items-center rounded-sm px-1.5 py-0.5 text-xs bg-success/15 text-success'
                  : 'inline-flex items-center rounded-sm px-1.5 py-0.5 text-xs bg-warning/15 text-warning'
              }
            >
              {specStatus.approvedAt
                ? t('kanban.specApproved')
                : t('kanban.specAwaitingApproval')}
            </span>
            {specStatus.path && (
              <span className="text-low truncate" title={specStatus.path}>
                {specStatus.path}
              </span>
            )}
            {gateBlockedReason && (
              <span
                className="ml-auto shrink-0 text-xs text-warning"
                title={gateBlockedReason}
              >
                {gateBlockedReason}
              </span>
            )}
            {!specStatus.approvedAt && onReopenReview && (
              <button
                type="button"
                onClick={onReopenReview}
                disabled={reopeningReview}
                className="ml-auto shrink-0 flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium bg-brand/15 text-brand hover:bg-brand/25 transition-colors disabled:opacity-50"
                title={t('kanban.reopenReviewHint')}
              >
                <StampIcon className="size-icon-2xs" weight="bold" />
                {reopeningReview
                  ? t('kanban.reopeningReview')
                  : t('kanban.reopenReview')}
              </button>
            )}
          </div>
        )}

        {/* Workspaces Section (Edit mode only) */}
        {!isCreateMode && issueId && renderWorkspacesSection && (
          <div className="border-t">{renderWorkspacesSection(issueId)}</div>
        )}

        {/* Relationships Section (Edit mode only) */}
        {!isCreateMode && issueId && renderRelationshipsSection && (
          <div className="border-t">{renderRelationshipsSection(issueId)}</div>
        )}

        {/* Sub-Issues Section (Edit mode only) */}
        {!isCreateMode && issueId && renderSubIssuesSection && (
          <div className="border-t">{renderSubIssuesSection(issueId)}</div>
        )}

        {/* Comments Section (Edit mode only) */}
        {!isCreateMode && issueId && renderCommentsSection && (
          <div className="border-t">{renderCommentsSection(issueId)}</div>
        )}
      </div>
    </div>
  );
}
