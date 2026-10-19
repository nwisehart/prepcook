import type { Issue, ProjectStatus } from 'shared/remote-types';
import type { KbDocumentSummaryT, KbEdgeT, KbNodeT } from '@/shared/lib/api';
import { WORKFLOW_STAGES } from '@/shared/lib/workflowTransitions';

/** Whole-day bounds as `YYYY-MM-DD` strings; an empty string is unbounded. */
export interface DayRange {
  since: string;
  until: string;
}

export interface CountRow {
  value: string;
  count: number;
}

export interface NodeStat {
  node: KbNodeT;
  /** RCAs in the current range linked to this node. */
  count: number;
  /** The most recently filed of those RCAs. */
  latest: KbDocumentSummaryT | null;
}

/**
 * The UTC calendar day of a timestamp. The export endpoint bounds documents
 * by `created_at.date_naive()` in UTC, so the page filters on the same day to
 * keep what is shown and what is exported identical.
 */
export function utcDay(timestamp: string): string {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime())
    ? timestamp.slice(0, 10)
    : date.toISOString().slice(0, 10);
}

export function isInDayRange(timestamp: string, range: DayRange): boolean {
  const day = utcDay(timestamp);
  return (
    (!range.since || day >= range.since) && (!range.until || day <= range.until)
  );
}

/**
 * Counts documents per value of one `namespace:` tag. A document carrying
 * several values of the namespace (e.g. two `detectable-by:` tags) counts
 * once for each.
 */
export function tagValueCounts(
  documents: KbDocumentSummaryT[],
  namespace: string,
  order: 'count' | 'value' = 'count'
): CountRow[] {
  const prefix = `${namespace}:`;
  const counts = new Map<string, number>();
  for (const doc of documents) {
    for (const tag of doc.tags) {
      if (!tag.startsWith(prefix)) continue;
      const value = tag.slice(prefix.length);
      counts.set(value, (counts.get(value) ?? 0) + 1);
    }
  }
  const rows = [...counts.entries()].map(([value, count]) => ({
    value,
    count,
  }));
  return order === 'value'
    ? rows.sort((a, b) => a.value.localeCompare(b.value))
    : rows.sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));
}

/**
 * Per-node RCA counts and most recent RCA, over the given documents only (so
 * a date range narrows them). With every document passed this matches the
 * server's `edge_count` for nodes linked only to RCAs.
 */
export function nodeStats(
  nodes: KbNodeT[],
  edges: KbEdgeT[],
  documentsById: Map<string, KbDocumentSummaryT>
): Map<string, NodeStat> {
  const stats = new Map<string, NodeStat>(
    nodes.map((node) => [node.id, { node, count: 0, latest: null }])
  );
  for (const edge of edges) {
    const stat = stats.get(edge.node_id);
    const doc = documentsById.get(edge.document_id);
    if (!stat || !doc) continue;
    stat.count += 1;
    if (
      !stat.latest ||
      Date.parse(doc.created_at) > Date.parse(stat.latest.created_at)
    ) {
      stat.latest = doc;
    }
  }
  return stats;
}

/** Most recently filed first. */
export function sortByNewest(
  documents: KbDocumentSummaryT[]
): KbDocumentSummaryT[] {
  return [...documents].sort(
    (a, b) => Date.parse(b.created_at) - Date.parse(a.created_at)
  );
}

/** Nodes of one kind with at least one RCA, most RCAs first. */
export function rankedNodes(
  stats: Map<string, NodeStat>,
  kind: string
): NodeStat[] {
  return [...stats.values()]
    .filter((stat) => stat.node.kind === kind && stat.count > 0)
    .sort(
      (a, b) => b.count - a.count || a.node.name.localeCompare(b.node.name)
    );
}

/** How many of the documents have no link to a node of `kind`. */
export function countUnlinked(
  documents: KbDocumentSummaryT[],
  edges: KbEdgeT[],
  nodes: KbNodeT[],
  kind: string
): number {
  const kindNodeIds = new Set(
    nodes.filter((node) => node.kind === kind).map((node) => node.id)
  );
  const linked = new Set(
    edges
      .filter((edge) => kindNodeIds.has(edge.node_id))
      .map((edge) => edge.document_id)
  );
  return documents.filter((doc) => !linked.has(doc.id)).length;
}

const PLAN_STAGE_NAMES = new Set(
  (WORKFLOW_STAGES.find((aliases) => aliases.includes('Plan')) ?? []).map(
    (name) => name.toLowerCase()
  )
);

function metadataObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object'
    ? (value as Record<string, unknown>)
    : null;
}

/**
 * RCAs filed as drafts and still waiting at the Plan gate: the document's card
 * sits in the Plan column and its RCA has no approval (`spec.approved_at`).
 * The document itself carries no approval state; it is filed once when the
 * verdict lands and again on approval. Investigations are never approved, so
 * they are never drafts.
 */
export function draftDocumentIds(
  documents: KbDocumentSummaryT[],
  issuesById: Map<string, Issue>,
  statuses: ProjectStatus[]
): Set<string> {
  const planStatusIds = new Set(
    statuses
      .filter((status) =>
        PLAN_STAGE_NAMES.has(status.name.trim().toLowerCase())
      )
      .map((status) => status.id)
  );
  const drafts = new Set<string>();
  if (planStatusIds.size === 0) return drafts;
  for (const doc of documents) {
    if (!doc.issue_id || doc.work_class === 'investigation') continue;
    const issue = issuesById.get(doc.issue_id);
    if (!issue || !planStatusIds.has(issue.status_id)) continue;
    const spec = metadataObject(metadataObject(issue.extension_metadata)?.spec);
    if (spec?.approved_at == null) drafts.add(doc.id);
  }
  return drafts;
}

/**
 * Display form of a tag value: dashes become spaces and the first letter is
 * capitalised; two-letter codes (`p0`, `qa`) are upper-cased.
 */
export function humanizeTagValue(value: string): string {
  if (value.length <= 2) return value.toUpperCase();
  const spaced = value.replace(/-/g, ' ');
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/** Whole local calendar days between a timestamp and `now` (0 = today). */
export function calendarDaysAgo(timestamp: string, now: Date): number {
  const date = new Date(timestamp);
  const startOf = (value: Date) =>
    new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  return Math.round((startOf(now) - startOf(date)) / 86_400_000);
}
