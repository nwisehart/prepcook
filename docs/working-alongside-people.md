# Working alongside people

Prepcook works tickets that people also work on, read and report on. These are the
rules it follows so that it helps and does not talk over anyone. Most of them
read the tracker; a card made on the board has no ticket, and of these only
the card-comment decisions apply to it.

## Taking a ticket over

An engineer takes a ticket over by assigning it to themselves in the tracker.
The orchestrator reads each in-flight ticket's assignee every five minutes or
so; an assignee who is not the agent's own tracker account is a takeover. From
then on:

- **What was running finishes; nothing new starts.** A planning session
  finishes its RCA, an executing or review run finishes its turn, a merge
  request stays as it is. No stage launches and the card does not advance.
- **Nothing is injected.** Card comments, tracker comments, merge-request
  comments and plan decisions are not passed to the agent while the ticket is
  held. A person who has taken a ticket over does not need the agent revived
  underneath them. The exception is a comment that @-mentions the agent: that
  person is asking it something, and the question goes to the session that has
  the context.
- **The ticket says so once.** Prepcook posts one comment on the ticket naming the
  stage it was in, what still runs, and how to hand it back, and tags the card
  `taken-over`. The card panel shows the pause and who holds it.
- **On an older board the card is parked in Close** once the in-flight work has
  finished, so the board reads "the person owns this now"; a board with Shipped
  has no Close, and the card stays where it is under its `taken-over` tag.
  Clean up still runs on a card that reaches it; it archives worktrees and
  touches nothing the person works on.
- **In Prep mode, the RCA reaches them.** When a taken-over ticket's planning
  verdict lands, the plain-language summary and a link to the full RCA are
  posted on the ticket, once.

A ticket that has already shipped is not taken over by its assignee of record:
the marker is set silently, with no comment.

### Handing a ticket back

Unassign yourself in the tracker. Prepcook posts "Handed back" on the ticket and
removes the `taken-over` tag, and stays parked: move the card back a column and
forward again to run that stage's automation again. Nothing restarts on its
own, because the card may no longer be where the automation left it.

## When it speaks on a ticket

The agent answers on a ticket only when it is its turn to.

- **During Plan**, new tracker comments from people are forwarded to the
  planning agent as they arrive (read every five minutes or so). The first poll
  only notes what is there, since those comments were imported with the
  description. The agent's own comments are never forwarded back to it.
- **While a card is in flight**, a tracker comment that @-mentions the agent is
  delivered to the session working the ticket, which holds the context to
  answer it. With no session running, a card comment (and a Slack post, when
  Slack is set up) asks a person instead.
- **After a ticket has shipped** (Prep mode only), a fresh human comment starts
  one analyze-only session from the `comment_reply` playbook that answers on
  the ticket. If a person holds
  the ticket, it answers only a comment that @-mentions it; the rest of that
  thread is theirs.

Mentions are matched on the tracker account id, which a rename cannot change.

## The daily update

With `orchestrator.eob_at_local` set (a local `HH:MM`, read in
`orchestrator.daily_timezone`), each in-flight ticket gets one `EOB update:` comment a day,
written for the reporter: whether anything happened, and who the ticket is
waiting on. It is skipped when:

- the update would say the same as the last one; the same paragraph twice says
  nothing new, whatever the priority. A card that is not moving shows up in the
  engineers' digest instead;
- the ticket is done in the tracker;
- a person holds the ticket.

When the ticket waits on a question for its reporter, the update addresses the
reporter by mention.

## Approvals through comments

A plan is decided from a comment as well as from Lavish or the API
([approvals.md](approvals.md)):

- On the **ticket**, `RCA: APPROVED`, `RCA: CHANGES — <reasons>` or
  `REPLAN: <reasons>` as the first line, from someone listed under
  **Settings → Ticketing → Plan approvers**. Anyone else gets a reply saying the
  comment was not acted on. Every decision is answered on the ticket.
- On the **card**, `APPROVE`, `REQUEST CHANGES: <reasons>` or `REPLAN: <reasons>`
  as the first line, from a member of the board's organization who is not an
  agent. This works without a tracker.

Neither is read while a person holds the ticket.

## Reopened tickets get follow-ups

A fix that shipped and did not hold is new work, not a continuation. When a
shipped ticket is moved back out of done, or a reporter replies with
`NOT FIXED` as the first line of a comment, Prepcook files a **follow-up ticket**
instead of re-arming the old card:

- It carries the original's issue type, its priority (P1 when the original was
  P0, so one incident is counted once), the reporter's words, and links to the
  previous merge request and RCA, with the instruction to start from what that
  attempt concluded.
- It is labelled `prepcook-followup`, linked as blocking the original, assigned to
  the agent's account (which is what imports it into Plan), and watched by the
  original's reporter and whoever reported the regression.
- The original waits in **Blocked**, with a comment linking the follow-up.
- A further report while the follow-up is open is added to it, not filed as a
  third ticket.
- In Prep mode, once the follow-up's analysis is in, its title is changed once
  to the analysis's plain-language summary, so it is named for the actual cause
  rather than the symptom the reporter described. A title someone has edited by
  hand is left alone.
- When the follow-up ships, the original returns to the status it held, and its
  reporter is asked to confirm the problem is gone once the release is out.
