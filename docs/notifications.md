# Notifications

Prepcook tells people about the things that need one: a plan waiting for a
human, a red pipeline, the daily digest, a Claude seat that ran out. Each of
these is an **event** with a stable kind and typed data. Events go to every
configured **target** — Slack, a JSON webhook, Discord, Microsoft Teams, any
number at once — and each target renders them its own way.

Delivery is best-effort: a target that fails is logged and retried where its
provider retries, and never stops the work that raised the event.

## Configuring targets

**Settings → Integrations → Notifications**, or `notifications.targets` in
`config.json` ([configuration.md](configuration.md#notifications) has every
provider's fields):

```json
{
  "notifications": {
    "sound_enabled": true,
    "push_enabled": true,
    "sound_file": "COW_MOOING",
    "targets": [
      { "kind": "slack", "bot_token": "xoxb-…", "channel": "C0123456" },
      {
        "kind": "webhook",
        "url": "https://ops.example.com/prepcook",
        "secret": "…",
        "events": ["planning_escalated", "pipeline_escalated", "daily_digest"]
      }
    ]
  }
}
```

`events` limits a target to the kinds listed; unset, or `["*"]`, sends it
everything. Secret fields (tokens, webhook URLs, the signing secret) are best
kept out of the file: Prepcook reads them from the secrets store first, under
`<KIND>_<FIELD>` (`SLACK_BOT_TOKEN`, `WEBHOOK_SECRET`, `DISCORD_WEBHOOK_URL`,
…), then from the config, then from the environment variable of that name.

A board can change its routing under
`orchestrator.project_defaults["<board id>"].notifications`: `enabled: false`
silences it, `skip_global: ["slack"]` drops the global targets of a kind, and
`targets` adds targets only that board sends to. A board's own target reads
its configured secret value before the store, so two boards can post to two
Slack workspaces. Events about the Claude seat are not about any board and go
to the global targets.

A config that still carries `slack_webhook_url`, `slack_bot_token` and
`slack_channel`, or a board `slack` override, is converted to targets the
first time it is read.

## Events

Every event has a `kind`, a `severity` (`info`, `warn` or `urgent`) and the
data below. The kinds are a contract: a target's `events` filter and the
webhook `type` field use them, and they do not change.

Shared shapes:

- **`IssueRef`** — the ticket an event is about:
  `{ "key": string, "title": string, "url": string | null, "card_url": string | null }`.
  `key` is the tracker key (`BUG-1234`), or the card's own id when the card
  has no ticket; `title` is the card title, which usually starts with
  `"<key>: "`; `url` is the ticket on the tracker; `card_url` is the card on
  the board.
- Timestamps are RFC 3339 in UTC (`2026-09-04T15:00:00Z`, with a fractional
  part when there is one); dates are `YYYY-MM-DD`.
- Text fields have every known secret value replaced by `[redacted]`.

| Kind | Severity | When | `data` |
| --- | --- | --- | --- |
| `seat_limit_hit` | `warn` | The Claude seat hit a usage window; agent launches pause until it reopens. | `{ "window": string, "minutes": number }` — the window's name (`session`, …) and roughly how many minutes are left. |
| `seat_exhausted` | `urgent` | Several windows in a row ran out with nothing getting through. | `{ "rounds": number, "report": string }` — consecutive windows exhausted, and what the seat last reported. |
| `seat_expired` | `urgent` | The seat monitor found the seat's login expired; no agent can run. | `{}` |
| `seat_recovered` | `info` | The seat answers again after `seat_expired`. | `{}` |
| `ticket_closed_with_open_mrs` | `warn` | A ticket was closed on the tracker while merge requests from its card were open; the card was retired. | `{ "issue": IssueRef, "mr_numbers": number[] }` |
| `daily_digest` | `info` | Once a day per board, at `orchestrator.digest_at_local`. | A digest, below. |
| `planning_escalated` | `urgent` | Under autonomy, a plan did not clear the bar and waits in Plan for a person. | An escalation, below. |
| `runner_fault` | `urgent` | The script runner cannot do its job for a card (a setup problem, not the branch's); once per card and outcome. | `{ "issue": IssueRef, "step": string, "outcome": string, "detail": string }` — what could not run (`Pre-merge verification`), the runner's outcome label, and its message or log tail. |
| `review_loop_capped` | `urgent` | Under autonomy, review kept requesting changes until the round cap. | `{ "issue": IssueRef, "rounds": number }` — the cap that was hit. |
| `review_round_abandoned` | `urgent` | Under autonomy, review findings could not be handed to a fixing turn. | `{ "issue": IssueRef, "reason": string }` |
| `pipeline_escalated` | `urgent` | A merge request's pipeline needs a person (the fix attempts are spent, or pushing one failed). | `{ "issue": IssueRef, "mr_url": string, "reason": string }` |
| `replan_requested` | `warn` | Merge request review rejects the approach; the agent stopped and re-planning is a person's call. | `{ "issue": IssueRef, "reason": string, "mr_url": string \| null }` |
| `blocker_mr_opened` | `warn` | A merge request was opened for a blocker-priority ticket. | `{ "issue": IssueRef, "mr_url": string, "summary": string }` — `summary` is the title without the key. |
| `mention_needs_human` | `warn` | The agent was @-mentioned on the tracker and no session is running to answer. | `{ "issue": IssueRef, "mentions": [{ "author": string, "text": string }] }` — each mention's text clipped to 700 characters. |
| `investigation_completed` | `info` | An investigation card posted its triage report and was triaged (once per card). | A completed investigation, below. |
| `card_converted` | `info` | A card's work class was changed: an investigation converted to a bug or a story. | `{ "issue": IssueRef, "from": "investigation", "to": "bug" \| "story", "by": string, "column": string }` — who converted it, as the caller named them, and the column it moved to (Execute when the RCA and harness carried over as the plan, else Plan). |

### `daily_digest`

```text
{
  "date": "YYYY-MM-DD",          // the local date the digest is for
  "generated_at": timestamp,
  "timezone": string,            // IANA zone `date` is in
  "next_run": "YYYY-MM-DD",      // the next digest's local date
  "awaiting_staging": number,    // merged changes not yet on staging
  "sections": [
    {
      "id": "needs_merge" | "needs_answer" | "not_moving",
      "title": string,           // "Needs a merge"
      "waiting_on": string,      // "an engineer to read a diff"
      "items": [
        {
          "key": string,
          "url": string | null,     // the ticket
          "title": string,
          "mr_url": string | null,
          "urgent": boolean,        // urgent priority (P0)
          "days": number | null,    // days since the card moved; null when not from the board
          "note": string | null     // why it is not moving (not_moving only)
        }
      ]
    }
  ]
}
```

All three sections are always present, in that order, each sorted urgent
first and then longest waiting; a section with nothing in it has an empty
`items`.

### `planning_escalated`

```text
{
  "issue": IssueRef,
  "tracker": string | null,          // the tracker's name ("Jira") when the ticket lives on one
  "plan_kind": "rca" | "spec",
  "confidence": "low" | "medium" | "high" | null,   // set for an rca
  "open_questions": number | null,   // set for a spec
  "blocked_on": string,              // "Low confidence RCA", "2 open question(s) on the spec"
  "root_cause": string,              // the summary's first sentence
  "why_not_higher": string | null,   // the agent's reason the RCA is not rated higher
  "next_steps": string[],            // steps the agent labelled as such; often empty
  "summary": string                  // the whole planning summary
}
```

### `investigation_completed`

```text
{
  "issue": IssueRef,
  "report": {
    "what_happens": string,      // each section's text as the agent wrote it;
    "where": string,             // "" when the agent left a section out
    "why": string,
    "scope": string,
    "workaround": string,        // "none found" when there is none
    "confidence": string,        // the section: the level, then one line why
    "markdown": string           // the whole TRIAGE REPORT comment
  },
  "rca_url": string | null,      // the RCA in the knowledge store, when filed
  "rca_id": string | null,       // its document id (a UUID)
  "harness": {
    "path": string | null,       // ".repro/<key>/repro.mjs" while it is on disk
    "outcome": "reproduced" | "not_reproduced" | "unknown" | "missing"
  },
  "confidence": "low" | "medium" | "high" | null,   // the level the Confidence section opens with
  "source": {                    // who filed the card through POST /api/cards; null otherwise
    "kind": "tammy" | "incident" | "api",
    "ref": string,               // the filer's own reference: a thread URL, an incident id
    "reporter": string | null
  } | null
}
```

`harness.outcome` is read from the card's `REPRODUCTION:` comment: `reproduced`
when any of the harness's cases failed (the reported behaviour happened),
`not_reproduced` when every case passed, `unknown` when the comment reports no
cases that could be read, `missing` when there is no such comment. Every field
is present; the ones that do not apply are `null`.

## Slack

A Slack target needs an incoming webhook URL, or a bot token and a channel,
or both. Simple events are one message: through the webhook when there is
one, otherwise from the bot. With a bot token:

- **The digest** is kept on a *Daily Digest* canvas at the top of the channel
  (rewritten every run, every card listed); the message then carries the
  counts and the P0s. Without one the message carries the whole list, capped
  at twelve cards a section.
- **A planning escalation** is posted by the bot as a short card, with the
  full planning summary as the first reply in its thread. The thread is
  recorded on the card, and every orchestrator tick while the card sits in
  Plan reads it: a teammate's reply is forwarded to the blocked agent as a
  follow-up, once. If the bot's post fails and a webhook is configured, the
  escalation goes through the webhook instead.

Without a bot token the escalation goes through the webhook with a trimmed
summary in place of the thread, and nothing is read back: the escalation
reaches people, but their answer has to reach the agent through the card or
the ticket. The same holds when no Slack target has a bot at all — every other
target still receives the escalation.

The bot needs `chat:write`, `channels:history` (or `groups:history` for a
private channel) and `canvases:write`, and must be a member of the channel.
The same bot backs the agents' `slack_read_thread` tool.

## Webhook

Each event is POSTed to the target's `url` as one JSON document:

```json
{
  "type": "pipeline_escalated",
  "severity": "urgent",
  "occurred_at": "2026-09-04T15:00:00.000Z",
  "prepcook": {
    "version": "0.1.44",
    "board_url": "https://prepcook.example.com/projects/7d0c6a0e-0000-4000-8000-000000000001"
  },
  "data": {
    "issue": {
      "key": "BUG-1234",
      "title": "BUG-1234: Export drops the last row",
      "url": "https://example.atlassian.net/browse/BUG-1234",
      "card_url": "https://prepcook.example.com/projects/7d0c6a0e-0000-4000-8000-000000000001/issues/3f1e9b52-0000-4000-8000-000000000002"
    },
    "mr_url": "https://gitlab.example.com/acme/app/-/merge_requests/812",
    "reason": "lint still fails after two fix attempts"
  }
}
```

`type` is the event kind and `data` its data, as listed above. `board_url` is
the board the event came from, or the app's root for an event that belongs to
no board (the seat events).

Headers:

| Header | Value |
| --- | --- |
| `Content-Type` | `application/json` |
| `X-PrepCook-Event` | The event kind, the same as `type`. |
| `X-PrepCook-Delivery` | A UUID for this delivery. Retries of one delivery carry the same value, so a receiver can drop duplicates. |
| `X-PrepCook-Signature` | `sha256=` and the lowercase hex HMAC-SHA256 of the raw request body, keyed with the target's `secret`. Absent when no secret is configured. |

A response of 2xx is success. A 5xx, a timeout or a refused connection is
retried three times, after 0.5, 1 and 2 seconds; any other status is final.

**Settings → Test connection** sends a `ping`: the same envelope with
`"type": "ping"`, `"severity": "info"` and `"data": {}`, signed like any
other delivery.

### Verifying the signature

Compute the HMAC over the raw bytes received — before any JSON parsing, which
would change them — and compare in constant time:

```ts
import { createHmac, timingSafeEqual } from 'node:crypto';

export function verifyPrepCookSignature(
  rawBody: Buffer,
  signatureHeader: string | undefined,
  secret: string
): boolean {
  if (!signatureHeader?.startsWith('sha256=')) return false;
  const expected = Buffer.from(
    'sha256=' + createHmac('sha256', secret).update(rawBody).digest('hex')
  );
  const received = Buffer.from(signatureHeader);
  return (
    expected.length === received.length && timingSafeEqual(expected, received)
  );
}

// Express: keep the raw body for this route.
// app.post('/prepcook', express.raw({ type: 'application/json' }), (req, res) => {
//   if (!verifyPrepCookSignature(req.body, req.get('X-PrepCook-Signature'), SECRET)) {
//     return res.sendStatus(401);
//   }
//   const event = JSON.parse(req.body.toString('utf8'));
//   res.sendStatus(204);
// });
```

## Discord and Teams

**Discord** posts each event as one embed through the channel webhook: the
event's headline as the title, its detail as the description, coloured by
severity and linked to the card, ticket or merge request. Mentions are never
parsed, so quoted text cannot ping anyone. Test connection reads the webhook
(a GET on its URL).

**Teams** posts each event as an Adaptive Card through an incoming webhook (a
Teams workflow that posts webhook requests to a channel, or a legacy Incoming
Webhook connector), with an **Open** button to the same link. Test connection
posts a test card; an incoming webhook has nothing to read.

## Adding a provider

Providers live in `crates/services/src/services/notifications/`, one file each.
To add one:

1. Write a struct with `new(&TargetSettings, …) -> Result<Self, NotifyError>`
   that reads its fields through `settings.get("<key>")` and returns
   `NotifyError::NotConfigured` when a required one is missing, and a
   `descriptor() -> Descriptor` naming its `kind`, display name, one-sentence
   summary and fields (`FieldKind::Secret` for anything that is a credential;
   its secrets-store name follows from the kind and the key).
2. Implement `Notifier` for it: `kind()`, `send(&Event)` (render from the
   event's data — `Event::headline()`, `Event::body()` and `Event::link()` give
   a plain rendering to start from), `conversational()` (`None` unless the
   service can read replies back, in which case also implement
   `Conversational`), and `test()` (one cheap read; a test message only where
   the service offers nothing to read).
3. Add a variant to `NotifierKind` and to `NotifierProvider`, and an arm in
   `NotifierKind::descriptor` and `build_provider` in `mod.rs`.
4. Test it against an HTTP mock (`wiremock`, as the other providers do); no
   test may reach the network.
5. Run `scripts/gen-config-docs.py` to regenerate the provider table in
   `configuration.md`. Settings needs no change: it renders every provider from
   its descriptor.

A new **event** is a variant on `Event` with its data as named fields, an
entry in `Event::KINDS` and the stability test beside it, a severity, its
Slack rendering in `slack.rs`, and a row in the table above.
