import type { IssueExtMetadata, WorkClass } from 'shared/types';
import { WORKFLOW_STAGES } from '@/shared/lib/workflowTransitions';

/**
 * Board signals derived from what a card already carries (its column, its
 * `extension_metadata`, its pull requests and its sessions). Pure functions:
 * the board and the card panel both read them, so a card shows the same
 * state in both places.
 */

/** The workflow stages, in column order (see `WORKFLOW_STAGES`). */
export type WorkflowStageKey =
  | 'todo'
  | 'plan'
  | 'execute'
  | 'review'
  | 'pushMr'
  | 'close'
  | 'cleanUp';

const STAGE_KEYS: readonly WorkflowStageKey[] = [
  'todo',
  'plan',
  'execute',
  'review',
  'pushMr',
  'close',
  'cleanUp',
];

/**
 * The workflow stage a column name stands for, matching every alias in
 * `WORKFLOW_STAGES` case-insensitively. Null for columns outside the
 * workflow (custom columns, Triaged).
 */
export function workflowStageKey(
  statusName: string | null | undefined
): WorkflowStageKey | null {
  const normalized = statusName?.trim().toLowerCase();
  if (!normalized) return null;
  const index = WORKFLOW_STAGES.findIndex((aliases) =>
    aliases.some((alias) => alias.toLowerCase() === normalized)
  );
  return index === -1 ? null : (STAGE_KEYS[index] ?? null);
}

/** Column-header dot: hollow for To Do, brand while work runs, green at
 * Close, grey for Clean up. */
export type ColumnDotTone = 'hollow' | 'brand' | 'success' | 'muted';

export function columnDotTone(stage: WorkflowStageKey): ColumnDotTone {
  switch (stage) {
    case 'todo':
      return 'hollow';
    case 'plan':
    case 'execute':
    case 'review':
    case 'pushMr':
      return 'brand';
    case 'close':
      return 'success';
    case 'cleanUp':
      return 'muted';
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function stringOrNull(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null;
}

function timeOf(value: unknown): number | null {
  if (typeof value !== 'string') return null;
  const time = Date.parse(value);
  return Number.isNaN(time) ? null : time;
}

/** True while a person has taken the ticket over (automation hands-off). */
export function isTakenOver(meta: IssueExtMetadata | null): boolean {
  const takeover = meta?.human_takeover;
  return isRecord(takeover) && stringOrNull(takeover.detected_at) !== null;
}

/** A plan (RCA or spec) waiting for a person's decision. */
export interface PendingPlanGate {
  /** 'rca' for a bug's root-cause analysis, 'spec' for everything else. */
  planKind: 'rca' | 'spec';
  confidence: string | null;
  openQuestions: number | null;
}

/**
 * Whether the card's plan waits on a person: the planning agent recorded
 * its verdict, nobody has approved the plan yet, and no later decision
 * (request changes, re-plan) sent it back to the agent. Investigations
 * have no gate, and a card a person has taken over is theirs to drive.
 * Callers check the stage (decisions are only taken in Plan).
 */
export function pendingPlanGate(
  meta: IssueExtMetadata | null
): PendingPlanGate | null {
  if (!meta) return null;
  if (meta.work_class === 'investigation') return null;
  if (isTakenOver(meta)) return null;

  const planning = meta.planning;
  if (!isRecord(planning)) return null;
  const spec = meta.spec;
  if (isRecord(spec) && stringOrNull(spec.approved_at) !== null) return null;

  const recordedAt = timeOf(planning.recorded_at);
  const approvals = Array.isArray(meta.approvals) ? meta.approvals : [];
  const latest = approvals[approvals.length - 1];
  if (
    isRecord(latest) &&
    latest.decision !== 'approve' &&
    (recordedAt === null || (timeOf(latest.at) ?? 0) >= recordedAt)
  ) {
    return null;
  }

  const kind = stringOrNull(planning.kind)?.toLowerCase() ?? null;
  const confidence = stringOrNull(planning.confidence)?.toLowerCase() ?? null;
  return {
    planKind:
      kind === 'rca' || (kind === null && confidence !== null) ? 'rca' : 'spec',
    confidence,
    openQuestions:
      typeof planning.open_questions === 'number'
        ? planning.open_questions
        : null,
  };
}

/** The pull-request fields the board reads. */
export interface CardPullRequest {
  number: number;
  url: string;
  status: string;
  merged_at: string | null;
  created_at: string;
  workspace_id?: string | null;
}

/** The newest merged pull request, or null when none merged. */
export function latestMergedPullRequest<T extends CardPullRequest>(
  pullRequests: readonly T[]
): T | null {
  let latest: T | null = null;
  for (const pr of pullRequests) {
    if (pr.status !== 'merged') continue;
    const at = timeOf(pr.merged_at) ?? timeOf(pr.created_at) ?? 0;
    const latestAt = latest
      ? (timeOf(latest.merged_at) ?? timeOf(latest.created_at) ?? 0)
      : -1;
    if (at > latestAt) latest = pr;
  }
  return latest;
}

/** The newest pull request of any status, or null. */
export function latestPullRequest<T extends CardPullRequest>(
  pullRequests: readonly T[]
): T | null {
  let latest: T | null = null;
  for (const pr of pullRequests) {
    if (
      !latest ||
      (timeOf(pr.created_at) ?? 0) > (timeOf(latest.created_at) ?? 0)
    ) {
      latest = pr;
    }
  }
  return latest;
}

/** Why a card waits on a person. */
export type NeedsYouReason =
  | { kind: 'plan'; gate: PendingPlanGate }
  | { kind: 'close'; pullRequest: CardPullRequest };

/**
 * A card waits on a person when its plan is ready for a decision in Plan,
 * or when it sits in Close with its pull request merged.
 */
export function needsYouReason(
  stage: WorkflowStageKey | null,
  meta: IssueExtMetadata | null,
  pullRequests: readonly CardPullRequest[]
): NeedsYouReason | null {
  if (stage === 'plan') {
    const gate = pendingPlanGate(meta);
    return gate ? { kind: 'plan', gate } : null;
  }
  if (stage === 'close') {
    const merged = latestMergedPullRequest(pullRequests);
    return merged ? { kind: 'close', pullRequest: merged } : null;
  }
  return null;
}

/** The session fields the board reads. */
export interface CardSession {
  name: string | null;
  localWorkspaceId: string | null;
  updatedAt: string;
  createdAt?: string;
}

/** Sessions Prepcook opens for the independent review carry this prefix. */
export const REVIEW_SESSION_PREFIX = 'Review: ';

export function isReviewSession(session: Pick<CardSession, 'name'>): boolean {
  return (session.name ?? '').startsWith(REVIEW_SESSION_PREFIX);
}

/**
 * The card's current implementation session: the newest session that is
 * not an independent review and exists on this machine. Planning and
 * execution share it, so it is what "open the session" means for a card.
 */
export function implementationSession<T extends CardSession>(
  sessions: readonly T[],
  isLocal: (localWorkspaceId: string) => boolean
): T | null {
  let best: T | null = null;
  for (const session of sessions) {
    if (isReviewSession(session)) continue;
    if (!session.localWorkspaceId || !isLocal(session.localWorkspaceId)) {
      continue;
    }
    const at = timeOf(session.createdAt ?? session.updatedAt) ?? 0;
    const bestAt = best ? (timeOf(best.createdAt ?? best.updatedAt) ?? 0) : -1;
    if (at > bestAt) best = session;
  }
  return best;
}

/** Work-class chip recipe on cards and in the panel. */
export type WorkClassChipTone = 'bug' | 'story' | 'epic' | 'neutral';

export function workClassChipTone(workClass: WorkClass): WorkClassChipTone {
  switch (workClass) {
    case 'bug':
      return 'bug';
    case 'story':
      return 'story';
    case 'epic':
      return 'epic';
    case 'investigation':
    case 'subtask':
      return 'neutral';
  }
}

/** P0–P3 for the tracker-style priority chip. */
export function priorityChipLabel(
  priority: 'urgent' | 'high' | 'medium' | 'low' | null
): string | null {
  switch (priority) {
    case 'urgent':
      return 'P0';
    case 'high':
      return 'P1';
    case 'medium':
      return 'P2';
    case 'low':
      return 'P3';
    default:
      return null;
  }
}

/** Splits archived (Clean up) cards into this week and earlier. */
export function isWithinLastWeek(iso: string, now: number): boolean {
  const time = timeOf(iso);
  return time !== null && now - time < 7 * 24 * 60 * 60 * 1000;
}

/** Milliseconds elapsed since `iso`, or null when it does not parse. */
export function elapsedSince(iso: unknown, now: number): number | null {
  const time = timeOf(iso);
  return time === null ? null : Math.max(0, now - time);
}
