import type { IssueExtMetadata, OriginKind, WorkClass } from 'shared/types';

/** Tag order: when a card carries several class tags, the first here wins. */
export const WORK_CLASSES: readonly WorkClass[] = [
  'bug',
  'investigation',
  'story',
  'epic',
  'subtask',
];

/** Label colours per class, matching the class tags the board seeds. */
export const WORK_CLASS_STYLES: Record<WorkClass, string> = {
  bug: 'bg-red-500/15 text-red-500',
  investigation: 'bg-cyan-500/15 text-cyan-600',
  story: 'bg-green-500/15 text-green-600',
  epic: 'bg-purple-500/15 text-purple-500',
  subtask: 'bg-blue-500/15 text-blue-500',
};

/** The class a card's tags name, or null when none does. */
export function workClassFromTags(tagNames: string[]): WorkClass | null {
  const normalized = new Set(tagNames.map((name) => name.trim().toLowerCase()));
  return WORK_CLASSES.find((workClass) => normalized.has(workClass)) ?? null;
}

/**
 * A card's class: its recorded `work_class`, else its class tag, else null
 * (unlike playbook resolution, which defaults to story).
 */
export function cardWorkClass(
  metadata: IssueExtMetadata | null,
  tagNames: string[]
): WorkClass | null {
  const recorded = metadata?.work_class;
  if (recorded && WORK_CLASSES.includes(recorded)) {
    return recorded;
  }
  return workClassFromTags(tagNames);
}

/** Who filed a card through the card API, when it was filed that way. */
export function cardOrigin(
  metadata: IssueExtMetadata | null
): { kind: OriginKind; ref: string; url: string | null } | null {
  const origin = metadata?.origin;
  if (!origin?.ref) return null;
  const ref = origin.ref.trim();
  const url = /^https?:\/\//i.test(ref) ? ref : null;
  return { kind: origin.kind, ref, url };
}
