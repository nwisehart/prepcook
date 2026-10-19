/**
 * Client-side mirror of the Prepcook workflow transition rules enforced by the
 * remote server in `crates/remote/src/workflow.rs`. Keep the two
 * implementations in sync.
 */

/**
 * Ordered Prepcook workflow stages; later aliases are earlier column names kept
 * so existing projects keep validating until renamed. Columns with other
 * names are exempt — a Triaged column a board added among them: triage is a
 * state of the card. A current board has one Shipped column where an older one
 * has Push MR and Close: Shipped shares Push MR's place, and Close keeps its
 * own. Done is Shipped's name on a board made while the column was called that.
 */
export const WORKFLOW_STAGES: readonly (readonly string[])[] = [
  ['To Do', 'Unassigned'],
  ['Plan', 'Planning'],
  ['Execute', 'Executing'],
  ['Review', 'Reviewing'],
  ['Shipped', 'Done', 'Push MR', 'Pushed MR'],
  ['Close', 'Deployed', 'Closed'],
  ['Clean up', 'Clean-up', 'Cleanup'],
] as const;

const NORMALIZED_STAGES = WORKFLOW_STAGES.map((aliases) =>
  aliases.map((name) => name.toLowerCase())
);

const EXECUTE_INDEX = NORMALIZED_STAGES.findIndex((aliases) =>
  aliases.includes('execute')
);

const CLOSED_INDEX = NORMALIZED_STAGES.findIndex((aliases) =>
  aliases.includes('close')
);

const CLEANUP_INDEX = NORMALIZED_STAGES.findIndex((aliases) =>
  aliases.includes('clean up')
);

function stageIndex(name: string | null | undefined): number {
  if (!name) return -1;
  const normalized = name.trim().toLowerCase();
  return NORMALIZED_STAGES.findIndex((aliases) => aliases.includes(normalized));
}

/**
 * Returns true when moving an issue from `fromName` to `toName` is allowed.
 *
 * Rules (stage names are matched case-insensitively):
 * - Either name outside the Prepcook stage list => allowed (non-workflow project
 *   or hidden/custom columns).
 * - Same stage (no-op) or any backward move => allowed.
 * - Forward moves are only allowed to the immediately next stage.
 * - Exceptions: any stage may move directly to Clean up, where
 *   `cleanupGate` reads the card's pull requests, or to an older board's
 *   Close (manual close).
 */
export function isTransitionAllowed(
  fromName: string | null | undefined,
  toName: string | null | undefined
): boolean {
  const from = stageIndex(fromName);
  const to = stageIndex(toName);
  if (from === -1 || to === -1) return true;
  if (to <= from) return true;
  if (to === CLOSED_INDEX || to === CLEANUP_INDEX) return true;
  return to === from + 1;
}

/** Whether `name` is the Clean up column, under any of its names. */
export function isCleanupStage(name: string | null | undefined): boolean {
  return stageIndex(name) === CLEANUP_INDEX;
}

/** Where a card's pull requests stand, taken together (`pr_state`). */
export type PrState = 'open' | 'merged' | 'closed';

/**
 * Mirror of `pr_state` in `crates/api-types/src/board.rs`: open while any is
 * open, merged when none is open and one merged, closed when every one
 * closed unmerged, null for a card with none.
 */
export function prState(
  pullRequests: readonly { status: string }[]
): PrState | null {
  if (pullRequests.length === 0) return null;
  if (pullRequests.some((pr) => pr.status === 'open')) return 'open';
  if (pullRequests.some((pr) => pr.status === 'merged')) return 'merged';
  return 'closed';
}

/** What Clean up says to a card. */
export type CleanupGate =
  | { kind: 'allowed' }
  | { kind: 'refused'; reason: string }
  /** The pull request closed unmerged: allowed once the discard is confirmed. */
  | { kind: 'confirm' };

/**
 * Mirror of `cleanup_block_reason`: Clean up takes a merged card or one with
 * no pull request, refuses an open pull request, and takes a closed,
 * unmerged one only with `discard_confirmed_at` recorded.
 */
export function cleanupGate(
  state: PrState | null,
  extensionMetadata: unknown
): CleanupGate {
  if (state === null || state === 'merged') return { kind: 'allowed' };
  if (state === 'open') {
    return {
      kind: 'refused',
      reason:
        'the pull request is still open — merge or close it before cleaning the card up',
    };
  }
  const metadata =
    extensionMetadata && typeof extensionMetadata === 'object'
      ? (extensionMetadata as Record<string, unknown>)
      : {};
  return metadata.discard_confirmed_at != null
    ? { kind: 'allowed' }
    : { kind: 'confirm' };
}

/**
 * Client-side mirror of `planning_gate_error` in
 * `crates/remote/src/workflow.rs`: the Planning → Executing move requires a
 * passing planning verdict (spec: `open_questions === 0`; rca:
 * `confidence === 'high'`) AND a human Lavish approval
 * (`spec.approved_at`). Generic tickets (no work_class and no planning
 * verdict) are exempt. An investigation is refused on every move into
 * Execute.
 *
 * Returns `null` when the move is allowed, or a short human-readable reason.
 */
export function planningGateError(
  fromName: string | null | undefined,
  toName: string | null | undefined,
  extensionMetadata: unknown
): string | null {
  const metadata =
    extensionMetadata && typeof extensionMetadata === 'object'
      ? (extensionMetadata as Record<string, unknown>)
      : {};
  if (
    stageIndex(toName) === EXECUTE_INDEX &&
    stageIndex(fromName) !== EXECUTE_INDEX &&
    metadata.work_class === 'investigation'
  ) {
    return 'investigation cards do not execute; convert to a bug first';
  }

  const isPlanningToExecuting =
    (fromName ?? '').trim().toLowerCase() === 'planning' &&
    (toName ?? '').trim().toLowerCase() === 'executing';
  if (!isPlanningToExecuting) return null;

  const planning =
    metadata.planning && typeof metadata.planning === 'object'
      ? (metadata.planning as Record<string, unknown>)
      : null;

  // Generic tickets never went through the Prepcook planning flow.
  if (metadata.work_class == null && planning == null) return null;

  if (planning == null) {
    return 'no planning verdict recorded yet';
  }

  const kind =
    typeof planning.kind === 'string' ? planning.kind.trim().toLowerCase() : '';
  if (kind === 'spec') {
    if (planning.open_questions !== 0) {
      return 'spec still has open questions';
    }
  } else if (kind === 'rca') {
    if (
      typeof planning.confidence !== 'string' ||
      planning.confidence.toLowerCase() !== 'high'
    ) {
      return 'RCA confidence must be high';
    }
  } else {
    return 'planning verdict is missing its kind ("spec" or "rca")';
  }

  const spec =
    metadata.spec && typeof metadata.spec === 'object'
      ? (metadata.spec as Record<string, unknown>)
      : null;
  if (!spec || spec.approved_at == null) {
    return 'spec/RCA not approved in the Lavish review gate yet';
  }

  return null;
}
