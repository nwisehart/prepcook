import type { IssueExtMetadata, WorkClass } from 'shared/types';
import type { KanbanCardStatusLine } from '@prepcook/ui/components/KanbanCardContent';
import {
  formatElapsed,
  formatRelativeTime,
  type KanbanTranslate,
} from '@prepcook/ui/components/KanbanRelativeTime';
import {
  elapsedSince,
  latestMergedPullRequest,
  latestPullRequest,
  type CardPullRequest,
  type NeedsYouReason,
  type PendingPlanGate,
  type ShippedIndicator,
  type WorkflowStageKey,
} from './cardSignals';

/** The helper line under each workflow column's name. */
export function columnHelper(
  t: KanbanTranslate,
  stage: WorkflowStageKey
): string {
  return t(`kanban.columnHelpers.${stage}`);
}

/** "Needs you · the RCA is ready" and its siblings. */
export function needsYouLabel(
  t: KanbanTranslate,
  reason: NeedsYouReason
): string {
  switch (reason.kind) {
    case 'triaged':
      return t('kanban.needsYou.triaged');
    case 'merged':
      return t('kanban.needsYou.merged');
    case 'closedUnmerged':
      return t('kanban.needsYou.closedUnmerged');
    case 'plan':
      return reason.gate.planKind === 'rca'
        ? t('kanban.needsYou.rcaReady')
        : t('kanban.needsYou.planReady');
  }
}

/** The Shipped column's chip ("PR open", "CI failing", "Merged", …). */
export function shippedIndicatorLabel(
  t: KanbanTranslate,
  indicator: ShippedIndicator
): string {
  return t(`kanban.shippedIndicator.${indicator.kind}`);
}

/** "Confidence high · no open questions": what the agent's verdict says. */
export function planGateDetail(
  t: KanbanTranslate,
  gate: PendingPlanGate
): string {
  const parts: string[] = [];
  if (gate.confidence) {
    parts.push(
      t('kanban.needsYou.confidence', {
        level: t(`kanban.needsYou.confidenceLevels.${gate.confidence}`, {
          defaultValue: gate.confidence,
        }),
      })
    );
  }
  if (gate.openQuestions !== null) {
    parts.push(
      gate.openQuestions === 0
        ? t('kanban.needsYou.noOpenQuestions')
        : t('kanban.verdictOpenQuestions', { count: gate.openQuestions })
    );
  }
  return parts.join(' · ');
}

/** "PR #212". */
export function pullRequestLabel(t: KanbanTranslate, number: number): string {
  return t('kanban.card.pullRequest', { number });
}

export interface StatusLineSession {
  isRunning?: boolean;
  hasPendingApproval?: boolean;
  latestProcessStatus?: 'running' | 'completed' | 'failed' | 'killed';
  latestProcessCompletedAt?: string;
  updatedAt: string;
}

export interface StatusLineInput {
  stage: WorkflowStageKey | null;
  workClass: WorkClass | null;
  meta: IssueExtMetadata | null;
  createdAt: string;
  needsYou: NeedsYouReason | null;
  /** The orchestrator tagged the card `agent-failed`. */
  agentFailed: boolean;
  pushState: 'pushing' | 'pushed' | 'failed' | null;
  agentQueued: boolean;
  pullRequests: readonly CardPullRequest[];
  mrUrl: string | null;
  /** The running session when there is one, else the current one. */
  session: StatusLineSession | null;
  origin: { text: string; linkText: string; url: string | null } | null;
}

function runningLabel(
  t: KanbanTranslate,
  input: StatusLineInput,
  now: number
): string {
  const elapsedMs = elapsedSince(input.meta?.playbook_started_at, now);
  const withElapsed = (label: string) =>
    elapsedMs === null ? label : `${label} · ${formatElapsed(t, elapsedMs)}`;

  switch (input.stage) {
    case 'plan':
      return withElapsed(t('kanban.card.status.planning'));
    case 'execute':
      return withElapsed(
        input.workClass === 'bug'
          ? t('kanban.card.status.writingFix')
          : input.workClass === 'epic'
            ? t('kanban.card.status.creatingSubtasks')
            : t('kanban.card.status.implementing')
      );
    case 'review': {
      const rounds = input.meta?.autonomy?.review_rounds;
      return typeof rounds === 'number' && rounds > 0
        ? `${t('kanban.card.status.reviewing')} · ${t(
            'kanban.card.status.fixRound',
            { n: rounds }
          )}`
        : withElapsed(t('kanban.card.status.reviewing'));
    }
    case 'shipped':
      return t('kanban.card.status.openingPr');
    case 'cleanUp':
      return t('kanban.card.status.cleaningUp');
    case 'todo':
    case 'close':
    case null:
      return withElapsed(t('kanban.card.status.working'));
  }
}

/**
 * The card's one status line, from what it already carries: a merged pull
 * request, a running session (with the stage verb and how long the stage
 * has run), an open pull request, or when it was imported or added. Null
 * for a card whose plan waits on a decision: its strip and verdict line say
 * it instead.
 */
export function buildStatusLine(
  t: KanbanTranslate,
  input: StatusLineInput,
  now: number
): KanbanCardStatusLine | null {
  if (input.needsYou?.kind === 'plan') return null;
  if (input.needsYou?.kind === 'triaged') {
    return { tone: 'attention', text: t('kanban.card.status.triaged') };
  }

  const pullRequests = input.pullRequests;
  const merged = latestMergedPullRequest(pullRequests);
  const mergedLine = (pr: CardPullRequest): KanbanCardStatusLine => ({
    tone: 'merged',
    link: { label: pullRequestLabel(t, pr.number), url: pr.url },
    text: t('kanban.card.status.mergedWhen', {
      when: formatRelativeTime(t, pr.merged_at ?? pr.created_at, now),
    }),
  });

  if (input.needsYou?.kind === 'merged') {
    return mergedLine(input.needsYou.pullRequest);
  }

  if (input.pushState === 'failed') {
    return {
      tone: 'error',
      text: t('kanban.pushStates.failed'),
      title: t('kanban.pushStates.failedHint'),
    };
  }

  if (input.agentFailed) {
    return { tone: 'error', text: t('kanban.card.status.agentFailed') };
  }

  const session = input.session;
  if (session?.isRunning) {
    if (session.hasPendingApproval) {
      return {
        tone: 'attention',
        text: t('kanban.card.status.waitingApproval'),
      };
    }
    return { tone: 'working', text: runningLabel(t, input, now) };
  }

  if (input.agentQueued) {
    return {
      tone: 'working',
      text: t('kanban.agentQueued'),
      title: t('kanban.agentQueuedHint'),
      pulse: true,
    };
  }
  if (input.pushState === 'pushing') {
    return {
      tone: 'working',
      text: t('kanban.card.status.openingPr'),
      pulse: true,
    };
  }

  if (merged) return mergedLine(merged);

  const latest = latestPullRequest(pullRequests);
  if (latest) {
    return {
      tone: 'idle',
      icon: 'pullRequest',
      link: { label: pullRequestLabel(t, latest.number), url: latest.url },
      text:
        latest.status === 'closed'
          ? t('kanban.card.status.prClosed')
          : t('kanban.card.status.openedWhen', {
              when: formatRelativeTime(t, latest.created_at, now),
            }),
    };
  }
  if (input.pushState === 'pushed' && input.mrUrl) {
    return {
      tone: 'idle',
      icon: 'pullRequest',
      link: { label: t('kanban.pushStates.pushed'), url: input.mrUrl },
      text: '',
    };
  }

  const approvedAt = input.meta?.spec?.approved_at;
  if (input.stage === 'plan' && typeof approvedAt === 'string') {
    return {
      tone: 'idle',
      icon: 'clock',
      text: t('kanban.card.status.planApproved', {
        when: formatRelativeTime(t, approvedAt, now),
      }),
    };
  }

  if (session) {
    const when = formatRelativeTime(
      t,
      session.latestProcessCompletedAt ?? session.updatedAt,
      now
    );
    if (
      session.latestProcessStatus === 'failed' ||
      session.latestProcessStatus === 'killed'
    ) {
      return {
        tone: 'error',
        text: t('kanban.card.status.sessionFailed', { when }),
      };
    }
    if (session.latestProcessStatus === 'completed') {
      return {
        tone: 'idle',
        icon: 'clock',
        text: t('kanban.card.status.agentFinished', { when }),
      };
    }
  }

  if (input.origin) {
    return {
      tone: 'idle',
      icon: 'clock',
      text: input.origin.text,
      ...(input.origin.url
        ? {
            link: { label: input.origin.linkText, url: input.origin.url },
            linkAt: 'end' as const,
          }
        : { text: `${input.origin.text} ${input.origin.linkText}` }),
    };
  }

  const when = formatRelativeTime(t, input.createdAt, now);
  return {
    tone: 'idle',
    icon: 'clock',
    text: input.meta?.jira
      ? t('kanban.card.status.imported', { when })
      : t('kanban.card.status.added', { when }),
  };
}
