---
name: "prepcook-self-review"
description: "Review my own branch before I push or open a pull request — weighing reuse of existing helpers and fit with established patterns above correctness — then fix the mechanical findings and hand back only what needs my judgment. Use after /prepcook-fix and before /prepcook-review: whenever I ask for a review of my own work, a check before pushing, a look at my branch or diff, or whether a change is ready for review. Usage: /prepcook-self-review <KEY>"
---

# Review my branch

<!-- Hand-maintained, not generated: toolkit/generator/generate.py neither writes nor checks this file. In the prepcook-* loop it runs after /prepcook-fix, in the same session, and before /prepcook-review, which runs cold in a fresh one. Edit freely; a good edit is a proposal like any other — open a pull request. -->

A branch earns its merge by making the codebase easier for whoever works in it
next. Correctness is table stakes — the real question is whether this change
improves the codebase or quietly erodes it. That question is worth asking before
a reviewer's time is spent on it, which is what this skill is for.

You are reviewing my work, so you may fix things. What you may not do is hand
back a list I have to re-derive: anything mechanical, you repair; anything that
needs a decision, you bring me with enough context to decide in one read.

## Establish the diff

Review against the branch's real target, not `main` by reflex. The target is the
one its pull request names when one exists (ask `gh` or `glab`, whichever your
host uses; on GitLab it is a merge request), else the remote's default branch:

```bash
target="$(git symbolic-ref --short refs/remotes/origin/HEAD)"   # unless the pull request says otherwise
git log --oneline "$target"..HEAD
git diff "$target"...HEAD --stat         # three dots: what the pull request page would show
```

Two dots mixes in unrelated churn from the target branch.

Exclude the noise, and read an excluded file only if the change is about it:
`':(exclude)*.lock' ':(exclude)*lock.json' ':(exclude)*lock.yaml' ':(exclude)*.snap' ':(exclude)*.min.*'`
plus any generated file the repository's `CLAUDE.md` or `AGENTS.md` names.

## Scale the reading

The findings that matter most are usually in code that is *not* in the diff — the
helper that already existed, the pattern this branch forked instead of extending.
So read outward from the change: the call sites of every symbol touched, the
files beside the ones changed, and the `CLAUDE.md` / design docs for the area.

- Up to ~6 changed files: read inline.
- Up to ~20: two Explore subagents over **disjoint** file sets. Overlapping
  scopes re-find the same things and pay twice.
- Beyond that, or when the branch introduces a new pattern: three at most, one
  carrying the prior-art question, plus a **fresh-eyes pass** — a subagent that
  reads the changed files *as they now exist*, never the diff, with no account of
  what changed or why, and reports only where it got confused. Its confusion is
  the finding; that is the cheapest readability signal available.

Ceilings, not quotas. A cohesive diff, or one that is mostly tests, needs none of
this.

## Judge, in order of weight

**Reuse and DRY.** For every new function, component, hook or util, hunt for an
existing equivalent before concluding it is new: the name stem, the shape of the
signature, the obvious homes. A working duplicate is still a finding — two places
to fix the next bug. Catch repetition inside the branch too: the same logic in
three files wants to be one helper.

**Pattern fit.** Red flags: a special-cased fork of a shared component instead of
extending it; a "temporary" workaround; a boolean prop that makes one thing behave
like a different thing; a bypass of the established data-fetching or state layer.
If the existing pattern genuinely cannot accommodate the need, the finding is
"this pattern needs updating", with a sketch — never "workaround accepted".

**Readability.** Names say what things are, in full words; a name that needs a
comment is the wrong name. Functions small enough to hold in your head. Comments
state a constraint the code cannot express and read cold, with no ticket or pull
request to hand: BAD `// New validation per BUG-1234`, GOOD `// Reject >1MB; larger
payloads crash the downstream parser`. Dead code, commented-out blocks and stale
TODOs go.

**Correctness and tests.** A bug needs a concrete failure scenario — inputs or
state, then the wrong outcome. New behaviour needs new or updated tests, and a
bug fix without a regression test is incomplete.

## Verify before reporting

Every finding survives verification or gets dropped. A short accurate review
beats a long padded one, and here you are about to act on your own findings, so a
wrong one does real damage.

- "Use existing helper X" requires reading X and confirming it fits this call
  site. A wrong reuse suggestion costs more than the duplication did.
- A bug claim requires its failure path checked against the real code, not the
  hunk.
- A convention claim must cite the file that states it — `CLAUDE.md`,
  `AGENTS.md`, `DESIGN.md`, `COMPONENTS.md` — or a specific existing helper. A
  pattern you inferred but cannot cite is not grounds for a finding.

Run the checks that cover what you touched — the targeted tests and linters for
those files, or the `[app] test` command in `.prepcook/scripts.toml` when nothing
narrower exists — rather than a full build. On a bug ticket, run the harness under
`.repro/<KEY>/` too, against the app you stood up from `.prepcook/scripts.toml`
(rebuild and restart it first if your repairs touched it): the harness is the
acceptance test, and a repair that makes it fail is not a repair. If a repair
changes behaviour in any way you did not intend, revert it and report instead.

## Fix, then report

**Fix and commit** the mechanical, behaviour-preserving ones: call the existing
helper, collapse the duplication, rewrite the comment, rename the unreadable
thing, delete the dead code. One commit, message naming the class of change —
not a mixed commit with unrelated repairs. Re-run the tests that cover the files
you touched.

**Report** everything else, in three groups:

```
## Fixed (committed)
- src/lists/ItemRow.tsx — ItemRow now calls visibleItems(); three other
  callers already did

## Needs your call
- server/lists.js:210 — archived items are filtered after the count, so the
  returned total includes them. A shared list reports more items than it shows.
  Fix depends on whether the count is meant to be pre- or post-filter.

## Worth doing, not now
- server/sharing.js:88 — this file is the third place building the same
  invitation payload; extracting it is a bigger change than this branch should
  carry.
```

If a group is empty, say so in a line rather than padding it. And if the branch
is genuinely clean, say that plainly — inventing a finding to look thorough wastes
the next reader's attention, which is the resource this whole exercise is trying
to protect.

Then the cold read: `/prepcook-review` in a fresh session, so it has no memory of
this one.
