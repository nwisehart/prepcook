# The method

A prep cook does the prep so the chef can cook. Prep Cook does the prep on a
bug — reproduces it, explains it, fixes it, reviews the fix, opens the pull
request — and the engineer who owns the ticket signs off.

Prep Cook works a bug the way a careful engineer would, in a fixed order: make it
happen, explain it, fix the mechanism, have someone else check the fix, then
ship it. Each step is a stage, each stage is a column on the board, and a card
moves through them left to right:

```
To Do → Plan → Execute → Review → Push MR → Close → Clean up
          ↘ Triaged (investigations stop here)
```

The orchestrator watches the columns from Plan onwards. When a card assigned to
you enters one, it starts that stage's work: an agent session in the card's git
worktree for Plan, Execute and Review, or its own work for Push MR and Clean up.
To Do and Close are where cards wait for a person. A forward move goes one
column at a time; any card can be moved back, or straight to Close.

A card is in the workflow when it has a work class (`bug`, `investigation`,
`story`, `epic`, `subtask`). A card imported from the tracker gets one from its
issue type. A card made on the board gets one from its tag: tag it `bug` before
moving it to Plan (when a card carries several class tags, the first in that
order wins). A card filed by another service through `POST /api/cards` gets the
one the request names ([api.md](api.md)). The work class picks the playbook each
stage's agent starts from ([playbooks.md](playbooks.md)).

A card is one of three things: a **bug** to fix, an **investigation** to
explain, or a piece of planned work (a **story**, an **epic**, a **sub-task**).

## The stages

| Stage | Produces | Waits on |
| --- | --- | --- |
| **Plan** | A reproduction harness, a `REPRODUCTION:` comment on the card, and a root-cause analysis (RCA) with a verdict: confidence, open questions, a summary. | A person approving the RCA. The card cannot enter Execute until the RCA is approved and its confidence is high. |
| **Execute** | The smallest fix for the mechanism the RCA isolated, its regression tests, and commits rebased onto the target branch. Nothing is pushed. | The fix passing the reproduction and the tests. With autonomy set to Full, the pre-merge verify ([scripts.md](scripts.md#what-each-stage-does)) also has to pass. |
| **Review** | An independent review in a fresh worktree: a verdict and typed findings, recorded on the card ([review.md](review.md)). | `REVIEW: APPROVE`. A `REQUEST CHANGES` goes back to the implementation agent as a fix round. |
| **Push MR** | The pushed branch and the pull request (GitHub) or merge request (GitLab), with the reproduction as its first comment and reviewers picked from blame. | People reviewing and merging it. Their comments are fed back to the implementation agent. Once everything merged, the card moves to Close. |
| **Clean up** | The repository's clean-up scripts run, the card's worktrees archived. | Nothing; it is the last column. |

Plan comes first because a fix without a reproduction is a guess
([reproduce-first.md](reproduce-first.md)). Every approved bug RCA is filed in
the knowledge store and linked into the root-cause graph
([rca-graph.md](rca-graph.md)).

## Investigations

An investigation is a reported problem Prep Cook reproduces and explains but
does not fix — typically filed by a support agent from a customer conversation,
which then translates the answer for the customer.

- **Plan** runs exactly as for a bug, from the `investigation:planning`
  playbook: the app stood up, the harness first, the `REPRODUCTION:` comment,
  then the RCA and its verdict. The agent then posts a `TRIAGE REPORT` comment
  in a fixed shape — `## What happens`, `## Where`, `## Why` (from the RCA, one
  paragraph), `## Scope` (who and what is affected, and whether it is
  configuration or a defect, with the evidence), `## Workaround` (or "none
  found") and `## Confidence` (high, medium or low, with one line why) — and
  stops. It does not plan a fix or write a spec.
- **Triaged.** Once that comment is on the card, the orchestrator records the
  report on the card, sends `investigation_completed` to the board's
  notification targets ([notifications.md](notifications.md)), copies the
  report onto the tracker ticket (moving it to `status_mapping.triaged` when
  that is set; on an incident, as a note, resolving it only with
  `incidents.resolve_on_triage`), and moves the card to the **Triaged**
  column. There is no
  approval gate, in Chef mode or Prep mode. Triaged is terminal: nothing runs
  there.
- **Convert.** An engineer who wants it fixed converts the card — the class
  label's menu on the card (**Convert to bug**), or
  `POST /api/cards/{id}/convert` ([api.md](api.md#convert-a-card)). The tag and
  class change; every comment, attachment, the RCA and the harness stay. When
  the RCA is at high confidence and the harness is still on disk, they are the
  plan: the conversion is recorded as its approval and the card goes straight
  to Execute, where `bug:executing` starts in the same worktree. Otherwise it
  goes back to Plan and is planned as a bug. Converting to a story always
  plans again.

An investigation never executes: the Executing gate refuses it ("investigation
cards do not execute; convert to a bug first"), and so does the board.

New boards get the Triaged column between Plan and Execute. A board made before
it existed does not: an investigation that finishes there stays in Plan, tagged
`needs-column`, and the orchestrator logs it once. Add a column named
`Triaged` and the card moves on the next tick; Prep Cook never adds a column to
a board by itself.

Stories, epics and sub-tasks take the same columns. In Plan a story gets an
implementation spec, an epic a plan with a breakdown into sub-tasks, and a
sub-task a spec for its slice; the card cannot advance while the spec has open
questions. In Execute an epic creates its sub-tasks instead of code. Nothing but
a bug, and an investigation in Plan, stands the app up.

## The two modes

The same stages run in two modes. The mode is **Settings → Orchestrator →
Autonomy mode**, global or per board.

**Chef mode** (autonomy off, the default). Prep Cook works your tickets and you move
them. You import tickets from the tracker or make cards on the board, drag them
between columns, and approve plans in Lavish, the review page each plan opens
in (the **Review plan** button on the card). Stories and epics get a spec or a
breakdown the way bugs get an RCA. A review's `REQUEST CHANGES` still starts a
fix round on its own; advancing an approved card is yours.

**Prep mode** (autonomy on). Prep Cook works on its own, on the machine meant to
run unattended. Tickets the tracker's pickup filters select are imported as
they arrive. Cards move themselves: a plan that clears the bar (an RCA at high
confidence, a spec with no open questions) approves itself and goes to Execute;
one below the bar is escalated to Slack and waits in Plan for a person, whose
decision comes from the tracker ticket, the card or the approvals API
([approvals.md](approvals.md)). A finished, verified fix goes to Review, and an
approved review goes to Push MR.

Either way a person merges. When an engineer takes a ticket over, Prep Cook steps
aside ([working-alongside-people.md](working-alongside-people.md)).

## Without a tracker

A tracker is optional. With none configured, every card is made on the board
and the card is the only record: nothing is imported, nothing is posted to a
ticket, and no sweep that reads a ticket's status runs. Every stage still runs,
plan decisions come from Lavish, the card or the API, and the pull request is
still opened. A card made on the board while a tracker is configured is treated
the same way. The [quickstart](../README.md#quickstart) uses this path.

## Tickets or incidents

Where cards come from is the ticketing mode (**Settings → Integrations →
Ticketing**, `ticketing.mode`).

**Tickets** (the default). The source is a ticket tracker — Jira, or GitHub
Issues through the `gh` CLI — and each ticket's type picks its work class.
Stages show on the ticket as the statuses (on GitHub, the labels) that
`status_mapping` names.

**Incidents.** The source is an incident tool — PagerDuty. Every open incident
on the configured services becomes an **investigation** card in Plan: Prep
Cook acknowledges it, reproduces and explains the problem, and adds the
triage report to the incident as a note. It never pages anyone and resolves
the incident only when configured to ([incidents.md](incidents.md)).

The two modes above (Chef and Prep) apply in either.

## More

- [scripts.md](scripts.md): how Prep Cook installs, starts, seeds and tests the app.
- [playbooks.md](playbooks.md): the prompt each stage's agent starts from.
- [approvals.md](approvals.md): every way a plan is decided.
- [incidents.md](incidents.md): Prep Cook as a first responder on PagerDuty
  incidents.
- [api.md](api.md): filing, reading and converting cards, and the other HTTP
  endpoints scripts use.
- [configuration.md](configuration.md): every setting and its default.
- [toolkit/README.md](../toolkit/README.md): the same method at your terminal,
  without the board.
