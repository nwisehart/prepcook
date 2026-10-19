import { useMemo, useCallback, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { useQueryClient } from '@tanstack/react-query';
import {
  ActionType,
  BaseAgentCapability,
  NormalizedEntry,
  ToolStatus,
  ToolResult,
  TodoItem,
  type RepoWithTargetBranch,
} from 'shared/types';
import type { WorkspaceWithSession } from '@/shared/types/attempt';
import { parseDiffStats } from '@/shared/lib/diffStatsParser';
import {
  usePersistedExpanded,
  type PersistKey,
} from '@/shared/stores/useUiPreferencesStore';
import { getActualTheme } from '@/shared/lib/theme';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useTheme } from '@/shared/hooks/useTheme';
import { useLavishSession } from '@/shared/hooks/useLavishSession';
import { workspaceSummaryKeys } from '@/shared/hooks/workspaceSummaryKeys';
import WYSIWYGEditor from '@/shared/components/WYSIWYGEditor';
import { useMessageEditContext } from '../model/contexts/MessageEditContext';
import { useApprovalFeedbackOptional } from '../model/contexts/ApprovalFeedbackContext';
import { useApprovalMutation } from '../model/hooks/useApprovalMutation';
import type { UseResetProcessResult } from '../model/hooks/useResetProcess';
import { useChangesViewActions } from '@/shared/hooks/useChangesView';
import { useLogsPanelActions } from '@/shared/hooks/useLogsPanel';
import { cn } from '@/shared/lib/utils';
import { SettingsDialog } from '@/shared/dialogs/settings/SettingsDialog';
import { useSessionCards } from '@/features/workspace/model/hooks/useSessionCards';
import { ChatToolSummary } from '@prepcook/ui/components/ChatToolSummary';
import { ChatTodoList } from '@prepcook/ui/components/ChatTodoList';
import {
  ChatFileEntry,
  type ChatFileEntryDiffInput,
} from '@prepcook/ui/components/ChatFileEntry';
import {
  ChatApprovalCard,
  type ChatApprovalCardDecision,
} from '@prepcook/ui/components/ChatApprovalCard';
import { ChatUserMessage } from '@prepcook/ui/components/ChatUserMessage';
import { ChatAssistantMessage } from '@prepcook/ui/components/ChatAssistantMessage';
import { ChatSystemMessage } from '@prepcook/ui/components/ChatSystemMessage';
import { ChatThinkingMessage } from '@prepcook/ui/components/ChatThinkingMessage';
import { ChatErrorMessage } from '@prepcook/ui/components/ChatErrorMessage';
import { ChatScriptEntry } from '@prepcook/ui/components/ChatScriptEntry';
import { ChatSubagentEntry } from '@prepcook/ui/components/ChatSubagentEntry';
import { ChatAggregatedToolEntries } from '@prepcook/ui/components/ChatAggregatedToolEntries';
import { ChatAggregatedDiffEntries } from '@prepcook/ui/components/ChatAggregatedDiffEntries';
import { ChatCollapsedThinking } from '@prepcook/ui/components/ChatCollapsedThinking';
import { ChatMarkdown } from '@prepcook/ui/components/ChatMarkdown';
import {
  ChatPrefixedComment,
  ChatStep,
  ChatStepDetail,
} from '@prepcook/ui/components/ChatStep';
import {
  DiffViewBody,
  useDiffData,
} from '@prepcook/ui/components/PierreConversationDiff';
import { useDiffViewMode } from '@/shared/stores/useDiffViewStore';
import type {
  AggregatedPatchGroup,
  AggregatedDiffGroup,
  AggregatedThinkingGroup,
  PatchTypeWithKey,
} from '@/shared/hooks/useConversationHistory/types';
import {
  fileBaseName,
  findPrefixedCommentInArgs,
  formatStepTime,
  humanizeToolName,
  splitPrefixedComment,
  summarizeList,
  type PrefixedComment,
} from './stepPresentation';

type Props = {
  expansionKey: string;
  executionProcessId: string;
  workspaceWithSession: WorkspaceWithSession;
  resetAction: UseResetProcessResult;
  repos: RepoWithTargetBranch[];
  entry: NormalizedEntry | null;
  aggregatedGroup: AggregatedPatchGroup | null;
  aggregatedDiffGroup: AggregatedDiffGroup | null;
  aggregatedThinkingGroup: AggregatedThinkingGroup | null;
};

type FileEditAction = Extract<ActionType, { action: 'file_edit' }>;
type ToolUseEntryType = Extract<
  NormalizedEntry['entry_type'],
  { type: 'tool_use' }
>;

// Details longer than this fold behind a disclosure when there is no output
// to open instead.
const LONG_DETAIL_CHARS = 90;

interface ToolDescription {
  label: string;
  detail?: string;
  /** Whether the detail is code-like (path, query, command) */
  mono: boolean;
  comment?: PrefixedComment | null;
}

/** The step label and detail line for a tool call. */
function describeTool(
  entryType: ToolUseEntryType,
  t: TFunction<'common'>
): ToolDescription {
  const { action_type, tool_name } = entryType;

  switch (action_type.action) {
    case 'file_read':
      return {
        label: t('conversation.steps.readFile'),
        detail: action_type.path,
        mono: true,
      };
    case 'search':
      return {
        label: t('conversation.steps.searched'),
        detail: action_type.query,
        mono: true,
      };
    case 'web_fetch':
      return {
        label: t('conversation.steps.fetched'),
        detail: action_type.url,
        mono: true,
      };
    case 'command_run':
      return {
        label: t('conversation.steps.ranCommand'),
        detail: action_type.command || undefined,
        mono: true,
      };
    case 'tool': {
      const { label, server } = humanizeToolName(
        action_type.tool_name || tool_name
      );
      const comment = findPrefixedCommentInArgs(action_type.arguments);
      return {
        label: comment ? t('conversation.steps.postedComment') : label,
        detail: server ?? undefined,
        mono: Boolean(server),
        comment,
      };
    }
    case 'ask_user_question':
      return { label: t('askQuestion.title'), mono: false };
    case 'other':
      return {
        label: action_type.description || tool_name || t('conversation.tool'),
        mono: false,
      };
    default:
      return {
        label: humanizeToolName(tool_name).label || t('conversation.tool'),
        mono: false,
      };
  }
}

/**
 * Extract the actual tool output from action_type.result
 * The output location depends on the action type:
 * - command_run: result.output
 * - tool: result.value (JSON stringified if object)
 * - others: fall back to entry.content
 */
function getToolOutput(
  entryType: ToolUseEntryType,
  entryContent: string
): string {
  const { action_type } = entryType;

  switch (action_type.action) {
    case 'command_run':
      return action_type.result?.output ?? entryContent;
    case 'tool':
      if (action_type.result?.value != null) {
        return typeof action_type.result.value === 'string'
          ? action_type.result.value
          : JSON.stringify(action_type.result.value, null, 2);
      }
      return entryContent;
    default:
      return entryContent;
  }
}

/**
 * Extract the command from action_type for command_run actions
 */
function getToolCommand(entryType: ToolUseEntryType): string | undefined {
  const { action_type } = entryType;

  if (action_type.action === 'command_run') {
    return action_type.command;
  }
  return undefined;
}

/** The step label for one file change, e.g. "Edited quantity.ts". */
function fileChangeLabel(
  action: FileEditAction['changes'][number]['action'],
  path: string,
  t: TFunction<'common'>
): string {
  const file = fileBaseName(path);
  switch (action) {
    case 'write':
      return t('conversation.steps.createdFile', { file });
    case 'delete':
      return t('conversation.steps.deletedFile', { file });
    case 'rename':
      return t('conversation.steps.renamedFile', { file });
    default:
      return t('conversation.steps.editedFile', { file });
  }
}

/** Timestamp of the first normalized entry in a group. */
function firstEntryTime(entries: PatchTypeWithKey[]): string | undefined {
  for (const entry of entries) {
    if (entry.type === 'NORMALIZED_ENTRY') {
      return formatStepTime(entry.content.timestamp);
    }
  }
  return undefined;
}

/**
 * Render tool_use entry types with appropriate components
 */
function renderToolUseEntry(
  entryType: ToolUseEntryType,
  entry: NormalizedEntry,
  props: Props,
  t: TFunction<'common'>
): React.ReactNode {
  const { expansionKey, executionProcessId, workspaceWithSession, repos } =
    props;
  const sessionId = workspaceWithSession?.session?.id;
  const { action_type, status } = entryType;
  const time = formatStepTime(entry.timestamp);

  // File edit - use ChatFileEntry
  if (action_type.action === 'file_edit') {
    const fileEditAction = action_type as FileEditAction;
    return (
      <>
        {fileEditAction.changes.map((change, idx) => (
          <FileEditEntry
            key={idx}
            path={fileEditAction.path}
            change={change}
            expansionKey={`edit:${expansionKey}:${idx}`}
            status={status}
            time={time}
          />
        ))}
      </>
    );
  }

  // Plan presentation - use ChatApprovalCard
  if (action_type.action === 'plan_presentation') {
    // Codex can emit an initial empty plan placeholder before content arrives.
    // Suppress empty cards to avoid duplicate-looking plan boxes.
    if (!action_type.plan.trim()) {
      return null;
    }
    return (
      <PlanEntry
        plan={action_type.plan}
        expansionKey={expansionKey}
        workspaceId={workspaceWithSession?.id}
        sessionId={sessionId}
        executionProcessId={executionProcessId}
        status={status}
      />
    );
  }

  // Todo management - use ChatTodoList
  if (action_type.action === 'todo_management') {
    return (
      <TodoManagementEntry
        todos={action_type.todos}
        expansionKey={expansionKey}
        time={time}
      />
    );
  }

  // Task/Subagent - use ChatSubagentEntry
  if (action_type.action === 'task_create') {
    return (
      <SubagentEntry
        description={action_type.description}
        subagentType={action_type.subagent_type}
        result={action_type.result}
        expansionKey={expansionKey}
        status={status}
        workspaceId={workspaceWithSession?.id}
        sessionId={sessionId}
        time={time}
      />
    );
  }

  // Script entries (Setup Script, Cleanup Script, Archive Script, Tool Install Script)
  const scriptToolNames = [
    'Setup Script',
    'Cleanup Script',
    'Archive Script',
    'Tool Install Script',
  ];
  if (
    action_type.action === 'command_run' &&
    scriptToolNames.includes(entryType.tool_name)
  ) {
    const exitCode =
      action_type.result?.exit_status?.type === 'exit_code'
        ? action_type.result.exit_status.code
        : null;

    return (
      <ScriptEntryWithFix
        title={entryType.tool_name}
        command={action_type.command}
        processId={executionProcessId ?? ''}
        exitCode={exitCode}
        status={status}
        workspaceId={workspaceWithSession?.id}
        repos={repos}
        time={time}
      />
    );
  }

  // Generic tool pending approval - decision card
  if (status.status === 'pending_approval') {
    // Question approvals are rendered via askQuestionMode in SessionChatBoxContainer.
    // Avoid showing a duplicate inline approval card in the conversation timeline.
    if (action_type.action === 'ask_user_question') {
      return null;
    }
    return (
      <GenericToolApprovalEntry
        toolName={entryType.tool_name}
        content={entry.content}
        expansionKey={expansionKey}
        workspaceId={workspaceWithSession?.id}
        sessionId={sessionId}
        executionProcessId={executionProcessId}
        status={status}
      />
    );
  }

  // Other tool uses - a quiet step
  return (
    <ToolSummaryEntry
      description={describeTool(entryType, t)}
      expansionKey={expansionKey}
      status={status}
      content={getToolOutput(entryType, entry.content)}
      toolName={entryType.tool_name}
      command={getToolCommand(entryType)}
      time={time}
    />
  );
}

function DisplayConversationEntry(props: Props) {
  const { t } = useTranslation('common');
  const { capabilities } = useUserSystem();
  const {
    entry,
    aggregatedGroup,
    aggregatedDiffGroup,
    aggregatedThinkingGroup,
    expansionKey,
    executionProcessId,
    workspaceWithSession,
    resetAction,
  } = props;
  const sessionId = workspaceWithSession?.session?.id;
  const executorCanFork = !!(
    workspaceWithSession?.session?.executor &&
    capabilities?.[workspaceWithSession.session.executor]?.includes(
      BaseAgentCapability.SESSION_FORK
    )
  );

  // Handle aggregated groups (consecutive file_read or search entries)
  if (aggregatedGroup) {
    return <AggregatedGroupEntry group={aggregatedGroup} />;
  }

  // Handle aggregated diff groups (consecutive file_edit entries for same file)
  if (aggregatedDiffGroup) {
    return <AggregatedDiffGroupEntry group={aggregatedDiffGroup} />;
  }

  // Handle aggregated thinking groups (thinking entries in previous turns)
  if (aggregatedThinkingGroup) {
    return (
      <AggregatedThinkingGroupEntry
        group={aggregatedThinkingGroup}
        workspaceId={workspaceWithSession?.id}
        sessionId={sessionId}
      />
    );
  }

  // If no entry, return null (shouldn't happen in normal usage)
  if (!entry) {
    return null;
  }

  const entryType = entry.entry_type;
  const time = formatStepTime(entry.timestamp);

  switch (entryType.type) {
    case 'tool_use':
      return renderToolUseEntry(entryType, entry, props, t);

    case 'user_message':
      return (
        <UserMessageEntry
          content={entry.content}
          expansionKey={expansionKey}
          workspaceId={workspaceWithSession?.id}
          sessionId={sessionId}
          executionProcessId={executionProcessId}
          executorCanFork={executorCanFork}
          resetAction={resetAction}
          time={time}
        />
      );

    case 'assistant_message':
      return (
        <AssistantMessageEntry
          content={entry.content}
          workspaceId={workspaceWithSession?.id}
          sessionId={sessionId}
          time={time}
        />
      );

    case 'system_message':
      return (
        <SystemMessageEntry
          content={entry.content}
          expansionKey={expansionKey}
        />
      );

    case 'thinking':
      return (
        <ChatThinkingMessage
          content={entry.content}
          workspaceId={workspaceWithSession?.id}
          renderMarkdown={({ content, workspaceId, className }) => (
            <AppChatMarkdown
              content={content}
              workspaceId={workspaceId}
              sessionId={sessionId}
              className={className}
              maxWidth={undefined}
            />
          )}
        />
      );

    case 'error_message':
      return (
        <ErrorMessageEntry
          content={entry.content}
          expansionKey={expansionKey}
          time={time}
        />
      );

    case 'next_action':
      // The new design doesn't need the next action bar
      return null;

    case 'token_usage_info':
      // Displayed in the chat header as the context-usage gauge
      return null;

    case 'user_feedback':
      return (
        <UserFeedbackEntry
          content={entry.content}
          deniedTool={entryType.denied_tool}
          workspaceId={workspaceWithSession?.id}
          sessionId={sessionId}
          time={time}
        />
      );

    case 'user_answered_questions':
      return (
        <UserAnsweredQuestionsEntry
          answers={entryType.answers}
          expansionKey={expansionKey}
          time={time}
        />
      );

    case 'loading':
      return <LoadingEntry />;

    default: {
      // Exhaustive check - TypeScript will error if a case is missing
      const _exhaustiveCheck: never = entryType;
      return _exhaustiveCheck;
    }
  }
}

/**
 * File edit entry with expandable diff
 */
function FileEntryDiffBody({
  diffContent,
}: {
  diffContent: ChatFileEntryDiffInput;
}) {
  const { theme } = useTheme();
  const actualTheme = getActualTheme(theme);
  const diffMode = useDiffViewMode();
  const diffData = useDiffData(diffContent);

  if (!diffData.isValid) {
    return null;
  }

  return (
    <DiffViewBody
      fileDiffMetadata={diffData.fileDiffMetadata}
      unifiedDiff={diffData.unifiedDiff}
      isValid={diffData.isValid}
      hideLineNumbers={diffData.hideLineNumbers}
      theme={actualTheme}
      diffMode={diffMode}
    />
  );
}

function AppChatMarkdown({
  content,
  workspaceId,
  sessionId,
  className,
  maxWidth,
}: {
  content: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  className: string | undefined;
  maxWidth: string | undefined;
}) {
  const { viewFileInChanges, findMatchingDiffPath } = useChangesViewActions();

  return (
    <ChatMarkdown
      content={content}
      workspaceId={workspaceId}
      className={className}
      maxWidth={maxWidth}
      renderContent={({ content, className, workspaceId }) => (
        <WYSIWYGEditor
          value={content}
          disabled
          className={className}
          workspaceId={workspaceId}
          sessionId={sessionId}
          findMatchingDiffPath={findMatchingDiffPath}
          onCodeClick={viewFileInChanges}
        />
      )}
    />
  );
}

function FileEditEntry({
  path,
  change,
  expansionKey,
  status,
  time,
}: {
  path: string;
  change: FileEditAction['changes'][number];
  expansionKey: string;
  status: ToolStatus;
  time: string | undefined;
}) {
  const { t } = useTranslation('common');
  // Auto-expand when pending approval
  const pendingApproval = status.status === 'pending_approval';
  const [expanded, toggle] = usePersistedExpanded(
    expansionKey as PersistKey,
    pendingApproval
  );
  const { viewFileInChanges, hasDiffPath } = useChangesViewActions();
  // Calculate diff stats for edit changes
  const { additions, deletions } = useMemo(() => {
    if (change.action === 'edit' && change.unified_diff) {
      return parseDiffStats(change.unified_diff);
    }
    return { additions: undefined, deletions: undefined };
  }, [change]);

  // For write actions, count as all additions
  const writeAdditions =
    change.action === 'write' ? change.content.split('\n').length : undefined;

  // Build diff content for rendering when expanded
  const diffContent: ChatFileEntryDiffInput | undefined = useMemo(() => {
    if (change.action === 'edit' && change.unified_diff) {
      return {
        type: 'unified',
        path,
        unifiedDiff: change.unified_diff,
        hasLineNumbers: change.has_line_numbers ?? true,
      };
    }
    // For write actions, use content-based diff (empty old, new content)
    if (change.action === 'write' && change.content) {
      return {
        type: 'content',
        oldContent: '',
        newContent: change.content,
        newPath: path,
      };
    }
    return undefined;
  }, [change, path]);
  const diffPreviewData = useDiffData(
    diffContent ?? { type: 'unified', path, unifiedDiff: '' }
  );
  const hasDiffContent = Boolean(diffContent && diffPreviewData.isValid);

  // Only show "open in changes" button if the file exists in current diffs
  const handleOpenInChanges = useCallback(() => {
    if (!hasDiffPath(path)) return;
    viewFileInChanges(path);
  }, [viewFileInChanges, hasDiffPath, path]);

  return (
    <ChatFileEntry
      label={fileChangeLabel(change.action, path, t)}
      filename={path}
      additions={additions ?? writeAdditions}
      deletions={deletions}
      time={time}
      expanded={expanded}
      onToggle={toggle}
      status={status}
      diffContent={hasDiffContent ? diffContent : undefined}
      renderDiffBody={
        hasDiffContent
          ? (entryDiffContent) => (
              <FileEntryDiffBody diffContent={entryDiffContent} />
            )
          : undefined
      }
      onOpenInChanges={handleOpenInChanges}
    />
  );
}

/**
 * Where the plan document can be opened from the decision card: the live
 * review page when one is open, else the RCA/spec file in the Review panel
 * when it is part of the diff.
 */
function usePlanDocumentLink(
  workspaceId: string | undefined
): ChatApprovalCardDecision['documentLink'] {
  const { t } = useTranslation('common');
  const { data: lavishSession } = useLavishSession(workspaceId);
  const sessionCards = useSessionCards({ enabled: Boolean(workspaceId) });
  const { viewFileInChanges, hasDiffPath } = useChangesViewActions();
  const card = workspaceId ? sessionCards.get(workspaceId) : undefined;
  const kind = card?.metadata?.planning?.kind;
  const label =
    kind === 'rca'
      ? t('conversation.decision.openRca')
      : kind === 'spec'
        ? t('conversation.decision.openSpec')
        : t('conversation.decision.openPlanDocument');

  const reviewUrl = lavishSession?.url;
  if (reviewUrl) {
    return {
      label,
      onClick: () => window.open(reviewUrl, '_blank', 'noopener,noreferrer'),
    };
  }
  const specPath = card?.metadata?.spec?.path ?? lavishSession?.file;
  if (specPath && hasDiffPath(specPath)) {
    return { label, onClick: () => viewFileInChanges(specPath) };
  }
  return null;
}

/**
 * The approve / request-changes controls for a tool call waiting on a
 * person. Returns null once the call is no longer pending.
 */
function useApprovalDecision({
  status,
  executionProcessId,
  workspaceId,
  helper,
}: {
  status: ToolStatus;
  executionProcessId: string | undefined;
  workspaceId: string | undefined;
  helper: string;
}): ChatApprovalCardDecision | null {
  const { t } = useTranslation('common');
  const queryClient = useQueryClient();
  const feedbackContext = useApprovalFeedbackOptional();
  const {
    approveAsync,
    denyAsync,
    isApproving,
    isDenying,
    approveError,
    denyError,
  } = useApprovalMutation();
  const [note, setNote] = useState('');
  const isPending = status.status === 'pending_approval';
  // Only a pending decision links the plan document, so resolved cards in
  // the history do not poll for a review page.
  const documentLink = usePlanDocumentLink(isPending ? workspaceId : undefined);

  if (!isPending || !executionProcessId) {
    return null;
  }
  const approvalId = status.approval_id;

  const refreshSummaries = () =>
    queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });

  return {
    helper,
    note,
    onNoteChange: setNote,
    noteLabel: t('conversation.decision.noteLabel'),
    notePlaceholder: t('conversation.decision.notePlaceholder'),
    approveLabel: t('conversation.decision.approve'),
    requestChangesLabel: t('conversation.decision.requestChanges'),
    onApprove: async () => {
      feedbackContext?.exitFeedbackMode();
      try {
        await approveAsync({ approvalId, executionProcessId });
        void refreshSummaries();
      } catch {
        // The mutation's error is shown on the card
      }
    },
    onRequestChanges: async () => {
      const reason = note.trim();
      if (!reason) return;
      try {
        await denyAsync({ approvalId, executionProcessId, reason });
        setNote('');
        void refreshSummaries();
      } catch {
        // The mutation's error is shown on the card
      }
    },
    isSubmitting: isApproving || isDenying,
    error: (approveError ?? denyError)?.message ?? null,
    documentLink,
  };
}

/** The chip naming a decided approval's outcome. */
function approvalOutcomeLabel(
  status: ToolStatus,
  t: TFunction<'common'>
): string | undefined {
  switch (status.status) {
    case 'success':
      return t('conversation.decision.approved');
    case 'denied':
      return t('conversation.decision.changesRequested');
    case 'timed_out':
      return t('conversation.decision.timedOut');
    default:
      return undefined;
  }
}

/**
 * Plan entry: the decision card while pending, a folded card after
 */
function PlanEntry({
  plan,
  expansionKey,
  workspaceId,
  sessionId,
  executionProcessId,
  status,
}: {
  plan: string;
  expansionKey: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  executionProcessId: string | undefined;
  status: ToolStatus;
}) {
  const { t } = useTranslation('common');
  // Expand plans by default when pending approval
  const pendingApproval = status.status === 'pending_approval';
  const [expanded, toggle] = usePersistedExpanded(
    `plan:${expansionKey}`,
    pendingApproval
  );
  const decision = useApprovalDecision({
    status,
    executionProcessId,
    workspaceId,
    helper: t('conversation.decision.planHelper'),
  });

  // Extract title from plan content (first line or default)
  const planTitle = useMemo(() => {
    const firstLine = plan.split('\n')[0];
    // Remove markdown heading markers
    const cleanTitle = firstLine.replace(/^#+\s*/, '').trim();
    return cleanTitle || t('conversation.plan');
  }, [plan, t]);

  return (
    <ChatApprovalCard
      title={t('conversation.decision.planTitle')}
      contentTitle={planTitle}
      content={plan}
      expanded={expanded}
      onToggle={toggle}
      workspaceId={workspaceId}
      status={status}
      outcomeLabel={approvalOutcomeLabel(status, t)}
      decision={decision}
      renderMarkdown={({ content, workspaceId }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={undefined}
          maxWidth={undefined}
        />
      )}
    />
  );
}

/**
 * Generic tool approval entry - the decision card while pending
 */
function GenericToolApprovalEntry({
  toolName,
  content,
  expansionKey,
  workspaceId,
  sessionId,
  executionProcessId,
  status,
}: {
  toolName: string;
  content: string;
  expansionKey: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  executionProcessId: string | undefined;
  status: ToolStatus;
}) {
  const { t } = useTranslation('common');
  const [expanded, toggle] = usePersistedExpanded(
    `tool:${expansionKey}`,
    true // auto-expand for pending approval
  );
  const toolLabel = humanizeToolName(toolName).label;
  const decision = useApprovalDecision({
    status,
    executionProcessId,
    workspaceId,
    helper: t('conversation.decision.toolHelper'),
  });

  return (
    <ChatApprovalCard
      title={t('conversation.decision.toolTitle', { tool: toolLabel })}
      contentTitle={toolLabel}
      content={content}
      expanded={expanded}
      onToggle={toggle}
      workspaceId={workspaceId}
      status={status}
      outcomeLabel={approvalOutcomeLabel(status, t)}
      decision={decision}
      renderMarkdown={({ content, workspaceId }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={undefined}
          maxWidth={undefined}
        />
      )}
    />
  );
}

/**
 * User message entry with expandable content
 */
function UserMessageEntry({
  content,
  expansionKey,
  workspaceId,
  sessionId,
  executionProcessId,
  executorCanFork,
  resetAction,
  time,
}: {
  content: string;
  expansionKey: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  executionProcessId: string | undefined;
  executorCanFork: boolean;
  resetAction: UseResetProcessResult;
  time: string | undefined;
}) {
  const [expanded, toggle] = usePersistedExpanded(`user:${expansionKey}`, true);
  const { startEdit, isEntryGreyed, isInEditMode } = useMessageEditContext();
  const { resetProcess, canResetProcess, isResetPending } = resetAction;

  const isGreyed = isEntryGreyed(expansionKey);

  const handleEdit = () => {
    if (executionProcessId) {
      startEdit(expansionKey, executionProcessId, content);
    }
  };

  const handleReset = () => {
    if (executionProcessId) {
      resetProcess(executionProcessId);
    }
  };

  // Only show actions when we have a process ID and not already in edit mode
  const canShowActions =
    !!executionProcessId && !isInEditMode && !isResetPending;
  // Edit/retry/reset is not supported when the executor doesn't have the fork capability
  const canEdit = canShowActions && executorCanFork;
  // Only show reset if we have a process ID, not in edit mode, and not pending
  const canReset = canEdit && canResetProcess(executionProcessId);

  return (
    <ChatUserMessage
      content={content}
      expanded={expanded}
      onToggle={toggle}
      time={time}
      workspaceId={workspaceId}
      onEdit={canEdit ? handleEdit : undefined}
      onReset={canReset ? handleReset : undefined}
      isGreyed={isGreyed}
      renderMarkdown={({ content, workspaceId }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={undefined}
          maxWidth={undefined}
        />
      )}
    />
  );
}

/**
 * User feedback entry for denied tool calls
 */
function UserFeedbackEntry({
  content,
  deniedTool,
  workspaceId,
  sessionId,
  time,
}: {
  content: string;
  deniedTool: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  time: string | undefined;
}) {
  const { t } = useTranslation('common');

  return (
    <ChatStep
      state="denied"
      label={t('conversation.deniedByUser', {
        toolName: humanizeToolName(deniedTool).label,
      })}
      time={time}
    >
      <div className="mt-1 max-w-[680px] rounded border bg-secondary px-3 py-2 text-sm text-normal">
        <WYSIWYGEditor
          value={content}
          disabled
          className="whitespace-pre-wrap break-words"
          workspaceId={workspaceId}
          sessionId={sessionId}
        />
      </div>
    </ChatStep>
  );
}

/**
 * User answered questions entry with expandable Q&A list
 */
function UserAnsweredQuestionsEntry({
  answers,
  expansionKey,
  time,
}: {
  answers: Extract<
    NormalizedEntry['entry_type'],
    { type: 'user_answered_questions' }
  >['answers'];
  expansionKey: string;
  time: string | undefined;
}) {
  const { t } = useTranslation('common');
  const [expanded, toggle] = usePersistedExpanded(
    `entry:${expansionKey}`,
    false
  );

  return (
    <ChatStep
      state="done"
      label={t('askQuestion.answeredCount', { count: answers.length })}
      time={time}
      expanded={expanded}
      onActivate={() => toggle()}
    >
      {expanded && (
        <div className="mt-1 flex max-w-[680px] flex-col gap-2 rounded border bg-secondary px-3 py-2.5">
          {answers.map((qa, i) => (
            <div key={i} className="flex flex-col gap-0.5">
              <span className="text-sm font-semibold text-high">
                {qa.question}
              </span>
              <span className="text-sm text-normal">
                {qa.answer.join(', ')}
              </span>
            </div>
          ))}
        </div>
      )}
    </ChatStep>
  );
}

/**
 * Loading placeholder entry
 */
function LoadingEntry() {
  return (
    <div className="flex items-center gap-3 py-1.5">
      <div className="size-6 shrink-0 animate-pulse rounded-full bg-panel" />
      <div className="flex flex-1 animate-pulse items-center gap-2">
        <div className="h-3 w-1/3 rounded-full bg-panel" />
        <div className="h-3 w-1/4 rounded-full bg-panel/70" />
      </div>
    </div>
  );
}

/**
 * Assistant message entry
 */
function AssistantMessageEntry({
  content,
  workspaceId,
  sessionId,
  time,
}: {
  content: string;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  time: string | undefined;
}) {
  const prefixedComment = useMemo(
    () => splitPrefixedComment(content),
    [content]
  );

  return (
    <ChatAssistantMessage
      content={content}
      workspaceId={workspaceId}
      time={time}
      prefixedComment={prefixedComment}
      renderMarkdown={({ content, workspaceId }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={undefined}
          maxWidth={undefined}
        />
      )}
    />
  );
}

/** The detail line of a step, in mono when it names code. */
function StepDetailText({ text, mono }: { text: string; mono: boolean }) {
  return <span className={cn(mono && 'font-mono')}>{text}</span>;
}

/**
 * A single tool call as a step; opens its output in the logs panel
 */
function ToolSummaryEntry({
  description,
  expansionKey,
  status,
  content,
  toolName,
  command,
  time,
}: {
  description: ToolDescription;
  expansionKey: string;
  status: ToolStatus;
  content: string;
  toolName: string;
  command: string | undefined;
  time: string | undefined;
}) {
  const [expanded, toggle] = usePersistedExpanded(
    `tool:${expansionKey}`,
    false
  );
  const { viewToolContentInPanel } = useLogsPanelActions();

  // Any tool with output can open the logs panel
  const hasOutput = content && content.trim().length > 0;

  const handleViewContent = useCallback(() => {
    viewToolContentInPanel(toolName, content, command);
  }, [viewToolContentInPanel, toolName, content, command]);

  const { detail, mono, comment } = description;
  const isLong = Boolean(detail && detail.length > LONG_DETAIL_CHARS);

  return (
    <ChatToolSummary
      label={description.label}
      detail={detail ? <StepDetailText text={detail} mono={mono} /> : undefined}
      fullDetail={isLong ? detail : undefined}
      time={time}
      expanded={expanded}
      onToggle={toggle}
      status={status}
      onViewContent={hasOutput ? handleViewContent : undefined}
    >
      {comment && (
        <ChatPrefixedComment
          prefix={comment.prefix}
          body={comment.body}
          className="mt-1.5"
        />
      )}
    </ChatToolSummary>
  );
}

/**
 * Todo management entry with expandable list of todos
 */
function TodoManagementEntry({
  todos,
  expansionKey,
  time,
}: {
  todos: TodoItem[];
  expansionKey: string;
  time: string | undefined;
}) {
  const [expanded, toggle] = usePersistedExpanded(
    `todo:${expansionKey}`,
    false
  );

  return (
    <ChatTodoList
      todos={todos}
      expanded={expanded}
      onToggle={toggle}
      time={time}
    />
  );
}

/**
 * Subagent/Task entry with expandable output
 */
function SubagentEntry({
  description,
  subagentType,
  result,
  expansionKey,
  status,
  workspaceId,
  sessionId,
  time,
}: {
  description: string;
  subagentType: string | null | undefined;
  result: ToolResult | null | undefined;
  expansionKey: string;
  status: ToolStatus;
  workspaceId: string | undefined;
  sessionId: string | undefined;
  time: string | undefined;
}) {
  // Only expandable if there's a result to show
  const hasResult = Boolean(result?.value);
  const [expanded, toggle] = usePersistedExpanded(
    `subagent:${expansionKey}`,
    false
  );

  return (
    <ChatSubagentEntry
      description={description}
      subagentType={subagentType}
      result={result}
      expanded={expanded}
      onToggle={hasResult ? toggle : undefined}
      status={status}
      workspaceId={workspaceId}
      time={time}
      renderMarkdown={({ content, workspaceId }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={undefined}
          maxWidth={undefined}
        />
      )}
    />
  );
}

/**
 * System message entry with expandable content
 */
function SystemMessageEntry({
  content,
  expansionKey,
}: {
  content: string;
  expansionKey: string;
}) {
  const [expanded, toggle] = usePersistedExpanded(
    `system:${expansionKey}`,
    false
  );

  return (
    <ChatSystemMessage
      content={content}
      expanded={expanded}
      onToggle={toggle}
    />
  );
}

/**
 * Script entry whose "fix" action opens the repository's Scripts panel in
 * Settings: scripts come from `.prepcook/scripts.toml` and are not edited here.
 */
function ScriptEntryWithFix({
  title,
  command,
  processId,
  exitCode,
  status,
  workspaceId,
  repos,
  time,
}: {
  title: string;
  command?: string;
  processId: string;
  exitCode: number | null;
  status: ToolStatus;
  workspaceId: string | undefined;
  repos: RepoWithTargetBranch[];
  time: string | undefined;
}) {
  const { viewProcessInPanel } = useLogsPanelActions();

  const reposRef = useRef(repos);
  reposRef.current = repos;

  const handleFix = useCallback(() => {
    const currentRepos = reposRef.current;
    if (currentRepos.length === 0) return;
    void SettingsDialog.show({
      initialSection: 'repos',
      initialState: {
        repoId: currentRepos.length === 1 ? currentRepos[0].id : undefined,
      },
    });
  }, []);

  // Only show fix button if we have the necessary context
  const canFix = workspaceId && repos.length > 0;

  return (
    <ChatScriptEntry
      title={title}
      command={command}
      processId={processId}
      exitCode={exitCode}
      status={status}
      time={time}
      onViewProcess={viewProcessInPanel}
      onFix={canFix ? handleFix : undefined}
    />
  );
}

/**
 * Error message entry with expandable content
 */
function ErrorMessageEntry({
  content,
  expansionKey,
  time,
}: {
  content: string;
  expansionKey: string;
  time: string | undefined;
}) {
  const [expanded, toggle] = usePersistedExpanded(
    `error:${expansionKey}`,
    false
  );

  return (
    <ChatErrorMessage
      content={content}
      time={time}
      expanded={expanded}
      onToggle={toggle}
    />
  );
}

/** Step label for a run of reads, searches, fetches or commands. */
function aggregatedLabel(
  group: AggregatedPatchGroup,
  count: number,
  t: TFunction<'common'>
): string {
  switch (group.aggregationType) {
    case 'file_read':
      return count === 1
        ? t('conversation.steps.readFile')
        : t('conversation.steps.readFiles', { count });
    case 'search':
      return count === 1
        ? t('conversation.steps.searched')
        : t('conversation.steps.searchedMany', { count });
    case 'web_fetch':
      return count === 1
        ? t('conversation.steps.fetched')
        : t('conversation.steps.fetchedMany', { count });
    case 'command_run_read':
    case 'command_run_search':
    case 'command_run_edit':
    case 'command_run_fetch':
      return count === 1
        ? t('conversation.steps.ranCommand')
        : t('conversation.steps.ranCommands', { count });
  }
}

/**
 * Aggregated group entry for consecutive file_read, search, or web_fetch entries
 */
function AggregatedGroupEntry({ group }: { group: AggregatedPatchGroup }) {
  const { t } = useTranslation('common');
  const { viewToolContentInPanel } = useLogsPanelActions();
  const [expanded, toggle] = usePersistedExpanded(
    `tool:${group.patchKey}`,
    false
  );

  // Extract summary and status from each entry in the group
  const aggregatedEntries = useMemo(() => {
    return group.entries.map((patchEntry) => {
      if (patchEntry.type !== 'NORMALIZED_ENTRY') {
        return {
          summary: '',
          status: undefined,
          expansionKey: patchEntry.patchKey,
          content: '',
          toolName: '',
        };
      }

      const entryType = patchEntry.content.entry_type;
      if (entryType.type !== 'tool_use') {
        return {
          summary: '',
          status: undefined,
          expansionKey: patchEntry.patchKey,
          content: '',
          toolName: '',
        };
      }

      const { action_type, status, tool_name } = entryType;
      let summary = '';
      let content = patchEntry.content.content;
      let command: string | undefined;
      if (action_type.action === 'file_read') {
        summary = action_type.path;
      } else if (action_type.action === 'search') {
        summary = action_type.query;
      } else if (action_type.action === 'web_fetch') {
        summary = action_type.url;
      } else if (action_type.action === 'command_run') {
        summary = action_type.command;
        command = action_type.command;
        content = action_type.result?.output ?? '';
      }

      return {
        summary,
        status,
        expansionKey: patchEntry.patchKey,
        content,
        toolName: tool_name,
        command,
      };
    });
  }, [group.entries]);

  const handleViewContent = useCallback(
    (index: number) => {
      const entry = aggregatedEntries[index];
      if (entry && entry.content) {
        viewToolContentInPanel(entry.toolName, entry.content, entry.command);
      }
    },
    [aggregatedEntries, viewToolContentInPanel]
  );

  const handleToggle = useCallback(() => {
    toggle();
  }, [toggle]);

  // Reads name the files; searches, fetches and commands quote themselves.
  const { shown, hidden } = summarizeList(
    aggregatedEntries.map((entry) =>
      group.aggregationType === 'file_read'
        ? fileBaseName(entry.summary)
        : entry.summary
    )
  );
  const detail = (
    <ChatStepDetail
      parts={[
        ...shown.map((item, index) => (
          <span key={index} className="font-mono">
            {item}
          </span>
        )),
        hidden > 0 ? t('conversation.steps.more', { count: hidden }) : null,
      ]}
    />
  );

  return (
    <ChatAggregatedToolEntries
      entries={aggregatedEntries}
      expanded={expanded}
      onToggle={handleToggle}
      onViewContent={handleViewContent}
      label={aggregatedLabel(group, aggregatedEntries.length, t)}
      detail={detail}
      time={firstEntryTime(group.entries)}
    />
  );
}

/**
 * Aggregated thinking group entry for thinking entries in previous turns
 */
function AggregatedThinkingGroupEntry({
  group,
  workspaceId,
  sessionId,
}: {
  group: AggregatedThinkingGroup;
  workspaceId: string | undefined;
  sessionId: string | undefined;
}) {
  const [expanded, toggle] = usePersistedExpanded(
    `entry:${group.patchKey}`,
    false
  );

  // Extract thinking entries from the group
  const thinkingEntries = useMemo(() => {
    return group.entries
      .filter((entry) => entry.type === 'NORMALIZED_ENTRY')
      .map((entry) => ({
        content: entry.type === 'NORMALIZED_ENTRY' ? entry.content.content : '',
        expansionKey: entry.patchKey,
      }));
  }, [group.entries]);

  const handleToggle = useCallback(() => {
    toggle();
  }, [toggle]);

  return (
    <ChatCollapsedThinking
      entries={thinkingEntries}
      expanded={expanded}
      onToggle={handleToggle}
      workspaceId={workspaceId}
      renderMarkdown={({ content, workspaceId, className }) => (
        <AppChatMarkdown
          content={content}
          workspaceId={workspaceId}
          sessionId={sessionId}
          className={className}
          maxWidth={undefined}
        />
      )}
    />
  );
}

function AggregatedDiffGroupEntry({ group }: { group: AggregatedDiffGroup }) {
  const { t } = useTranslation('common');
  const { viewFileInChanges, hasDiffPath } = useChangesViewActions();
  const [expanded, toggle] = usePersistedExpanded(
    `diff:${group.patchKey}`,
    false
  );
  // Extract change data and status from each entry
  const aggregatedDiffEntries = useMemo(() => {
    return group.entries.flatMap((patchEntry, entryIdx) => {
      if (patchEntry.type !== 'NORMALIZED_ENTRY') {
        return [];
      }

      const entryType = patchEntry.content.entry_type;
      if (entryType.type !== 'tool_use') {
        return [];
      }

      const { action_type, status } = entryType;
      if (action_type.action !== 'file_edit') {
        return [];
      }

      // Each file_edit entry can have multiple changes
      return action_type.changes.map((change, changeIdx) => ({
        change,
        status,
        expansionKey: `${patchEntry.patchKey}:${entryIdx}:${changeIdx}`,
      }));
    });
  }, [group.entries]);

  const handleToggle = useCallback(() => {
    toggle();
  }, [toggle]);

  const handleOpenInChanges = useCallback(() => {
    if (!hasDiffPath(group.filePath)) return;
    viewFileInChanges(group.filePath);
  }, [viewFileInChanges, hasDiffPath, group.filePath]);

  return (
    <ChatAggregatedDiffEntries
      label={t('conversation.steps.editedFile', {
        file: fileBaseName(group.filePath),
      })}
      filePath={group.filePath}
      entries={aggregatedDiffEntries}
      expanded={expanded}
      onToggle={handleToggle}
      onOpenInChanges={handleOpenInChanges}
      time={firstEntryTime(group.entries)}
      renderDiffBody={({ diffContent }) =>
        diffContent ? <FileEntryDiffBody diffContent={diffContent} /> : null
      }
    />
  );
}

const DisplayConversationEntrySpaced = (props: Props) => {
  const { isEntryGreyed } = useMessageEditContext();
  const isGreyed = isEntryGreyed(props.expansionKey);

  return (
    <div className={cn('px-6', isGreyed && 'opacity-50 pointer-events-none')}>
      <DisplayConversationEntry {...props} />
    </div>
  );
};

export default DisplayConversationEntrySpaced;
