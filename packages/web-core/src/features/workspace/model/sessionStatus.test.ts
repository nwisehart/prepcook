import { describe, it, expect } from 'vitest';
import type { IssueExtMetadata } from 'shared/types';
import {
  deriveSessionStatus,
  elapsedSince,
  groupSession,
  resolvePlanReady,
  resolveSessionStage,
} from './sessionStatus';

describe('resolveSessionStage', () => {
  it('maps column names and legacy aliases case-insensitively', () => {
    expect(resolveSessionStage('Plan')).toBe('plan');
    expect(resolveSessionStage('executing')).toBe('execute');
    expect(resolveSessionStage(' Push MR ')).toBe('push');
    expect(resolveSessionStage('Deployed')).toBe('close');
    expect(resolveSessionStage('Clean-up')).toBe('cleanup');
  });

  it('returns null for custom or missing columns', () => {
    expect(resolveSessionStage('Backlog')).toBeNull();
    expect(resolveSessionStage(null)).toBeNull();
  });
});

describe('resolvePlanReady', () => {
  const verdict = (kind: string): IssueExtMetadata => ({
    planning: { kind, recorded_at: '2026-10-18T10:00:00Z' },
  });

  it('flags a recorded, unapproved verdict in Plan', () => {
    expect(resolvePlanReady(verdict('rca'), 'plan')).toBe('rca');
    expect(resolvePlanReady(verdict('spec'), 'plan')).toBe('spec');
  });

  it('ignores approved plans and other stages', () => {
    expect(
      resolvePlanReady(
        { ...verdict('rca'), spec: { approved_at: '2026-10-18T11:00:00Z' } },
        'plan'
      )
    ).toBeNull();
    expect(resolvePlanReady(verdict('rca'), 'execute')).toBeNull();
    expect(resolvePlanReady(null, 'plan')).toBeNull();
  });
});

describe('deriveSessionStatus', () => {
  it('puts a pending approval ahead of running', () => {
    const status = deriveSessionStatus({
      isRunning: true,
      hasPendingApproval: true,
    });
    expect(status).toEqual({
      kind: 'approval',
      tone: 'attention',
      needsYou: true,
    });
  });

  it('keeps running sessions out of Needs you even when unseen', () => {
    const status = deriveSessionStatus({
      isRunning: true,
      hasUnseenActivity: true,
    });
    expect(status.kind).toBe('running');
    expect(status.needsYou).toBe(false);
  });

  it('asks for the close once the pull request merged', () => {
    expect(
      deriveSessionStatus({ prStatus: 'merged', stage: 'close' }).kind
    ).toBe('merged-ready');
    expect(
      deriveSessionStatus({ prStatus: 'merged', stage: 'cleanup' }).kind
    ).toBe('merged');
  });

  it('reports a failed last run as an error', () => {
    expect(deriveSessionStatus({ latestProcessStatus: 'failed' }).tone).toBe(
      'error'
    );
  });
});

describe('groupSession', () => {
  const now = new Date('2026-10-18T15:00:00');

  it('splits idle sessions by local day', () => {
    const idle = deriveSessionStatus({});
    expect(groupSession(idle, '2026-10-18T09:00:00', now)).toBe('done-today');
    expect(groupSession(idle, '2026-10-17T23:00:00', now)).toBe('earlier');
    expect(groupSession(idle, undefined, now)).toBe('earlier');
  });
});

describe('elapsedSince', () => {
  const now = new Date('2026-10-18T15:00:00Z');

  it('rounds down and never reads below one minute', () => {
    expect(elapsedSince('2026-10-18T14:59:50Z', now)).toEqual({
      unit: 'minutes',
      count: 1,
    });
    expect(elapsedSince('2026-10-18T12:30:00Z', now)).toEqual({
      unit: 'hours',
      count: 2,
    });
    expect(elapsedSince('2026-10-15T15:00:00Z', now)).toEqual({
      unit: 'days',
      count: 3,
    });
    expect(elapsedSince('not a date', now)).toBeNull();
  });
});
