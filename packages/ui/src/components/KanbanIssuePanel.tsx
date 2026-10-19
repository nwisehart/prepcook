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
  ArrowSquareOutIcon,
  CaretDownIcon,
  DotsThreeIcon,
  FileTextIcon,
  ImageIcon,
  LinkIcon,
  PaperclipIcon,
  StampIcon,
  TrashIcon,
  XIcon,
} from '@phosphor-icons/react';
import {
  IssueTagsRow,
  type IssueTagBase,
  type IssueTagsRowAddTagControlProps,
  type LinkedPullRequest as IssueTagsLinkedPullRequest,
} from './IssueTagsRow';
import { Button } from './Button';
import { Toggle } from './Toggle';
import {
  IssuePropertyRow,
  type IssueExtraProperty,
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
import { IssuePanelSection } from './IssuePanelSection';
import type { KanbanColumnDot } from './KanbanBoard';
import {
  KANBAN_CLASS_CHIP_STYLES,
  KanbanCardStatus,
  type KanbanCardStatusLine,
  type KanbanClassTone,
} from './KanbanCardContent';

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

/**
 * The amber decision block at the top of the panel when the card waits on
 * a person: its plan is ready (Approve / Request changes / open it), or it
 * is merged and waits to be closed.
 */
export type KanbanIssuePanelDecision =
  | {
      kind: 'plan';
      /** "Needs you · the RCA is ready". */
      label: string;
      /** "Confidence high · no open questions". */
      detail?: string | null;
      onApprove: () => void;
      /** Resolves true when the request went through. */
      onRequestChanges: (reason: string) => Promise<boolean>;
      onOpenPlan?: (() => void) | null;
      /** "Open the RCA" / "Open the plan". */
      openPlanLabel: string;
      isSubmitting?: boolean;
    }
  | {
      kind: 'close';
      label: string;
      statusLine?: KanbanCardStatusLine | null;
      closeLabel: string;
      onClose: () => void;
      isSubmitting?: boolean;
    };

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

  // Header chips and facts
  classChip?: { name: string; tone: KanbanClassTone } | null;
  /** "P0"–"P3"; null shows a "Priority" chip to set one. */
  priorityLabel?: string | null;
  /** "Open in Jira" when the card has a tracker URL. */
  trackerLink?: { label: string; url: string } | null;
  /** The Stage pill's dot. */
  statusDot?: KanbanColumnDot;
  /** Playbook, Tracker, Branch: whatever the card has. */
  extraProperties?: IssueExtraProperty[];
  decision?: KanbanIssuePanelDecision | null;
  /** Footer "Close card": moves the card on to its closing column. */
  closeCardAction?: {
    label: string;
    onClick: () => void;
    disabled?: boolean;
  } | null;

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
  // (edit mode only): renders a "Review plan" link while the gate is open.
  reviewUrl?: string | null;

  // Re-opens a dead review gate for the unapproved spec (edit mode only,
  // offered when no live session exists). `reopeningReview` keeps the
  // button disabled until the new session shows up.
  onReopenReview?: () => void;
  reopeningReview?: boolean;

  // The approved plan's artifact (edit mode only), once the gate approved it.
  approvedPlanUrl?: string | null;

  // Edit-mode section renderers
  renderWorkspacesSection?: (issueId: string) => ReactNode;
  renderRelationshipsSection?: (issueId: string) => ReactNode;
  renderSubIssuesSection?: (issueId: string) => ReactNode;
  renderCommentsSection?: (issueId: string) => ReactNode;
}

const CHIP =
  'inline-flex h-[22px] shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full px-2 text-xs font-medium leading-none';

function DecisionBlock({
  decision,
  issueId,
}: {
  decision: KanbanIssuePanelDecision;
  issueId: string | null | undefined;
}) {
  const { t } = useTranslation('common');
  const [isRequesting, setIsRequesting] = useState(false);
  const [reason, setReason] = useState('');
  const [isSending, setIsSending] = useState(false);

  // A half-written request belongs to the card it was written on.
  useEffect(() => {
    setIsRequesting(false);
    setReason('');
  }, [issueId]);

  const sendRequest = async () => {
    if (decision.kind !== 'plan' || !reason.trim() || isSending) return;
    setIsSending(true);
    try {
      const sent = await decision.onRequestChanges(reason.trim());
      if (sent) {
        setIsRequesting(false);
        setReason('');
      }
    } finally {
      setIsSending(false);
    }
  };

  return (
    <section
      aria-label={decision.label}
      className="mt-3.5 overflow-hidden rounded-lg border border-attention-border"
    >
      <div className="flex items-center gap-2 bg-attention-soft px-3.5 py-2 text-xs font-bold text-attention-foreground">
        <span
          aria-hidden="true"
          className="size-2 shrink-0 rounded-full bg-attention"
        />
        {decision.label}
      </div>
      <div className="flex flex-col gap-2.5 px-3.5 pb-3.5 pt-3">
        {decision.kind === 'plan' ? (
          <>
            <span className="text-[13px] text-normal">
              {decision.detail ? `${decision.detail}. ` : ''}
              {t('kanban.needsYou.panelNote')}
            </span>
            {isRequesting ? (
              <div className="flex flex-col gap-2">
                <textarea
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  onKeyDown={(e) => {
                    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
                      e.preventDefault();
                      void sendRequest();
                    }
                  }}
                  placeholder={t('kanban.needsYou.requestChangesPlaceholder')}
                  aria-label={t('kanban.needsYou.requestChanges')}
                  autoFocus
                  rows={3}
                  className="w-full resize-y rounded border border-border bg-secondary px-3 py-2 text-[13px] text-high placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"
                />
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    onClick={() => void sendRequest()}
                    disabled={!reason.trim() || isSending}
                  >
                    {t('kanban.needsYou.sendRequest')}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => {
                      setIsRequesting(false);
                      setReason('');
                    }}
                  >
                    {t('buttons.cancel')}
                  </Button>
                </div>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  onClick={decision.onApprove}
                  disabled={decision.isSubmitting}
                >
                  {t('kanban.needsYou.approve')}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setIsRequesting(true)}
                  disabled={decision.isSubmitting}
                >
                  {t('kanban.needsYou.requestChanges')}
                </Button>
                {decision.onOpenPlan && (
                  <button
                    type="button"
                    onClick={decision.onOpenPlan}
                    className="ml-auto text-[13px] font-semibold text-brand-secondary hover:underline"
                  >
                    {decision.openPlanLabel}
                  </button>
                )}
              </div>
            )}
          </>
        ) : (
          <>
            {decision.statusLine && (
              <KanbanCardStatus line={decision.statusLine} />
            )}
            <div>
              <Button
                type="button"
                variant="outline"
                onClick={decision.onClose}
                disabled={decision.isSubmitting}
              >
                {decision.closeLabel}
              </Button>
            </div>
          </>
        )}
      </div>
    </section>
  );
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
  classChip,
  priorityLabel,
  trackerLink,
  statusDot,
  extraProperties,
  decision,
  closeCardAction,
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
  approvedPlanUrl,
  renderWorkspacesSection,
  renderRelationshipsSection,
  renderSubIssuesSection,
  renderCommentsSection,
}: KanbanIssuePanelProps) {
  const { t } = useTranslation('common');
  const isCreateMode = mode === 'create';
  const showCreator =
    !isCreateMode &&
    Boolean(creatorUser?.first_name?.trim() || creatorUser?.username?.trim());
  const isEditing = !isCreateMode && !!issueId;

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
  const verdictPasses =
    planningVerdict?.confidence === 'high' ||
    planningVerdict?.openQuestions === 0;
  const showPlanSection =
    isEditing &&
    (!!planningVerdict || !!specStatus || !!reviewUrl || !!approvedPlanUrl);

  const iconButtonClass =
    'flex size-8 shrink-0 items-center justify-center rounded text-low transition-colors hover:bg-panel hover:text-high focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand';

  return (
    <div
      className="flex h-full flex-col overflow-hidden bg-secondary outline-none"
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      {/* Header: key and chips, title, property grid */}
      <div className="flex shrink-0 flex-col gap-2.5 border-b border-divider px-[22px] pb-3.5 pt-4">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate font-mono text-[13px] font-medium text-normal">
            {displayId}
          </span>
          {classChip && (
            <span
              className={cn(CHIP, KANBAN_CLASS_CHIP_STYLES[classChip.tone])}
            >
              {classChip.name}
            </span>
          )}
          <button
            type="button"
            onClick={() => onFormChange('priority', formData.priority)}
            disabled={isSubmitting}
            className={cn(
              CHIP,
              'bg-panel transition-colors hover:text-high disabled:opacity-50',
              priorityLabel ? 'text-normal' : 'text-low'
            )}
          >
            {priorityLabel ?? t('kanban.priority')}
          </button>
          <span className="flex-1" />
          {!isCreateMode && trackerLink && (
            <a
              href={trackerLink.url}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex shrink-0 items-center gap-1.5 text-xs font-semibold text-brand-secondary hover:underline"
            >
              {trackerLink.label}
              <ArrowSquareOutIcon className="size-3.5" aria-hidden="true" />
            </a>
          )}
          {!isCreateMode && onCopyLink && (
            <button
              type="button"
              onClick={onCopyLink}
              className={iconButtonClass}
              aria-label={t('kanban.copyLink')}
              title={t('kanban.copyLink')}
            >
              <LinkIcon className="size-4" />
            </button>
          )}
          {!isCreateMode && onMoreActions && (
            <button
              type="button"
              onClick={onMoreActions}
              className={iconButtonClass}
              aria-label={t('kanban.moreActions')}
              title={t('kanban.moreActions')}
            >
              <DotsThreeIcon className="size-4" weight="bold" />
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            className={iconButtonClass}
            aria-label={t('kanban.closePanel')}
            title={t('kanban.closePanel')}
          >
            <XIcon className="size-4" />
          </button>
        </div>

        <div className="relative w-full">
          <AutoResizeTextarea
            ref={titleInputRef}
            value={formData.title}
            onChange={(value) => onFormChange('title', value)}
            onKeyDown={handleTitleKeyDown}
            placeholder={t('kanban.panel.titlePlaceholder')}
            autoFocus={isCreateMode}
            aria-label={t('kanban.panel.titlePlaceholder')}
            disabled={isSubmitting}
            className={cn(
              'text-lg font-bold leading-snug tracking-[-0.2px] text-high',
              'placeholder:text-low',
              isSubmitting && 'pointer-events-none opacity-50'
            )}
          />
        </div>

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
          statusDot={statusDot}
          extraProperties={isCreateMode ? [] : extraProperties}
          disabled={isSubmitting}
        />

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

        {/* Human takeover: automation is paused on this card. */}
        {isEditing && humanTakeover && (
          <div className="flex min-w-0 items-center gap-2 text-xs">
            <span
              className={cn(
                CHIP,
                'bg-attention-soft text-attention-foreground'
              )}
            >
              {t('kanban.takeoverPaused')}
            </span>
            <span className="truncate text-low">
              {humanTakeover.assignee
                ? t('kanban.takeoverBy', { name: humanTakeover.assignee })
                : t('kanban.takeoverByUnknown')}
            </span>
          </div>
        )}
      </div>

      {/* Scrollable content */}
      <div className="flex flex-1 flex-col overflow-y-auto px-[22px]">
        {isEditing && decision && (
          <DecisionBlock decision={decision} issueId={issueId} />
        )}

        <IssuePanelSection
          label={t('kanban.panel.description')}
          divider={isEditing && !!decision}
        >
          <div
            ref={descriptionContainerRef}
            {...(isDescriptionEditing ? dropzoneProps?.getRootProps() : {})}
            className={cn(
              'relative -mx-1',
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
                'px-1 text-[13.5px] text-high',
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
                              'rounded-sm p-half transition-colors',
                              'text-low hover:bg-panel hover:text-high',
                              'disabled:cursor-not-allowed disabled:opacity-50'
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
              <div className="px-1">
                <ErrorAlert
                  message={attachmentError}
                  className="my-half"
                  onDismiss={onDismissAttachmentError}
                  dismissLabel={t('buttons.close')}
                />
              </div>
            )}
            {dropzoneProps?.isDragActive && (
              <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center rounded border-2 border-dashed border-brand bg-primary/80 backdrop-blur-sm animate-in fade-in-0 duration-150">
                <div className="text-center">
                  <div className="mx-auto mb-2 flex size-10 items-center justify-center rounded-full bg-brand-soft">
                    <ImageIcon className="size-5 text-brand" />
                  </div>
                  <p className="text-sm font-medium text-high">
                    {t('kanban.dropFilesHere')}
                  </p>
                  <p className="mt-0.5 text-xs text-low">
                    {t('kanban.fileDropHint')}
                  </p>
                </div>
              </div>
            )}
          </div>
        </IssuePanelSection>

        {/* Create mode: open a session draft right after creating */}
        {isCreateMode && (
          <div className="border-t border-divider py-3.5">
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

        {/* The plan the agent recorded and where its approval stands */}
        {showPlanSection && (
          <IssuePanelSection label={t('kanban.panel.plan')}>
            {planningVerdict && (
              <div className="flex flex-col gap-1.5 text-[13px]">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span
                    className={cn(
                      CHIP,
                      verdictPasses
                        ? 'bg-success-soft text-success'
                        : 'bg-attention-soft text-attention-foreground'
                    )}
                  >
                    {planningVerdict.kind ?? t('kanban.panel.planKindFallback')}
                    {planningVerdict.confidence
                      ? ` · ${planningVerdict.confidence}`
                      : ''}
                  </span>
                  {planningVerdict.openQuestions != null && (
                    <span className="shrink-0 text-low">
                      {t('kanban.verdictOpenQuestions', {
                        count: planningVerdict.openQuestions,
                      })}
                    </span>
                  )}
                </div>
                {planningVerdict.summary && (
                  <p className="m-0 line-clamp-3 text-normal">
                    {planningVerdict.summary}
                  </p>
                )}
                {planningVerdict.confidenceReason && (
                  <p className="m-0 text-xs text-attention-foreground">
                    {t('kanban.verdictWhyNotHigher')}{' '}
                    {planningVerdict.confidenceReason}
                  </p>
                )}
              </div>
            )}
            {specStatus && (
              <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
                <span
                  className={cn(
                    CHIP,
                    specStatus.approvedAt
                      ? 'bg-success-soft text-success'
                      : 'bg-attention-soft text-attention-foreground'
                  )}
                >
                  {specStatus.approvedAt
                    ? t('kanban.specApproved')
                    : t('kanban.specAwaitingApproval')}
                </span>
                {specStatus.path && (
                  <span
                    className="min-w-0 truncate font-mono text-xs text-low"
                    title={specStatus.path}
                  >
                    {specStatus.path}
                  </span>
                )}
              </div>
            )}
            {gateBlockedReason && (
              <p className="m-0 text-xs text-attention-foreground">
                {gateBlockedReason}
              </p>
            )}
            {(reviewUrl ||
              approvedPlanUrl ||
              (!specStatus?.approvedAt && onReopenReview)) && (
              <div className="flex flex-wrap items-center gap-2">
                {approvedPlanUrl && (
                  <Button asChild variant="outline" size="sm">
                    <a
                      href={approvedPlanUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      <FileTextIcon className="mr-1.5 size-3.5" />
                      {t('kanban.openApprovedPlan')}
                    </a>
                  </Button>
                )}
                {reviewUrl && (
                  <Button asChild variant="outline" size="sm">
                    <a
                      href={reviewUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      <StampIcon className="mr-1.5 size-3.5" weight="bold" />
                      {t('kanban.reviewPlan')}
                    </a>
                  </Button>
                )}
                {!reviewUrl && !specStatus?.approvedAt && onReopenReview && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onReopenReview}
                    disabled={reopeningReview}
                    title={t('kanban.reopenReviewHint')}
                  >
                    <StampIcon className="mr-1.5 size-3.5" weight="bold" />
                    {reopeningReview
                      ? t('kanban.reopeningReview')
                      : t('kanban.reopenReview')}
                  </Button>
                )}
              </div>
            )}
          </IssuePanelSection>
        )}

        {/* Sessions on this card */}
        {isEditing &&
          renderWorkspacesSection &&
          renderWorkspacesSection(issueId)}

        {/* Activity */}
        {isEditing && renderCommentsSection && renderCommentsSection(issueId)}

        {/* Related cards and sub-issues */}
        {isEditing &&
          renderRelationshipsSection &&
          renderRelationshipsSection(issueId)}
        {isEditing && renderSubIssuesSection && renderSubIssuesSection(issueId)}
      </div>

      {/* Footer */}
      {isCreateMode ? (
        <footer className="flex shrink-0 items-center gap-2 border-t border-divider bg-secondary px-[22px] pb-4 pt-3">
          <Button
            type="button"
            onClick={onSubmit}
            disabled={isSubmitting || isUploading || !formData.title.trim()}
          >
            {t('kanban.createCard')}
          </Button>
          {onDeleteDraft && (
            <IconButton
              icon={TrashIcon}
              onClick={onDeleteDraft}
              disabled={isSubmitting}
              aria-label={t('kanban.panel.deleteDraft')}
              title={t('kanban.panel.deleteDraft')}
              className="hover:bg-error-soft hover:text-error"
            />
          )}
        </footer>
      ) : (
        isEditing && (
          <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-divider bg-secondary px-[22px] pb-4 pt-3">
            <Button
              type="button"
              variant="outline"
              onClick={() => onFormChange('statusId', formData.statusId)}
              disabled={isSubmitting || statusLocked}
              className="gap-1.5"
            >
              {t('kanban.panel.moveTo')}
              <CaretDownIcon className="size-3 text-low" />
            </Button>
            {stageAction && (
              <Button
                type="button"
                variant="outline"
                onClick={stageAction.onRun}
                disabled={stageAction.disabled}
              >
                {stageAction.label}
              </Button>
            )}
            <span className="flex-1" />
            {closeCardAction && (
              <Button
                type="button"
                variant="ghost"
                onClick={closeCardAction.onClick}
                disabled={closeCardAction.disabled}
                className="text-low"
              >
                {closeCardAction.label}
              </Button>
            )}
          </footer>
        )
      )}
    </div>
  );
}
