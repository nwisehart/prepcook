import { ConfirmDialog } from '@prepcook/ui/components/ConfirmDialog';
import type { KanbanTranslate } from '@prepcook/ui/components/KanbanRelativeTime';

/**
 * Asked before Clean up takes a card whose pull request closed without
 * merging: cleaning it up discards the branch and the worktree. Resolves
 * true when the person confirmed.
 */
export async function confirmDiscard(t: KanbanTranslate): Promise<boolean> {
  const result = await ConfirmDialog.show({
    title: t('kanban.cleanUpGate.discardTitle'),
    message: t('kanban.cleanUpGate.discardMessage'),
    confirmText: t('kanban.cleanUpGate.discardConfirm'),
    variant: 'destructive',
  });
  return result === 'confirmed';
}
