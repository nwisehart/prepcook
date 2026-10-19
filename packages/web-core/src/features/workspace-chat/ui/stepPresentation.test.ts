import { describe, it, expect } from 'vitest';
import {
  fileBaseName,
  findPrefixedCommentInArgs,
  humanizeToolName,
  splitPrefixedComment,
  summarizeList,
} from './stepPresentation';

describe('splitPrefixedComment', () => {
  it('splits the workflow prefixes', () => {
    expect(splitPrefixedComment('REPRODUCTION: stores 0')).toEqual({
      prefix: 'REPRODUCTION:',
      body: 'stores 0',
    });
    expect(splitPrefixedComment('  gate: moved to Execute')).toEqual({
      prefix: 'gate:',
      body: 'moved to Execute',
    });
  });

  it('reads REVIEW-REJECTED: as its own prefix', () => {
    expect(splitPrefixedComment('REVIEW-REJECTED: a.ts:4 holds')?.prefix).toBe(
      'REVIEW-REJECTED:'
    );
  });

  it('ignores ordinary prose', () => {
    expect(splitPrefixedComment('The review found nothing.')).toBeNull();
    expect(splitPrefixedComment(null)).toBeNull();
  });
});

describe('findPrefixedCommentInArgs', () => {
  it('finds the comment among string arguments', () => {
    expect(
      findPrefixedCommentInArgs({
        issue_id: 'abc',
        message: 'REVIEW: APPROVE',
      })
    ).toEqual({ prefix: 'REVIEW:', body: 'APPROVE' });
    expect(findPrefixedCommentInArgs(['REVIEW: APPROVE'])).toBeNull();
  });
});

describe('humanizeToolName', () => {
  it('splits MCP tool names into a label and server', () => {
    expect(humanizeToolName('mcp__prepcook__add_issue_comment')).toEqual({
      label: 'Add issue comment',
      server: 'prepcook',
    });
    expect(humanizeToolName('WebSearch')).toEqual({
      label: 'Web search',
      server: null,
    });
  });
});

describe('fileBaseName and summarizeList', () => {
  it('keeps the last path segment', () => {
    expect(fileBaseName('src/import/quantity.ts')).toBe('quantity.ts');
    expect(fileBaseName('README.md')).toBe('README.md');
  });

  it('caps the list and counts the rest', () => {
    expect(summarizeList(['a', 'b', 'c', 'd', ''], 3)).toEqual({
      shown: ['a', 'b', 'c'],
      hidden: 1,
    });
  });
});
