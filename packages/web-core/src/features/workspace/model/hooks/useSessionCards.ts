import { useEffect, useMemo, useState } from 'react';
import {
  PROJECT_ISSUES_SHAPE,
  PROJECT_PROJECT_STATUSES_SHAPE,
  type Issue,
  type ProjectStatus,
} from 'shared/remote-types';
import type { IssueExtMetadata } from 'shared/types';
import { createShapeCollection } from '@/shared/lib/electric/collections';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useUserContext } from '@/shared/hooks/useUserContext';
import { parseIssueExtMetadata } from '@/shared/lib/stagePlaybooks';
import { resolveSessionStage, type SessionStage } from '../sessionStatus';

/** The card a session works on, as the Sessions view shows it. */
export interface SessionCard {
  issueId: string;
  projectId: string;
  /** The card key, e.g. "PAN-49". */
  key: string;
  title: string;
  /** The column name as the board spells it. */
  stageName: string | null;
  stage: SessionStage | null;
  metadata: IssueExtMetadata | null;
}

const EMPTY_CARDS = new Map<string, SessionCard>();

/**
 * Resolves each local session (workspace) to the card it was launched for:
 * key, title, column and planning metadata. Sessions that are not linked to
 * a card are absent from the map.
 *
 * Subscribes to the issues and statuses of every project that has a linked
 * session through the raw collection API, so the number of projects can
 * change without breaking the rules of hooks. Collections are cached by
 * createShapeCollection, so the board's own subscriptions are shared.
 */
export function useSessionCards(
  options: { enabled?: boolean } = {}
): Map<string, SessionCard> {
  const { enabled = true } = options;
  const { workspaces: remoteWorkspaces } = useUserContext();
  const { hasBoardAccess } = useAuth();

  const links = useMemo(() => {
    const result = new Map<string, { projectId: string; issueId: string }>();
    for (const remote of remoteWorkspaces) {
      if (!remote.local_workspace_id || !remote.issue_id) continue;
      result.set(remote.local_workspace_id, {
        projectId: remote.project_id,
        issueId: remote.issue_id,
      });
    }
    return result;
  }, [remoteWorkspaces]);

  const projectIdsKey = useMemo(
    () =>
      Array.from(new Set(Array.from(links.values(), (l) => l.projectId)))
        .sort()
        .join(','),
    [links]
  );

  const [issuesByProject, setIssuesByProject] = useState<
    Record<string, Issue[]>
  >({});
  const [statusesByProject, setStatusesByProject] = useState<
    Record<string, ProjectStatus[]>
  >({});

  useEffect(() => {
    if (!enabled || !hasBoardAccess || !projectIdsKey) return;

    const subscriptions: { unsubscribe: () => void }[] = [];
    for (const projectId of projectIdsKey.split(',')) {
      const issues = createShapeCollection(PROJECT_ISSUES_SHAPE, {
        project_id: projectId,
      });
      subscriptions.push(
        issues.subscribeChanges(
          () =>
            setIssuesByProject((prev) => ({
              ...prev,
              [projectId]: issues.toArray as unknown as Issue[],
            })),
          { includeInitialState: true }
        )
      );

      const statuses = createShapeCollection(PROJECT_PROJECT_STATUSES_SHAPE, {
        project_id: projectId,
      });
      subscriptions.push(
        statuses.subscribeChanges(
          () =>
            setStatusesByProject((prev) => ({
              ...prev,
              [projectId]: statuses.toArray as unknown as ProjectStatus[],
            })),
          { includeInitialState: true }
        )
      );
    }

    return () => {
      subscriptions.forEach((subscription) => subscription.unsubscribe());
    };
  }, [enabled, hasBoardAccess, projectIdsKey]);

  return useMemo(() => {
    if (!enabled || links.size === 0) return EMPTY_CARDS;

    const cards = new Map<string, SessionCard>();
    for (const [workspaceId, { projectId, issueId }] of links) {
      const issue = issuesByProject[projectId]?.find((i) => i.id === issueId);
      if (!issue) continue;
      const stageName =
        statusesByProject[projectId]?.find((s) => s.id === issue.status_id)
          ?.name ?? null;
      cards.set(workspaceId, {
        issueId,
        projectId,
        key: issue.simple_id,
        title: issue.title,
        stageName,
        stage: resolveSessionStage(stageName),
        metadata: parseIssueExtMetadata(issue.extension_metadata),
      });
    }
    return cards;
  }, [enabled, links, issuesByProject, statusesByProject]);
}
