import type {
  ApprovalDecisionRequest,
  ApprovalRecord,
  Decision,
} from 'shared/types';
import { handleApiResponse } from '@/shared/lib/api';
import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';

/**
 * Records a plan decision on a card through `POST /api/approvals/{issue_id}`
 * (docs/approvals.md). The board's own pages call it without a token and the
 * server takes the request as the board owner's. An approval sets the spec
 * approval and moves the card to Execute once the plan passes the gate;
 * request changes clears any approval and sends the comment to the planning
 * agent. Rejects with the server's message (409 when the card has left
 * Plan, or is an investigation).
 */
export async function postPlanDecision(
  issueId: string,
  decision: Decision,
  comment?: string
): Promise<ApprovalRecord> {
  const body: ApprovalDecisionRequest = { decision };
  if (comment?.trim()) {
    body.comment = comment.trim();
  }
  const response = await makeLocalApiRequest(
    `/api/approvals/${encodeURIComponent(issueId)}`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }
  );
  return handleApiResponse<ApprovalRecord>(response);
}
