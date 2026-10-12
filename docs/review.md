# The independent review

When a card enters Review, Prep Cook starts a new agent in a fresh workspace
branched from the implementation branch. It has not seen the plan, the RCA or
the session that wrote the fix. That is the point: the implementing agent knows
why every line is there, and a reviewer who knows the same reasons reads the
diff the way its author did. The reviewer starts from the `reviewing` playbook
([playbooks.md](playbooks.md#reviewing)).

## What it may read

- The diff against the target branch, freshly fetched, with lock files,
  minified files and generated types left out unless the ticket is about them.
- The rest of the codebase. Independence is not ignorance: the findings that
  matter most are outside the diff, in the helper that already existed or the
  pattern the branch forked. It reads the call sites of every symbol touched,
  the files beside the changed ones, and the `CLAUDE.md`, `AGENTS.md` and
  design docs that govern the area.
- Two kinds of comment on the card, fetched with `list_issue_comments`:
  - the **reproduction** (`REPRODUCTION:` when the planning agent wrote it
    under the current playbook): the harness's cases with expected and
    observed values. It is an acceptance test, not the author's reasoning, and
    is read for the same reason the tests are;
  - any **`REVIEW-REJECTED: <file>:<line>`** comment, where an earlier fix round
    argued that a finding does not hold. Each round starts with no memory of the
    last, and without these an answered finding comes back in new words every
    round.

## What it may not read

Anything written for this ticket's reasoning: its RCA or spec, its planning
notes, `docs/specs/`, any `docs/plans/` document the branch adds or edits, and
any earlier session. Nor earlier `REVIEW:` comments: they hold another
reviewer's conclusions, and reading them makes this round an echo instead of a
second look. Standing design docs that predate the branch are fair game.

## The reproduction drives the verdict

When a reproduction exists, the review answers two questions first. Does the
change lie on the path the reproduction exercises, or does it fix a
neighbouring path with the same symptom? A fix off the reproduced path is a
correctness blocker, however good it is otherwise. And was the reproduction
re-run against this change? One that was not is a correctness major: passing
unit tests do not stand in for the case the customer hit.

No reproduction on the card at all is the first finding, systematic, major.
Anything under `repro/` or `.repro/` in the diff is a systematic major too.

## Order of weight

Findings are judged in this order, most important first:

1. **Reuse and DRY.** For every new function, component, hook or util, an
   existing equivalent is searched for first. A working duplicate is still a
   finding: two places to fix the next bug. Repetition inside the branch counts.
2. **Pattern fit.** A fork of a shared component instead of an extension, a
   temporary workaround, a boolean that makes one thing behave like another, a
   bypass of the established data or state layer. When the pattern cannot
   accommodate the need, the finding is "this pattern needs updating", with a
   sketch.
3. **Readability.** Names in full words, functions small enough to hold in your
   head, no dead code. A comment that narrates the change, cites a ticket, or
   describes code the branch removed is major: the next reader has no diff.
4. **Correctness and tests.** A bug is reported only with a concrete failure
   scenario: inputs or state, then the wrong outcome. New behaviour needs tests;
   a bug fix without a regression test is incomplete.
5. **Sibling paths and swallowed failures.** For every guard the branch adds,
   the other paths to the same state, and whether each is covered. For every
   catch, default or fallback, whether a real failure now looks like success.
   For every field read off a payload, what happens when it is absent.
6. **Security and performance**, only where the diff creates the exposure:
   untrusted input reaching a query, shell, path or template; a secret in a log;
   a skipped authorization check; a query in a loop or an unbounded fetch, with
   the scale at which it hurts.

Every finding is verified before it is reported. "Use helper X" requires reading
X; a bug claim requires its failure path checked against the real code; a
convention claim cites the file that states it.

## Classes and severities

| Class | Meaning |
| --- | --- |
| `systematic` | A rule the codebase already answers: a duplicate helper, pattern divergence, a broken cited convention, a narrating comment, dead code, a missing test. |
| `correctness` | A real bug with its failure scenario, a security or data-loss risk, or a performance regression with its scale. |
| `product` | A question only a person can answer: intended behaviour, an acceptable trade-off, scope. Asked, never asserted. |

Severity is `blocker`, `major`, `minor` or `nit`.

## When REQUEST CHANGES is earned

The verdict is `REQUEST CHANGES` only when at least one systematic or
correctness finding is a blocker or major. Everything else is `APPROVE` with the
findings listed. The reviewer calibrates toward shipping: a change that clearly
improves the codebase is approved even when imperfect, and one that makes it
worse with a promise to fix it later is not. A naming complaint never sends work
back, and a product question cannot: the fix agent has no way to answer it. A
misleading comment is the one readability finding that earns a round.

The reviewer records the outcome twice: the `record_review` tool call with the
verdict and every finding (class, severity, path, line, title, body), which is
what Prep Cook routes on; then one `REVIEW: APPROVE` or `REVIEW: REQUEST CHANGES`
comment for people. When the two disagree, the recorded severities win.

## Fix rounds and the cap

A routed `REQUEST CHANGES` starts a fix round, in either mode:

1. The blocker and major systematic and correctness findings, and only those,
   go to the implementation agent as a follow-up turn in its own worktree.
   Nits and product questions stay behind.
2. The agent fixes each finding or rejects it with a
   `REVIEW-REJECTED: <file>:<line>` comment saying why. A round that changes
   nothing is a legitimate outcome.
3. When the implementation workspace goes idle, the card's claim is cleared and
   a fresh reviewer starts, with no memory of the last round.

After **Review-fix rounds before escalating** rounds (`autonomy_review_iterations`,
default 3, settable per board) a reviewer that still requests changes stops the
loop: the card waits in Review for a person, with a comment saying so (and a
Slack post in Prep mode).

An approved review moves the card to Push MR in Prep mode; in Chef mode a person
moves it. At Push MR the product questions are posted on the pull request, where
the person merging answers them. Blocker and major findings are posted there
only when the loop gave up on them at the cap.

## The pre-merge verify

The reviewer reads the change; the verify runs it. A verify is a fresh
stand-up of the app from `.prepcook/scripts.toml` in the implementation worktree
(`setup`, `build`, `start`, the ready check, `seed`), then the manifest's
`test`, then teardown ([scripts.md](scripts.md)).

- In Prep mode it is the gate between Execute and Review. A failure on the
  branch's side is handed back to the implementing agent with the failing step's
  output, up to four times. A failure on the setup's side (a missing secret, no
  free port) holds the card, is escalated, and is retried after 15 minutes.
- In either mode, a comment on the card whose first line is `VERIFY:` runs the
  same check on demand, in Execute, Review or Push MR, and posts the result on
  the card and on the pull request.

A repository without `.prepcook/scripts.toml` has no verify.
