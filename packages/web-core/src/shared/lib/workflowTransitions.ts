/**
 * Client-side mirror of the Prep Cook workflow transition rules enforced by the
 * remote server in `crates/remote/src/workflow.rs`. Keep the two
 * implementations in sync.
 */

/**
 * Ordered Prep Cook workflow stages; later aliases are earlier column names kept
 * so existing projects keep validating until renamed. Columns with other
 * names are exempt.
 */
export const WORKFLOW_STAGES: readonly (readonly string[])[] = [
  ['To Do', 'Unassigned'],
  ['Plan', 'Planning'],
  ['Execute', 'Executing'],
  ['Review', 'Reviewing'],
  ['Push MR', 'Pushed MR'],
  ['Close', 'Deployed', 'Closed'],
  ['Clean up', 'Clean-up', 'Cleanup'],
] as const;

const NORMALIZED_STAGES = WORKFLOW_STAGES.map((aliases) =>
  aliases.map((name) => name.toLowerCase())
);

const CLOSED_INDEX = NORMALIZED_STAGES.findIndex((aliases) =>
  aliases.includes('close')
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
 * - Either name outside the Prep Cook stage list => allowed (non-workflow project
 *   or hidden/custom columns).
 * - Same stage (no-op) or any backward move => allowed.
 * - Forward moves are only allowed to the immediately next stage.
 * - Exception: any stage may move directly to "Closed" (manual close).
 * - Consequence: "Clean-up" is only reachable from "Closed".
 */
export function isTransitionAllowed(
  fromName: string | null | undefined,
  toName: string | null | undefined
): boolean {
  const from = stageIndex(fromName);
  const to = stageIndex(toName);
  if (from === -1 || to === -1) return true;
  if (to <= from) return true;
  if (to === CLOSED_INDEX) return true;
  return to === from + 1;
}

/**
 * Client-side mirror of `planning_gate_error` in
 * `crates/remote/src/workflow.rs`: the Planning → Executing move requires a
 * passing planning verdict (spec: `open_questions === 0`; rca:
 * `confidence === 'high'`) AND a human Lavish approval
 * (`spec.approved_at`). Generic tickets (no work_class and no planning
 * verdict) are exempt.
 *
 * Returns `null` when the move is allowed, or a short human-readable reason.
 */
export function planningGateError(
  fromName: string | null | undefined,
  toName: string | null | undefined,
  extensionMetadata: unknown
): string | null {
  const isPlanningToExecuting =
    (fromName ?? '').trim().toLowerCase() === 'planning' &&
    (toName ?? '').trim().toLowerCase() === 'executing';
  if (!isPlanningToExecuting) return null;

  const metadata =
    extensionMetadata && typeof extensionMetadata === 'object'
      ? (extensionMetadata as Record<string, unknown>)
      : {};
  const planning =
    metadata.planning && typeof metadata.planning === 'object'
      ? (metadata.planning as Record<string, unknown>)
      : null;

  // Generic tickets never went through the Prep Cook planning flow.
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
