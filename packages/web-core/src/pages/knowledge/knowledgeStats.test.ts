import { describe, it, expect } from 'vitest';
import type { Issue, ProjectStatus } from 'shared/remote-types';
import type { KbDocumentSummaryT, KbEdgeT, KbNodeT } from '@/shared/lib/api';
import {
  calendarDaysAgo,
  countUnlinked,
  draftDocumentIds,
  humanizeTagValue,
  isInDayRange,
  nodeStats,
  rankedNodes,
  sortByNewest,
  tagValueCounts,
  utcDay,
} from './knowledgeStats';

function doc(
  id: string,
  overrides: Partial<KbDocumentSummaryT> = {}
): KbDocumentSummaryT {
  return {
    id,
    project_id: 'p',
    kind: 'rca',
    title: `RCA ${id}`,
    issue_id: null,
    jira_key: null,
    work_class: 'bug',
    summary: null,
    tags: [],
    created_at: '2026-10-01T12:00:00Z',
    updated_at: '2026-10-01T12:00:00Z',
    ...overrides,
  };
}

function node(id: string, kind = 'root_cause_class'): KbNodeT {
  return {
    id,
    project_id: 'p',
    kind,
    name: `Node ${id}`,
    description: null,
    seeded: false,
    investigation_jira_key: null,
    investigation_edge_count: null,
    edge_count: 0,
    created_at: '2026-01-01T00:00:00Z',
  };
}

function edge(nodeId: string, documentId: string): KbEdgeT {
  return {
    id: `${nodeId}-${documentId}`,
    project_id: 'p',
    node_id: nodeId,
    document_id: documentId,
    rationale: null,
    confidence: null,
    created_at: '2026-10-01T12:00:00Z',
  };
}

function issue(
  id: string,
  statusId: string,
  extensionMetadata: Issue['extension_metadata'] = {}
): Issue {
  return {
    id,
    project_id: 'p',
    issue_number: 1,
    simple_id: 'PAN-1',
    status_id: statusId,
    title: 'Bug',
    description: null,
    priority: null,
    start_date: null,
    target_date: null,
    completed_at: null,
    sort_order: 0,
    parent_issue_id: null,
    parent_issue_sort_order: null,
    extension_metadata: extensionMetadata,
    creator_user_id: null,
    created_at: '2026-10-01T12:00:00Z',
    updated_at: '2026-10-01T12:00:00Z',
  };
}

function status(id: string, name: string): ProjectStatus {
  return {
    id,
    project_id: 'p',
    name,
    color: '0 0% 0%',
    sort_order: 0,
    hidden: false,
    created_at: '2026-01-01T00:00:00Z',
  };
}

describe('date range', () => {
  it('reads the UTC day of a microsecond timestamp', () => {
    expect(utcDay('2026-10-18T23:59:59.123456Z')).toBe('2026-10-18');
  });

  it('treats both bounds as inclusive whole days and empty as open', () => {
    const at = '2026-10-18T23:30:00Z';
    expect(isInDayRange(at, { since: '', until: '' })).toBe(true);
    expect(isInDayRange(at, { since: '2026-10-18', until: '' })).toBe(true);
    expect(isInDayRange(at, { since: '', until: '2026-10-18' })).toBe(true);
    expect(isInDayRange(at, { since: '2026-10-19', until: '' })).toBe(false);
    expect(isInDayRange(at, { since: '', until: '2026-10-17' })).toBe(false);
  });
});

describe('tagValueCounts', () => {
  const documents = [
    doc('a', { tags: ['origin:dormant', 'detectable-by:qa'] }),
    doc('b', {
      tags: ['origin:dormant', 'detectable-by:qa', 'detectable-by:code-review'],
    }),
    doc('c', { tags: ['origin:regression', 'severity:p1'] }),
    doc('d', { tags: ['severity:p0'] }),
  ];

  it('counts each value, most first', () => {
    expect(tagValueCounts(documents, 'origin')).toEqual([
      { value: 'dormant', count: 2 },
      { value: 'regression', count: 1 },
    ]);
  });

  it('counts every value a document carries in the namespace', () => {
    expect(tagValueCounts(documents, 'detectable-by')).toEqual([
      { value: 'qa', count: 2 },
      { value: 'code-review', count: 1 },
    ]);
  });

  it('can order by value instead', () => {
    expect(
      tagValueCounts(documents, 'severity', 'value').map((row) => row.value)
    ).toEqual(['p0', 'p1']);
  });
});

describe('nodeStats', () => {
  const documents = [
    doc('old', { created_at: '2026-10-01T12:00:00.5Z' }),
    doc('new', { created_at: '2026-10-01T12:00:01Z' }),
  ];
  const documentsById = new Map(documents.map((d) => [d.id, d]));

  it('counts in-range RCAs and finds the newest by time, not by string', () => {
    const stats = nodeStats(
      [node('n')],
      [edge('n', 'old'), edge('n', 'new'), edge('n', 'outside-range')],
      documentsById
    );
    expect(stats.get('n')?.count).toBe(2);
    expect(stats.get('n')?.latest?.id).toBe('new');
  });

  it('ranks one kind, dropping nodes with no RCAs', () => {
    const stats = nodeStats(
      [node('a'), node('b'), node('c'), node('area', 'product_area')],
      [
        edge('a', 'old'),
        edge('b', 'old'),
        edge('b', 'new'),
        edge('area', 'new'),
      ],
      documentsById
    );
    expect(
      rankedNodes(stats, 'root_cause_class').map((s) => s.node.id)
    ).toEqual(['b', 'a']);
  });
});

describe('countUnlinked', () => {
  it('counts documents with no link to a node of the kind', () => {
    const documents = [doc('a'), doc('b'), doc('c')];
    const nodes = [node('cls'), node('area', 'product_area')];
    const edges = [edge('cls', 'a'), edge('area', 'b')];
    expect(countUnlinked(documents, edges, nodes, 'root_cause_class')).toBe(2);
  });
});

describe('draftDocumentIds', () => {
  const statuses = [status('plan', 'Plan'), status('exec', 'Execute')];
  const issues = new Map(
    [
      issue('waiting', 'plan', { spec: { path: 'x' } }),
      issue('approved', 'plan', { spec: { approved_at: '2026-10-02' } }),
      issue('executing', 'exec', {}),
      issue('investigating', 'plan', {}),
    ].map((i) => [i.id, i])
  );

  it('marks unapproved RCAs whose card waits in Plan', () => {
    const documents = [
      doc('d1', { issue_id: 'waiting' }),
      doc('d2', { issue_id: 'approved' }),
      doc('d3', { issue_id: 'executing' }),
      doc('d4', { issue_id: 'investigating', work_class: 'investigation' }),
      doc('d5', { issue_id: null }),
      doc('d6', { issue_id: 'deleted-card' }),
    ];
    expect([...draftDocumentIds(documents, issues, statuses)]).toEqual(['d1']);
  });

  it('accepts the older Planning column name', () => {
    const planning = [status('plan', 'Planning')];
    expect(
      draftDocumentIds([doc('d1', { issue_id: 'waiting' })], issues, planning)
        .size
    ).toBe(1);
  });
});

describe('display helpers', () => {
  it('humanizes tag values', () => {
    expect(humanizeTagValue('p0')).toBe('P0');
    expect(humanizeTagValue('qa')).toBe('QA');
    expect(humanizeTagValue('code-review')).toBe('Code review');
    expect(humanizeTagValue('dormant')).toBe('Dormant');
  });

  it('counts calendar days, not 24-hour spans', () => {
    const now = new Date(2026, 9, 18, 0, 30);
    expect(
      calendarDaysAgo(new Date(2026, 9, 18, 0, 5).toISOString(), now)
    ).toBe(0);
    expect(
      calendarDaysAgo(new Date(2026, 9, 17, 23, 55).toISOString(), now)
    ).toBe(1);
    expect(calendarDaysAgo(new Date(2026, 9, 14, 9).toISOString(), now)).toBe(
      4
    );
  });

  it('sorts newest first', () => {
    const documents = [
      doc('a', { created_at: '2026-10-01T00:00:00Z' }),
      doc('b', { created_at: '2026-10-03T00:00:00Z' }),
      doc('c', { created_at: '2026-10-02T00:00:00Z' }),
    ];
    expect(sortByNewest(documents).map((d) => d.id)).toEqual(['b', 'c', 'a']);
  });
});
