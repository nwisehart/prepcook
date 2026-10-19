import { describe, it, expect } from 'vitest';
import type { IssueExtMetadata } from 'shared/types';
import {
  cardCleanupGate,
  columnDotTone,
  implementationSession,
  isTriaged,
  isWithinLastWeek,
  latestMergedPullRequest,
  needsYouReason,
  pendingPlanGate,
  priorityChipLabel,
  shippedIndicator,
  withDiscardConfirmed,
  workflowStageKey,
} from './cardSignals';
import {
  isTransitionAllowed,
  planningGateError,
} from '@/shared/lib/workflowTransitions';

const rcaVerdict = (
  overrides: Partial<NonNullable<IssueExtMetadata['planning']>> = {}
): IssueExtMetadata => ({
  work_class: 'bug',
  planning: {
    kind: 'rca',
    confidence: 'high',
    open_questions: 0,
    recorded_at: '2026-10-18T10:00:00Z',
    ...overrides,
  },
});

const pr = (
  overrides: Partial<{
    number: number;
    status: string;
    merged_at: string | null;
    created_at: string;
  }> = {}
) => ({
  number: 1,
  url: 'https://example.com/pr/1',
  status: 'open',
  merged_at: null,
  created_at: '2026-10-18T09:00:00Z',
  ...overrides,
});

describe('workflowStageKey', () => {
  it('matches every alias case-insensitively', () => {
    expect(workflowStageKey('Plan')).toBe('plan');
    expect(workflowStageKey('planning')).toBe('plan');
    expect(workflowStageKey('Shipped')).toBe('shipped');
    expect(workflowStageKey('Done')).toBe('shipped');
    expect(workflowStageKey(' Pushed MR ')).toBe('shipped');
    expect(workflowStageKey('Push MR')).toBe('shipped');
    expect(workflowStageKey('Deployed')).toBe('close');
    expect(workflowStageKey('Clean-up')).toBe('cleanUp');
  });

  it('leaves custom columns outside the workflow', () => {
    expect(workflowStageKey('Triaged')).toBeNull();
    expect(workflowStageKey('')).toBeNull();
    expect(workflowStageKey(null)).toBeNull();
  });
});

describe('columnDotTone', () => {
  it('follows the stage', () => {
    expect(columnDotTone('todo')).toBe('hollow');
    expect(columnDotTone('review')).toBe('brand');
    expect(columnDotTone('shipped')).toBe('success');
    expect(columnDotTone('close')).toBe('success');
    expect(columnDotTone('cleanUp')).toBe('muted');
  });
});

describe('pendingPlanGate', () => {
  it('is pending once the verdict is recorded and nobody approved', () => {
    expect(pendingPlanGate(rcaVerdict())).toEqual({
      planKind: 'rca',
      confidence: 'high',
      openQuestions: 0,
    });
  });

  it('reads a spec as the plan kind', () => {
    const meta: IssueExtMetadata = {
      work_class: 'story',
      planning: { kind: 'spec', open_questions: 2 },
    };
    expect(pendingPlanGate(meta)?.planKind).toBe('spec');
  });

  it('is not pending without a verdict, once approved, or for investigations', () => {
    expect(pendingPlanGate({ work_class: 'bug' })).toBeNull();
    expect(
      pendingPlanGate({
        ...rcaVerdict(),
        spec: { approved_at: '2026-10-18T11:00:00Z' },
      })
    ).toBeNull();
    expect(
      pendingPlanGate({ ...rcaVerdict(), work_class: 'investigation' })
    ).toBeNull();
  });

  it('is not pending while a person has taken the card over', () => {
    expect(
      pendingPlanGate({
        ...rcaVerdict(),
        human_takeover: { detected_at: '2026-10-18T11:00:00Z' },
      })
    ).toBeNull();
  });

  it('waits on the agent after request changes, until a newer verdict', () => {
    const sentBack: IssueExtMetadata = {
      ...rcaVerdict(),
      approvals: [
        {
          decision: 'request_changes',
          by: 'Ada',
          source: 'api',
          comment: 'Show the refresh failing',
          at: '2026-10-18T11:00:00Z',
        },
      ],
    };
    expect(pendingPlanGate(sentBack)).toBeNull();
    expect(
      pendingPlanGate({
        ...sentBack,
        planning: {
          ...sentBack.planning,
          recorded_at: '2026-10-18T12:00:00Z',
        },
      })
    ).not.toBeNull();
  });
});

describe('needsYouReason', () => {
  it('asks for a decision in Plan only', () => {
    expect(needsYouReason('plan', rcaVerdict(), [])?.kind).toBe('plan');
    expect(needsYouReason('execute', rcaVerdict(), [])).toBeNull();
  });

  it('asks to clean up a merged card in Shipped (and an older Close)', () => {
    const merged = pr({
      number: 208,
      status: 'merged',
      merged_at: '2026-10-18T10:00:00Z',
    });
    for (const stage of ['shipped', 'close'] as const) {
      const reason = needsYouReason(stage, null, [merged]);
      expect(reason?.kind).toBe('merged');
      expect(reason?.kind === 'merged' && reason.pullRequest.number).toBe(208);
    }
  });

  it('leaves an open pull request to the agent and its reviewers', () => {
    const merged = pr({ number: 2, status: 'merged' });
    expect(needsYouReason('shipped', null, [pr()])).toBeNull();
    expect(needsYouReason('shipped', null, [pr(), merged])).toBeNull();
    expect(needsYouReason('shipped', null, [])).toBeNull();
  });

  it('asks about a pull request closed without merging', () => {
    const reason = needsYouReason('shipped', null, [
      pr({ number: 9, status: 'closed' }),
    ]);
    expect(reason?.kind).toBe('closedUnmerged');
  });

  it('asks about a triaged investigation in Plan or a Triaged column', () => {
    const triaged: IssueExtMetadata = {
      work_class: 'investigation',
      triage: {
        report: {
          what_happens: '',
          where: '',
          why: '',
          scope: '',
          workaround: '',
          confidence: '',
          markdown: 'TRIAGE REPORT',
        },
        recorded_at: '2026-10-18T10:00:00Z',
      },
    };
    expect(isTriaged(triaged)).toBe(true);
    expect(needsYouReason('plan', triaged, [])?.kind).toBe('triaged');
    expect(needsYouReason(null, triaged, [])?.kind).toBe('triaged');
    expect(needsYouReason('cleanUp', triaged, [])).toBeNull();
    // Still investigating: nothing to decide yet.
    expect(
      needsYouReason('plan', { work_class: 'investigation' }, [])
    ).toBeNull();
    // Converted: a bug with its triage on record is planned as a bug.
    expect(isTriaged({ ...triaged, work_class: 'bug' })).toBe(false);
  });
});

describe('shippedIndicator', () => {
  it('reads every pull-request state', () => {
    expect(shippedIndicator(null, [])).toBeNull();
    expect(shippedIndicator(null, [pr()])).toEqual({ kind: 'open' });
    expect(
      shippedIndicator({ autonomy: { pipeline_handled_sha: 'abc123' } }, [pr()])
    ).toEqual({ kind: 'ciFailing' });
    expect(
      shippedIndicator(null, [
        pr({ status: 'merged' }),
        pr({ status: 'closed' }),
      ])
    ).toEqual({ kind: 'merged' });
    expect(shippedIndicator(null, [pr({ status: 'closed' })])).toEqual({
      kind: 'closed',
    });
    // An open request holds the card whatever else merged.
    expect(
      shippedIndicator(null, [pr({ status: 'merged' }), pr({ number: 2 })])
    ).toEqual({ kind: 'open' });
  });
});

describe('Clean up gate', () => {
  it('takes merged and pull-request-less cards', () => {
    expect(cardCleanupGate(null, [pr({ status: 'merged' })]).kind).toBe(
      'allowed'
    );
    expect(cardCleanupGate(null, []).kind).toBe('allowed');
  });

  it('refuses an open pull request', () => {
    const gate = cardCleanupGate(
      { discard_confirmed_at: '2026-10-18T10:00:00Z' },
      [pr()]
    );
    expect(gate.kind).toBe('refused');
  });

  it('needs the discard confirmed for a closed, unmerged pull request', () => {
    const closed = [pr({ status: 'closed' })];
    expect(cardCleanupGate(null, closed).kind).toBe('confirm');
    const confirmed = withDiscardConfirmed(
      { work_class: 'bug', foreign: true },
      new Date('2026-10-18T10:00:00Z')
    );
    expect(confirmed).toEqual({
      work_class: 'bug',
      foreign: true,
      discard_confirmed_at: '2026-10-18T10:00:00.000Z',
    });
    expect(cardCleanupGate(confirmed as IssueExtMetadata, closed).kind).toBe(
      'allowed'
    );
  });
});

describe('workflow transitions', () => {
  it('moves a current board through Shipped', () => {
    expect(isTransitionAllowed('Review', 'Shipped')).toBe(true);
    expect(isTransitionAllowed('Execute', 'Shipped')).toBe(false);
    expect(isTransitionAllowed('Shipped', 'Clean up')).toBe(true);
    // Clean up is reachable from every stage; its gate decides.
    expect(isTransitionAllowed('Plan', 'Clean up')).toBe(true);
  });

  it('keeps an older board working', () => {
    expect(isTransitionAllowed('Review', 'Push MR')).toBe(true);
    expect(isTransitionAllowed('Push MR', 'Close')).toBe(true);
    expect(isTransitionAllowed('Close', 'Clean up')).toBe(true);
    // Done is Shipped on a board made while the column was called that.
    expect(isTransitionAllowed('Review', 'Done')).toBe(true);
    expect(isTransitionAllowed('Done', 'Clean up')).toBe(true);
  });

  it('never lets an investigation into Execute', () => {
    const investigation = { work_class: 'investigation' };
    for (const from of ['Plan', 'Triaged', 'Planning']) {
      expect(planningGateError(from, 'Execute', investigation)).toMatch(
        /convert to a bug/
      );
    }
    expect(planningGateError('Plan', 'Execute', { work_class: 'bug' })).toBe(
      null
    );
  });
});

describe('latestMergedPullRequest', () => {
  it('picks the newest merge', () => {
    const first = pr({
      number: 1,
      status: 'merged',
      merged_at: '2026-10-01T00:00:00Z',
    });
    const second = pr({
      number: 2,
      status: 'merged',
      merged_at: '2026-10-02T00:00:00Z',
    });
    expect(latestMergedPullRequest([second, first])?.number).toBe(2);
  });
});

describe('implementationSession', () => {
  it('skips review sessions and sessions not on this machine', () => {
    const sessions = [
      {
        name: 'Review: PAN-49',
        localWorkspaceId: 'r',
        updatedAt: '2026-10-18T12:00:00Z',
      },
      {
        name: 'PAN-49',
        localWorkspaceId: 'a',
        updatedAt: '2026-10-18T10:00:00Z',
      },
      {
        name: 'PAN-49 retry',
        localWorkspaceId: 'remote',
        updatedAt: '2026-10-18T11:00:00Z',
      },
    ];
    expect(
      implementationSession(sessions, (id) => id !== 'remote')?.localWorkspaceId
    ).toBe('a');
  });
});

describe('priorityChipLabel and isWithinLastWeek', () => {
  it('maps priorities to P0–P3', () => {
    expect(priorityChipLabel('urgent')).toBe('P0');
    expect(priorityChipLabel('low')).toBe('P3');
    expect(priorityChipLabel(null)).toBeNull();
  });

  it('splits a week back from now', () => {
    const now = Date.parse('2026-10-18T12:00:00Z');
    expect(isWithinLastWeek('2026-10-12T12:00:01Z', now)).toBe(true);
    expect(isWithinLastWeek('2026-10-11T12:00:00Z', now)).toBe(false);
  });
});
