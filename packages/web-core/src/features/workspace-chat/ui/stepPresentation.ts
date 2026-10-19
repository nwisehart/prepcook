import type { JsonValue } from 'shared/types';

/** Clock time of a transcript entry ("10:02"), in the viewer's locale. */
export function formatStepTime(
  iso: string | null | undefined
): string | undefined {
  if (!iso) return undefined;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return undefined;
  return date.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Last path segment: "src/import/quantity.ts" → "quantity.ts". */
export function fileBaseName(path: string): string {
  const trimmed = path.replace(/\/+$/, '');
  return trimmed.slice(trimmed.lastIndexOf('/') + 1) || path;
}

export interface PrefixedComment {
  prefix: string;
  body: string;
}

// Board comments that carry the workflow (see docs/method.md and
// docs/review.md): the harness's reproduction, a review verdict, a fix
// round's rejection of a finding, and the orchestrator's gate notes.
// Longer prefixes come first so REVIEW-REJECTED: is not read as REVIEW:.
const COMMENT_PREFIXES = [
  'REPRODUCTION:',
  'REVIEW-REJECTED:',
  'REVIEW:',
  'gate:',
] as const;

/** Splits `REPRODUCTION: …` style text into its prefix and the rest. */
export function splitPrefixedComment(
  text: string | null | undefined
): PrefixedComment | null {
  if (!text) return null;
  const trimmed = text.trimStart();
  const prefix = COMMENT_PREFIXES.find((p) => trimmed.startsWith(p));
  if (!prefix) return null;
  return { prefix, body: trimmed.slice(prefix.length).trim() };
}

/**
 * Finds a prefixed board comment among a tool call's string arguments, e.g.
 * the `message` an agent passes to `add_issue_comment`.
 */
export function findPrefixedCommentInArgs(
  args: JsonValue | null | undefined
): PrefixedComment | null {
  if (!args || typeof args !== 'object' || Array.isArray(args)) return null;
  for (const value of Object.values(args)) {
    if (typeof value !== 'string') continue;
    const comment = splitPrefixedComment(value);
    if (comment) return comment;
  }
  return null;
}

/**
 * A tool name as a step label. MCP tools arrive as
 * `mcp__<server>__<tool_name>`; the server becomes the detail line.
 */
export function humanizeToolName(toolName: string): {
  label: string;
  server: string | null;
} {
  const mcp = /^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/.exec(toolName);
  const server = mcp ? mcp[1] : null;
  const name = mcp ? mcp[2] : toolName;
  const words = name
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .trim()
    .toLowerCase();
  const label = words
    ? words.charAt(0).toUpperCase() + words.slice(1)
    : toolName;
  return { label, server };
}

/** Up to `limit` items joined by " · ", for a grouped step's detail line. */
export function summarizeList(
  items: string[],
  limit = 3
): {
  shown: string[];
  hidden: number;
} {
  const nonEmpty = items.filter(Boolean);
  return {
    shown: nonEmpty.slice(0, limit),
    hidden: Math.max(0, nonEmpty.length - limit),
  };
}
