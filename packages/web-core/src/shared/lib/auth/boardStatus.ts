import type { BoardStatus } from 'shared/types';

/**
 * Whether the server holds a board URL and a board secret. A board that is
 * not answering still counts: the degraded banner covers the outage.
 */
export function hasBoardAccess(
  status: Pick<BoardStatus, 'status'> | null | undefined
): boolean {
  return status?.status === 'connected' || status?.status === 'not_answering';
}
