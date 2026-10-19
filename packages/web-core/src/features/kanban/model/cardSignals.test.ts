import { describe, it, expect } from 'vitest';
import type { IssueExtMetadata } from 'shared/types';
import {
  columnDotTone,
  implementationSession,
  isWithinLastWeek,
  latestMergedPullRequest,
  needsYouReason,
  pendingPlanGate,
  priorityChipLabel,
  workflowStageKey,
} from './cardSignals';

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
    expect(workflowStageKey(' Pushed MR ')).toBe('pushMr');
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

  it('asks to close a merged card in Close', () => {
    const merged = pr({
      number: 208,
      status: 'merged',
      merged_at: '2026-10-18T10:00:00Z',
    });
    const reason = needsYouReason('close', null, [pr(), merged]);
    expect(reason?.kind).toBe('close');
    expect(reason?.kind === 'close' && reason.pullRequest.number).toBe(208);
    expect(needsYouReason('close', null, [pr()])).toBeNull();
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
