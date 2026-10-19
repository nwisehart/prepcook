# Plan decisions

Before a card leaves Plan, a person decides on its plan (the spec, or for a
bug the RCA). There are three decisions:

| Decision | What it means |
| --- | --- |
| **Approve** | The plan is right; the work may go ahead. |
| **Request changes** | The plan needs revising by the agent still working on it. |
| **Re-plan** | Start planning over — for a plan that has already been acted on, typically because merge-request review rejected the approach. |

Approve and request changes are taken while the card is in **Plan**. Re-plan is
taken from **Plan, Execute, Review or Push MR**. A decision on a card anywhere
else is refused.

**An investigation card has no gate.** It is planned like a bug — reproduced,
explained — and its `TRIAGE REPORT` comment moves it to Triaged by itself, in
Chef mode and Prep mode alike ([method.md](method.md#investigations)). Approve
and request changes on one are refused with a 409 (on the card or ticket, a
reply saying so); re-plan still runs its planning again. To have its finding
fixed, convert it to a bug ([api.md](api.md#convert-a-card)): the conversion is
recorded as the plan's approval when the investigation's RCA is at high
confidence and its harness is still on disk, and the card goes straight to
Execute.

Every decision, wherever it was made, is applied the same way and recorded on
the card in its metadata as `approvals`: one record per decision with
`decision`, `by`, `source` (`lavish`, `tracker`, `card` or `api`), `comment` and
`at`. The card also gets a `gate:` comment saying what happened.

## Where a decision can be made

### Lavish

The Approve button in a Lavish review session. Prepcook relays the reviewer's
feedback to the planning agent, and records an Approve as a decision from
`lavish`. The agent then records the spec approval itself, which is what moves
the card on.

Planning agents put these buttons in their document only while **Settings →
Orchestrator → Plan approvals** (`approvals.in_app_gate`) is `lavish`, the
default. With `none`, the document has no buttons and the decision comes from
one of the surfaces below (see [playbooks.md](playbooks.md#the-review-gate)).

### The tracker ticket

A comment on the tracker ticket — a Jira comment, a GitHub issue comment, a
PagerDuty note — whose **first line** is one of:

```
RCA: APPROVED
RCA: CHANGES — <reasons>
REPLAN: <reasons>
```

This needs a configured tracker and a card with a ticket. A card made on the
board, or any card when no tracker is configured, is decided in Lavish, on the
card or through the API, which work the same with or without one.

Case does not matter; reasons can continue on the lines below. Only the people
ticked under **Settings → Ticketing → Plan approvers** can decide this way, and
the orchestrator's own account never can. Anyone else gets a reply on the
ticket saying the comment was not acted on. Prepcook answers every decision on the
ticket, so the person who made it sees what happened without opening the
board. Comments are read every two minutes or so.

### The card

A comment on the Prepcook card whose **first line** is one of:

```
APPROVE
REQUEST CHANGES: <reasons>
REPLAN: <reasons>
```

`PLAN: APPROVE` is also accepted. The verb has to end the line or be followed
by a colon or a dash, so prose such as "Approve once CI is green?" is not a
decision. The author must be a non-agent member of the board's organization.

The card cannot tell a person from the agent by author, so the agent's comment
tool refuses a comment that opens with one of these verbs, and the
orchestrator's own comments never do. Decisions on a card are not read while a
person has taken the ticket over; neither are tracker comments.

### The API

```
POST /api/approvals/{issue_id}
GET  /api/approvals/{issue_id}
```

`issue_id` is the card's id. The POST body:

```json
{ "decision": "approve" | "request_changes" | "replan", "comment": "...", "by": "..." }
```

`comment` and `by` are optional; `by` is recorded as given. Responses:

| Status | When |
| --- | --- |
| 200 | Applied; the body's `data` is the `ApprovalRecord` that was written. |
| 401 | No bearer token while the server has no board secret, or a bearer token that is not `PREPCOOK_API_TOKEN`. |
| 404 | No such card. |
| 409 | The card is not in a stage that takes this decision, or is an investigation (which takes only re-plan). |
| 503 | The board could not be reached. |

`GET` answers with every record on the card, oldest first.

Approve:

```sh
curl -sS -X POST "http://localhost:$PORT/api/approvals/$ISSUE_ID" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"decision": "approve", "by": "Ada Lovelace", "comment": "Root cause matches the logs."}'
```

Request changes:

```sh
curl -sS -X POST "http://localhost:$PORT/api/approvals/$ISSUE_ID" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"decision": "request_changes", "by": "Ada Lovelace",
       "comment": "The token expiry is not proven; show the refresh failing in the logs."}'
```

#### The API token

This is the scheme every endpoint in [api.md](api.md) uses. The board's own
pages call the endpoint without a token: the server takes such a request as the
board owner's while it holds a board secret. Scripts and other services send a
bearer token, which is the `PREPCOOK_API_TOKEN` secret:

```sh
prepcook secret set PREPCOOK_API_TOKEN     # prompts for the value
```

The token is compared in constant time. A request that carries a bearer token
is judged on the token alone: a wrong token is refused even while the server
holds a board secret. With no token set, bearer requests are refused and only
the owner's token-less requests are accepted. Remove the secret (`prepcook
secret unset PREPCOOK_API_TOKEN`) to turn token access off again.

## What happens after a decision

A decision made by a person is explicit intent, so it takes effect the same
way in **Chef mode** (orchestrator Auto, autonomy Off: a person moves cards
between columns) and **Prep mode** (autonomy Full: Prepcook moves them itself).

| Decision | From the tracker, the card or the API | From Lavish |
| --- | --- | --- |
| Approve | The approval is recorded and the card moves to Execute, where the executing agent starts. If the plan does not yet pass the gate (a spec with no open questions, an RCA at high confidence), the board refuses the move: the approval stays recorded, the card stays in Plan, and it moves as soon as the verdict passes. | The approval is recorded and handed to the planning agent, which records the spec approval itself. The orchestrator's gate then moves the card to Execute once the verdict passes, as it does for any approved plan. |
| Request changes | Any approval is cleared and the reasons go to the planning agent as a follow-up turn. With no live planning session, nothing is sent; use re-plan. | — |
| Re-plan | The card's claim is cleared and it moves back to Plan, where a fresh planning agent starts on the next tick with the reasons in its prompt. A tracker comment's reasons reach that agent the way every new tracker comment does. | — |

Approve and request changes are refused once the card has left Plan; re-plan
is refused outside Plan, Execute, Review and Push MR. A decision Prepcook cannot
carry out is answered once — on the ticket for a tracker comment, on the card
otherwise — and not retried.
