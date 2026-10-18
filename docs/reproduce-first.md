# Reproduce first

A bug card's Plan stage does two things in a fixed order: it makes the failure
happen, and only then explains it. The planning agent is told not to read code
for the cause until it has a script that fails for the reported reason.

## Why the harness comes before the explanation

An RCA that cannot make the failure happen is a guess. Code that looks wrong is
everywhere; the question is whether it is the code the reporter's path runs
through. A harness that fails on the target branch answers that, and one that
passes against a candidate change confirms the mechanism rather than the
reading of it.

The harness is also the acceptance test for everything after Plan:

- The fix is done when the harness passes. The executing agent is told to run
  it first, so it has watched the failure it is about to fix, and again when it
  believes it is finished.
- The reviewer reads the `REPRODUCTION:` comment before it judges the branch
  ([review.md](review.md)). A fix that lies off the reproduced path is a
  blocker; a reproduction nobody re-ran against the change is a major finding.
- The pull request carries the harness as its first comment, so a human
  reviewer can run it too.

## What the harness must assert

The harness lives in `.repro/<KEY>/` in the card's worktree. `.repro/` goes into
`.git/info/exclude` before anything is written there: it is never committed, and
a branch that contains it is refused at the push gate.

It is a Playwright script built with the `repro-harness` skill, run against the
app Prep Cook stood up for the worktree (`$PREPCOOK_APP_URL`, with the seed's login in
the environment; see [scripts.md](scripts.md)). It must:

- **Assert the reported behaviour.** It fails against the target branch, or the
  ticket is not reproduced. A script that only shows the page is not a harness.
- **Print one JSON line per case** on stdout: case, expected, observed, pass.
- **Exit non-zero** when any case fails.
- **Screenshot each case** to `.repro/<KEY>/case-<name>.png`, and write no log
  files.
- **Keep the login out of everything**: read from the environment, never
  printed, logged, uploaded or committed.

The reproduction is posted as soon as it exists: a `REPRODUCTION:` comment on
the card with the JSON lines, the mechanism in one paragraph, the commit the
app was built from and the UTC time; and, when a tracker is configured, the
same evidence in plain language on the ticket.

## The harness and the logs

The harness shows the failure on this worktree's build; where the app runs for
real there is often more. With the failure in hand, the planning agent is told
about every evidence source its board has — an error tracker, a log service,
the log files the app writes in the worktree — and to call `evidence_search`
with a pattern from the report or the harness before guessing at a cause
([evidence.md](evidence.md)). What a source shows goes into the RCA's Root
cause with the source and the time, and so does a source that shows nothing.
Logs corroborate the mechanism the harness exercises; they do not replace the
harness, and when none of them show the failure, the harness is the evidence.

## Intermittent bugs are a rate

When the report says "sometimes", "every few days", or names a time window
rather than an action, a pass/fail harness measures nothing. Three passes in a
row is not evidence the bug is gone. The harness measures a rate instead:

- N trials against the suspected path and N against a healthy control in the
  same session.
- N, and the separation that counts as reproduced, fixed before the first run.
- One JSON line per trial, with its arm; the rate is computed from the lines.
- Exit non-zero when the subject's rate clears the threshold.
- What could not be held still (time of day, load, whatever else was running)
  is stated with the rate.

If the two rates separate, that is the reproduction. If they do not, that is a
result of its own, and the RCA reports it as one.

## When it does not fail

A harness that will not fail means one of four things, and the RCA says which:

1. **The failure is not on the target branch's path.** The RCA records what was
   checked and against what, with confidence low, and names the data shape or
   state production has that the seed lacks.
2. **The target branch already carries a fix.** The agent finds the commit,
   cites it, and routes the ticket as fixed but not yet released.
3. **The bug needs a customer object or credential the seed cannot have.** The
   RCA names it precisely, and the question goes to the reporter in the same
   comment that reports the attempt. The agent does not wait for the answer.
4. **The failure is intermittent.** It is measured as a rate (above). Rates
   that do not separate are reported with the trials, the period and the
   control, confidence low. That is never rounded to "not on the path": the two
   are different claims.

Separately, whenever the failing path runs through an external system (a
third-party service, a customer's SSO, any external API), the agent names it in
the verdict's integration field whatever it concludes. A seeded app cannot hold
the customer's credentials, and the person approving the RCA is told so.

An RCA below high confidence cannot leave Plan. It lists the next steps that
would raise confidence, the most valuable question first.

## Reproduce it on your own machine

Every RCA ends with a **Reproduce locally** section: which seeded user and
records the harness relied on (by name, never by id or credential), anything it
had to create that the seed lacks, and a kickoff prompt for a local Claude Code
session with the `prepcook-repro` skill, in a block to copy whole.

That works because the app comes from the repository, not from Prep Cook. Whoever
picks the ticket up runs the same `.prepcook/scripts.toml`: its `setup`, `build`,
`start` and `seed`, with the same variables ([scripts.md](scripts.md#environment)).
The seed gives them the same kind of records and the same login variables the
agent had. Pasting the kickoff into a session started from that shell runs
`/prepcook-repro`, which builds or re-runs the harness against their app and stops
before diagnosing. The [toolkit](../toolkit/README.md) has the rest of the loop
(`/prepcook-rca`, `/prepcook-fix`, `/prepcook-review`, `/prepcook-mr`), generated from the same
playbooks.

A board can reproduce against a remote target instead: a preview environment,
a dev tenant or a per-branch staging deploy, stood up by the repository's
`[app.remote]` scripts and torn down after ([scripts.md](scripts.md#remote-targets)).
The agent, the harness and the verification see the same `$PREPCOOK_APP_URL` and
seeded login either way, so the RCA's kickoff works against whichever the person
picking the ticket up has: the app on their own machine, or the branch's deploy
with `PREPCOOK_APP_URL` pointing at it.
