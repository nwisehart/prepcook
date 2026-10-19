import type { IssueExtMetadata } from 'shared/types';
import { WORKFLOW_STAGES } from '@/shared/lib/workflowTransitions';

/**
 * How the Sessions view describes a session: which group it sits in, the
 * one-line status under its title, and the colour that line wears. Everything
 * here is derived from what the sidebar already streams (workspace summary)
 * plus the linked card's column and planning metadata when the session is
 * linked to a card.
 */

export type SessionStage =
  | 'todo'
  | 'plan'
  | 'execute'
  | 'review'
  | 'shipped'
  | 'close'
  | 'cleanup';

// Same order as WORKFLOW_STAGES, one key per stage.
const STAGE_KEYS: readonly SessionStage[] = [
  'todo',
  'plan',
  'execute',
  'review',
  'shipped',
  'close',
  'cleanup',
];

/** Maps a column name (or one of its legacy aliases) to its stage. */
export function resolveSessionStage(
  statusName: string | null | undefined
): SessionStage | null {
  const normalized = statusName?.trim().toLowerCase();
  if (!normalized) return null;
  const index = WORKFLOW_STAGES.findIndex((aliases) =>
    aliases.some((alias) => alias.toLowerCase() === normalized)
  );
  return index >= 0 ? (STAGE_KEYS[index] ?? null) : null;
}

/**
 * The plan document waiting for a person: a planning verdict was recorded
 * while the card is in Plan and nobody has approved the RCA/spec yet.
 */
export function resolvePlanReady(
  metadata: IssueExtMetadata | null | undefined,
  stage: SessionStage | null
): 'rca' | 'spec' | null {
  if (stage !== 'plan') return null;
  const verdict = metadata?.planning;
  if (!verdict?.recorded_at) return null;
  if (metadata?.spec?.approved_at) return null;
  return verdict.kind === 'rca' ? 'rca' : 'spec';
}

export interface SessionStatusInput {
  isRunning?: boolean;
  hasPendingApproval?: boolean;
  hasUnseenActivity?: boolean;
  latestProcessStatus?: 'running' | 'completed' | 'failed' | 'killed';
  latestProcessCompletedAt?: string;
  prStatus?: 'open' | 'merged' | 'closed' | 'unknown';
  stage?: SessionStage | null;
  planReady?: 'rca' | 'spec' | null;
}

export type SessionStatusKind =
  /** A tool approval or agent question is waiting on a person. */
  | 'approval'
  /** The RCA or spec is written and waits for a decision. */
  | 'plan-ready'
  /** The pull request merged; the card waits to be closed. */
  | 'merged-ready'
  /** The agent finished a turn nobody has looked at yet. */
  | 'unseen'
  | 'running'
  | 'failed'
  | 'merged'
  | 'pr-open'
  | 'idle';

export type SessionStatusTone =
  | 'attention'
  | 'working'
  | 'error'
  | 'success'
  | 'muted';

export interface SessionStatus {
  kind: SessionStatusKind;
  tone: SessionStatusTone;
  /** True when the session is waiting on a person ("Needs you"). */
  needsYou: boolean;
}

const NEEDS_YOU = (kind: SessionStatusKind): SessionStatus => ({
  kind,
  tone: 'attention',
  needsYou: true,
});

export function deriveSessionStatus(input: SessionStatusInput): SessionStatus {
  if (input.hasPendingApproval) return NEEDS_YOU('approval');
  if (input.isRunning) {
    return { kind: 'running', tone: 'working', needsYou: false };
  }
  if (input.planReady) return NEEDS_YOU('plan-ready');
  if (input.prStatus === 'merged' && input.stage !== 'cleanup') {
    return NEEDS_YOU('merged-ready');
  }
  if (input.hasUnseenActivity) return NEEDS_YOU('unseen');
  if (
    input.latestProcessStatus === 'failed' ||
    input.latestProcessStatus === 'killed'
  ) {
    return { kind: 'failed', tone: 'error', needsYou: false };
  }
  if (input.prStatus === 'merged') {
    return { kind: 'merged', tone: 'success', needsYou: false };
  }
  if (input.prStatus === 'open') {
    return { kind: 'pr-open', tone: 'muted', needsYou: false };
  }
  return { kind: 'idle', tone: 'muted', needsYou: false };
}

export type SessionGroup = 'needs-you' | 'running' | 'done-today' | 'earlier';

export const SESSION_GROUP_ORDER: readonly SessionGroup[] = [
  'needs-you',
  'running',
  'done-today',
  'earlier',
];

function isSameLocalDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/**
 * Needs you first, then running, then the rest split by whether the session
 * was last active today. A session with no recorded activity counts as
 * earlier.
 */
export function groupSession(
  status: SessionStatus,
  lastActiveAt: string | undefined,
  now: Date = new Date()
): SessionGroup {
  if (status.needsYou) return 'needs-you';
  if (status.kind === 'running') return 'running';
  if (!lastActiveAt) return 'earlier';
  const at = new Date(lastActiveAt);
  if (Number.isNaN(at.getTime())) return 'earlier';
  return isSameLocalDay(at, now) ? 'done-today' : 'earlier';
}

export interface ElapsedParts {
  unit: 'minutes' | 'hours' | 'days';
  count: number;
}

/**
 * Whole minutes, hours or days since `iso`, never below one minute so a
 * fresh event still reads "1 min". Null when the timestamp is missing or
 * unparseable.
 */
export function elapsedSince(
  iso: string | null | undefined,
  now: Date = new Date()
): ElapsedParts | null {
  if (!iso) return null;
  const at = new Date(iso).getTime();
  if (Number.isNaN(at)) return null;
  const minutes = Math.max(1, Math.floor((now.getTime() - at) / 60_000));
  if (minutes < 60) return { unit: 'minutes', count: minutes };
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return { unit: 'hours', count: hours };
  return { unit: 'days', count: Math.floor(hours / 24) };
}

/** True when the stage is past the agent's work: an older board's Close,
 * and Clean up. */
export function isFinishedStage(stage: SessionStage | null): boolean {
  return stage === 'close' || stage === 'cleanup';
}
