import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation } from '@tanstack/react-query';
import type { Decision } from 'shared/types';
import { Badge } from '@prepcook/ui/components/Badge';
import { ChatApprovalCard } from '@prepcook/ui/components/ChatApprovalCard';
import { useLavishSession } from '@/shared/hooks/useLavishSession';
import { postPlanDecision } from '@/features/kanban/model/planDecisionsApi';
import { useSessionCards } from '@/features/workspace/model/hooks/useSessionCards';
import { resolvePlanReady } from '@/features/workspace/model/sessionStatus';

/**
 * Closes the transcript with the plan gate while the session's card waits in
 * Plan with a recorded, unapproved RCA or spec: approve or request changes
 * on the card (docs/approvals.md), or open the plan's review page.
 * Investigations have no gate, so they get no card.
 */
export function PlanDecisionContainer({
  workspaceId,
}: {
  workspaceId: string;
}) {
  const { t } = useTranslation('common');
  const sessionCards = useSessionCards();
  const card = sessionCards.get(workspaceId);
  const planReady = resolvePlanReady(card?.metadata, card?.stage ?? null);
  const hasGate =
    Boolean(card && planReady) &&
    card?.metadata?.work_class !== 'investigation';
  const { data: lavishSession } = useLavishSession(
    hasGate ? workspaceId : undefined
  );
  const [note, setNote] = useState('');

  const decide = useMutation({
    mutationFn: ({
      issueId,
      decision,
      comment,
    }: {
      issueId: string;
      decision: Decision;
      comment: string;
    }) => postPlanDecision(issueId, decision, comment),
    onSuccess: () => setNote(''),
  });

  if (!card || !hasGate) return null;

  const verdict = card.metadata?.planning;
  const meta: ReactNode[] = [];
  if (verdict?.confidence) {
    meta.push(
      <Badge
        key="confidence"
        className={
          verdict.confidence === 'high'
            ? 'bg-success-soft text-success'
            : 'bg-attention-soft text-attention-foreground'
        }
      >
        {t('conversation.decision.confidence', { level: verdict.confidence })}
      </Badge>
    );
  }
  if (verdict?.open_questions != null) {
    meta.push(
      <Badge key="questions" variant="secondary">
        {t('conversation.decision.openQuestions', {
          count: verdict.open_questions,
        })}
      </Badge>
    );
  }

  const reviewUrl = lavishSession?.url;
  const submit = (decision: Decision) =>
    decide.mutate({ issueId: card.issueId, decision, comment: note });

  return (
    <div className="px-6">
      <ChatApprovalCard
        title={t('conversation.decision.planTitle')}
        content=""
        status={{ status: 'pending_approval' }}
        renderMarkdown={() => null}
        decision={{
          helper: t('conversation.decision.gateHelper'),
          meta: meta.length > 0 ? meta : undefined,
          note,
          onNoteChange: setNote,
          noteLabel: t('conversation.decision.noteLabel'),
          notePlaceholder: t('conversation.decision.gateNotePlaceholder'),
          approveLabel: t('conversation.decision.approve'),
          requestChangesLabel: t('conversation.decision.requestChanges'),
          onApprove: () => submit('approve'),
          onRequestChanges: () => submit('request_changes'),
          isSubmitting: decide.isPending,
          error: decide.error?.message ?? null,
          documentLink: reviewUrl
            ? {
                label:
                  planReady === 'rca'
                    ? t('conversation.decision.openRca')
                    : t('conversation.decision.openSpec'),
                onClick: () =>
                  window.open(reviewUrl, '_blank', 'noopener,noreferrer'),
              }
            : null,
        }}
      />
    </div>
  );
}
