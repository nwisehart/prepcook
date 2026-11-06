// Unit tests for the server-free parts of scripts/lib/pantry-board.mjs:
// `node --test scripts/lib/pantry-board.test.mjs`.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARDS,
  commentHeading,
  describeMissing,
  isComplete,
  misfiled,
  missingState,
  planGatePending,
  pullRequestState,
  seededMetadata,
} from "./pantry-board.mjs";

const COLUMNS = new Map(
  [
    ["backlog", "To Do"],
    ["planning", "Plan"],
    ["executing", "Execute"],
    ["reviewing", "Review"],
    ["pushed_mr", "Shipped"],
  ].map(([word, name]) => [word, { id: `status-${word}`, name }]),
);

function issue(role, column, extra = {}) {
  return {
    id: `issue-${role}`,
    title: CARDS[role].title,
    status_id: `status-${column}`,
    ...extra,
  };
}

test("misfiled names only seeded cards outside their seeded column", () => {
  const issues = [
    issue("backlog", "backlog"),
    // A film that stopped after approving the Plan bug.
    issue("planning", "executing"),
    issue("triaged", "planning"),
    issue("executing", "executing"),
    issue("reviewing", "reviewing"),
    issue("done", "pushed_mr"),
    // Not a seeded title: never refiled, wherever it is.
    {
      id: "issue-other",
      title: "Someone's own card",
      status_id: "status-reviewing",
    },
  ];
  const found = misfiled(issues, COLUMNS, Object.keys(CARDS));
  assert.deepEqual(
    found.map(({ role, issue, home, found }) => [
      role,
      issue.id,
      home.name,
      found?.name,
    ]),
    [["planning", "issue-planning", "Plan", "Execute"]],
  );
});

test("misfiled leaves a board as seeded, and missing cards, alone", () => {
  const issues = [issue("planning", "planning"), issue("done", "pushed_mr")];
  assert.deepEqual(misfiled(issues, COLUMNS, Object.keys(CARDS)), []);
});

test("misfiled looks only at the roles it is given", () => {
  const issues = [issue("planning", "executing"), issue("backlog", "planning")];
  assert.deepEqual(
    misfiled(issues, COLUMNS, ["planning", "done"]).map((m) => m.role),
    ["planning"],
  );
});

test("misfiled reports a column it does not know as found: null", () => {
  const issues = [issue("done", "cleanup")];
  const [entry] = misfiled(issues, COLUMNS, ["done"]);
  assert.equal(entry.home.name, "Shipped");
  assert.equal(entry.found, null);
});

test("misfiled never refiles a card whose seeded column is missing", () => {
  const noReview = new Map(COLUMNS);
  noReview.delete("reviewing");
  const issues = [issue("reviewing", "backlog")];
  assert.deepEqual(misfiled(issues, noReview, ["reviewing"]), []);
});

// What a card holds when the seed has written all of it.
function held(role) {
  const card = CARDS[role];
  return {
    metadata: { seeded_by: "x", ...seededMetadata(card, "2026-10-19T10:00Z") },
    comments: (card.comments ?? []).map((name, i) => ({
      id: `c${i}`,
      message: `${commentHeading(name)}\n\nbody`,
    })),
    pullRequests: card.pullRequest
      ? [{ number: card.pullRequest.number, status: "merged" }]
      : [],
  };
}

test("missingState finds nothing on a card as seeded", () => {
  for (const role of Object.keys(CARDS)) {
    const missing = missingState(CARDS[role], held(role));
    assert.ok(isComplete(missing), `${role}: ${JSON.stringify(missing)}`);
  }
});

test("missingState names a triage record lost from the investigation", () => {
  const state = held("triaged");
  delete state.metadata.triage;
  const missing = missingState(CARDS.triaged, state);
  assert.deepEqual(missing, {
    metadata: ["triage"],
    comments: [],
    pullRequest: false,
  });
  assert.equal(describeMissing(CARDS.triaged, missing), "metadata triage");
});

test("missingState counts a null key or another work class as missing", () => {
  const state = held("triaged");
  state.metadata.triage = null;
  state.metadata.work_class = "bug";
  assert.deepEqual(missingState(CARDS.triaged, state).metadata, [
    "work_class",
    "triage",
  ]);
});

test("missingState names the Shipped card's pull request when absent", () => {
  const state = held("done");
  state.pullRequests = [];
  const missing = missingState(CARDS.done, state);
  assert.equal(missing.pullRequest, true);
  assert.deepEqual(missing.metadata, []);
  assert.equal(
    describeMissing(CARDS.done, missing),
    "its merged pull request #1",
  );
});

test("missingState matches pull requests by number only", () => {
  const state = held("done");
  state.pullRequests = [{ number: 1, status: "open" }];
  assert.equal(missingState(CARDS.done, state).pullRequest, false);
  state.pullRequests = [{ number: 2, status: "merged" }];
  assert.equal(missingState(CARDS.done, state).pullRequest, true);
});

test("missingState matches comments by first line, not by body", () => {
  const state = held("planning");
  // The RCA's body was edited; its first line is what names it.
  state.comments[1].message =
    "RCA: sharing looks the email up case-sensitively\n\nedited";
  // Only the reproduction is gone.
  state.comments = [state.comments[1]];
  const missing = missingState(CARDS.planning, state);
  assert.deepEqual(missing.comments, ["pantry-reproduction.md"]);
  assert.equal(
    describeMissing(CARDS.planning, missing),
    'the "REPRODUCTION:" comment',
  );
});

test("missingState on a card the board kept nothing of", () => {
  const missing = missingState(CARDS.planning, {
    metadata: null,
    comments: [],
    pullRequests: [],
  });
  assert.deepEqual(missing, {
    metadata: ["work_class", "planning"],
    comments: ["pantry-reproduction.md", "pantry-rca.md"],
    pullRequest: false,
  });
});

test("planGatePending reads the plan gate as the board does", () => {
  const meta = held("planning").metadata;
  assert.equal(planGatePending(meta), true);
  assert.equal(planGatePending({ ...meta, planning: undefined }), false);
  assert.equal(
    planGatePending({ ...meta, spec: { approved_at: "2026-10-19T11:00Z" } }),
    false,
  );
  assert.equal(
    planGatePending({
      ...meta,
      approvals: [{ decision: "request_changes", at: "2026-10-19T11:00Z" }],
    }),
    false,
  );
  assert.equal(
    planGatePending({ ...meta, work_class: "investigation" }),
    false,
  );
});

test("pullRequestState follows the chip's precedence", () => {
  assert.equal(pullRequestState([]), null);
  assert.equal(pullRequestState([{ status: "merged" }]), "merged");
  assert.equal(
    pullRequestState([{ status: "merged" }, { status: "open" }]),
    "open",
  );
  assert.equal(pullRequestState([{ status: "closed" }]), "closed");
});
