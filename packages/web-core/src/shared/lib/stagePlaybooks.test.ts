import { describe, it, expect } from 'vitest';
import {
  DEFAULT_PLAYBOOKS,
  interpolatePlaybookTemplate,
  parseIssueExtMetadata,
  resolveStage,
  resolveStagePlaybook,
  resolveWorkClass,
} from './stagePlaybooks';

const playbooks = {
  'bug:planning': 'RCA for {{jira_key}}: {{title}}\n\n{{description}}',
  'story:planning': 'Spec for {{jira_key}} at {{spec_path}}',
  'bug:executing': 'Fix {{jira_key}} using {{spec_path}}',
  'story:executing': 'Execute {{spec_path}}',
};

function createIssueFixture(
  overrides: Partial<{
    simple_id: string;
    title: string;
    description: string | null;
    extension_metadata: unknown;
  }> = {}
) {
  return {
    simple_id: 'VK-7',
    title: 'Fix login crash',
    description: 'Crashes on empty password',
    extension_metadata: null,
    ...overrides,
  };
}

describe('parseIssueExtMetadata', () => {
  it('returns null for non-object values', () => {
    expect(parseIssueExtMetadata(null)).toBeNull();
    expect(parseIssueExtMetadata('x')).toBeNull();
    expect(parseIssueExtMetadata([1])).toBeNull();
  });

  it('passes objects through', () => {
    expect(parseIssueExtMetadata({ work_class: 'bug' })).toEqual({
      work_class: 'bug',
    });
  });
});

describe('resolveWorkClass', () => {
  it('prefers metadata work_class', () => {
    expect(resolveWorkClass({ work_class: 'bug' }, ['story'])).toBe('bug');
  });

  it('falls back to work-class tags, case-insensitively', () => {
    expect(resolveWorkClass(null, ['Backend', 'Bug'])).toBe('bug');
  });

  it('defaults to story', () => {
    expect(resolveWorkClass(null, ['backend'])).toBe('story');
    expect(resolveWorkClass({}, [])).toBe('story');
  });
});

describe('resolveStage', () => {
  it('matches Planning/Executing column names case-insensitively', () => {
    expect(resolveStage('Planning')).toBe('planning');
    expect(resolveStage(' executing ')).toBe('executing');
  });

  it('returns null for other columns', () => {
    expect(resolveStage('Reviewing')).toBeNull();
    expect(resolveStage(null)).toBeNull();
  });
});

describe('interpolatePlaybookTemplate', () => {
  it('replaces known placeholders and keeps unknown ones', () => {
    expect(
      interpolatePlaybookTemplate('{{jira_key}} / {{unknown}}', {
        jira_key: 'BUG-1',
      })
    ).toBe('BUG-1 / {{unknown}}');
  });

  it('fills dotted names and does not rescan inserted values', () => {
    expect(
      interpolatePlaybookTemplate('{{product.name}}: {{title}}', {
        'product.name': 'Pantry',
        title: 'uses {{ticket_key}}',
        ticket_key: 'KEY-1',
      })
    ).toBe('Pantry: uses {{ticket_key}}');
  });

  it('keeps an if block only when its value is not blank', () => {
    const template =
      'A.{{#if product.areas}} Areas: {{product.areas}}.{{/if}} ' +
      '{{#if product.evidence_tools}}{{product.evidence_tools}}' +
      '{{else}}No logs.{{/if}}';
    expect(
      interpolatePlaybookTemplate(template, {
        'product.areas': ' ',
        'product.evidence_tools': '',
      })
    ).toBe('A. No logs.');
    expect(
      interpolatePlaybookTemplate(template, {
        'product.areas': 'Lists, Items',
        'product.evidence_tools': 'See logs/app.log.',
      })
    ).toBe('A. Areas: Lists, Items. See logs/app.log.');
  });

  it('fills the gate instructions before inserting them', () => {
    expect(
      interpolatePlaybookTemplate('Gate: {{gate_instructions}}', {
        gate_instructions: 'review {{spec_path}}',
        spec_path: 'docs/specs/KEY-1.html',
      })
    ).toBe('Gate: review docs/specs/KEY-1.html');
  });
});

describe('DEFAULT_PLAYBOOKS', () => {
  it('has every stage and uses the gate in every planning playbook', () => {
    expect(Object.keys(DEFAULT_PLAYBOOKS).sort()).toEqual([
      'bug:executing',
      'bug:planning',
      'epic:executing',
      'epic:planning',
      'investigation:planning',
      'rca_analysis',
      'reviewing',
      'story:executing',
      'story:planning',
      'subtask:executing',
      'subtask:planning',
      'triage',
    ]);
    for (const [key, text] of Object.entries(DEFAULT_PLAYBOOKS)) {
      if (key.endsWith(':planning')) {
        expect(text, key).toContain('{{gate_instructions}}');
      }
      expect(text, key).not.toMatch(/lavish|jira|master/i);
    }
  });
});

describe('resolveStagePlaybook', () => {
  it('resolves a bug planning playbook from metadata', () => {
    const result = resolveStagePlaybook({
      issue: createIssueFixture({
        extension_metadata: { work_class: 'bug', jira: { key: 'BUG-1234' } },
      }),
      tagNames: [],
      statusName: 'Planning',
      playbooks,
    });

    expect(result).toEqual({
      stage: 'planning',
      workClass: 'bug',
      prompt: 'RCA for BUG-1234: Fix login crash\n\nCrashes on empty password',
    });
  });

  it('fills the board context alongside the issue', () => {
    const result = resolveStagePlaybook({
      issue: createIssueFixture({ extension_metadata: { work_class: 'bug' } }),
      tagNames: [],
      statusName: 'Executing',
      playbooks: {
        'bug:executing':
          '{{ticket_key}} onto {{target_branch}} as a {{pr_noun}}' +
          '{{#if product.areas}} in {{product.areas}}{{/if}}',
      },
      context: {
        target_branch: 'origin/main',
        pr_noun: 'merge request',
        'product.areas': '',
        ticket_key: 'ignored: the issue wins',
      },
    });

    expect(result?.prompt).toBe('VK-7 onto origin/main as a merge request');
  });

  it('falls back to simple_id and default spec path', () => {
    const result = resolveStagePlaybook({
      issue: createIssueFixture({ description: null }),
      tagNames: [],
      statusName: 'Executing',
      playbooks,
    });

    expect(result?.prompt).toBe('Execute docs/specs/VK-7.html');
  });

  it('returns null outside playbook stages or without a template', () => {
    const issue = createIssueFixture();
    expect(
      resolveStagePlaybook({
        issue,
        tagNames: [],
        statusName: 'Unassigned',
        playbooks,
      })
    ).toBeNull();
    expect(
      resolveStagePlaybook({
        issue,
        tagNames: ['epic'],
        statusName: 'Planning',
        playbooks,
      })
    ).toBeNull();
  });
});
