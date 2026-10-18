# The HTTP API

Prep Cook's server is the board's own backend; most of its routes exist for
the board's pages and change without notice. The endpoints on this page are the
ones meant for scripts and other services — a support agent that files cards,
an incident tool, a CI job — and they are kept stable.

All of them are on the local server (`http://localhost:$PORT`, where `PORT` is
`PREPCOOK_PORT`, 4100 by default) under `/api`, take and return JSON, and
answer in one envelope:

```json
{ "success": true, "data": { … }, "error_data": null, "message": null }
```

A failure has `"success": false` and says why in `message`.

## Authentication

The card and approval endpoints accept two callers:

- **A service**, sending `Authorization: Bearer <token>`, where the token is
  the `PREPCOOK_API_TOKEN` secret (`prepcook secret set PREPCOOK_API_TOKEN`).
  A request with a bearer token is judged on the token alone, compared in
  constant time; with no token set, every bearer request is refused.
- **The board owner**: a request with no `Authorization` header, accepted while
  the server holds a board secret. This is how the board's own pages call them.

Filing a card (`POST /api/cards`) takes the token only. A refused request is a
`401` with the reason in `message`. See [approvals.md](approvals.md#the-api-token).

The script and app-run endpoints at the end of this page are the local server's
own, for the Settings page and for agents running on the same machine; they
take no token.

## Cards

### File a card

```
POST /api/cards
```

Puts a card on a board for the orchestrator to work, assigned to the board's
user. The body:

| Field | Required | Meaning |
| --- | --- | --- |
| `project_id` | no | The board. Defaults to `orchestrator.default_board`, else the first board under Settings → Orchestrator. |
| `title` | yes | The card's title, up to 255 characters. |
| `description` | no | Markdown. |
| `work_class` | yes | `investigation`, `bug` or `story`. |
| `origin` | yes | `{ "kind": "tammy" \| "incident" \| "api", "ref": string, "reporter"?: string }` — who filed it, and the filer's own reference (a thread URL, an incident id). Stored on the card and shown on it: "Filed by Tammy from *thread*". |
| `attachments` | no | `[{ "name": string, "url": string }]`, each an `http(s)` URL. Listed as links in the description, where the planning agent sees them. |
| `metadata` | no | An object kept on the card under `external`; it never stands in for a workflow key. |
| `start` | no | `true` places the card in Plan, where the orchestrator starts on it within a poll; `false` (the default) leaves it in To Do. |

```sh
curl -sS -X POST "http://localhost:$PORT/api/cards" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "CSV export drops the last row",
    "description": "Acme says every export over 500 rows is one short.",
    "work_class": "investigation",
    "origin": {
      "kind": "tammy",
      "ref": "https://acme.slack.com/archives/C0123456/p1760000000000100",
      "reporter": "Grace Hopper"
    },
    "attachments": [{ "name": "export.csv", "url": "https://files.example.com/export.csv" }],
    "metadata": { "customer": "acme" },
    "start": true
  }'
```

`201 Created`:

```json
{
  "success": true,
  "data": {
    "id": "3f1e9b52-0000-4000-8000-000000000002",
    "url": "http://localhost:4100/projects/7d0c6a0e-0000-4000-8000-000000000001/issues/3f1e9b52-0000-4000-8000-000000000002",
    "key": "PC-42"
  },
  "error_data": null,
  "message": null
}
```

| Status | When |
| --- | --- |
| 201 | Filed. |
| 400 | The body is not valid: a blank or overlong title, a class other than the three, a blank `origin.ref`, an attachment without a name or an `http(s)` URL, `metadata` that is not an object — or no `project_id` and no board configured. Nothing was written. |
| 401 | No bearer token, or the wrong one. |
| 404 | No such board. |
| 409 | The board has no Plan column (`start: true`) or no To Do column. |
| 503 | The board could not be reached. |

### Read a card

```
GET /api/cards/{id}
```

```json
{
  "success": true,
  "data": {
    "id": "3f1e9b52-0000-4000-8000-000000000002",
    "key": "PC-42",
    "title": "CSV export drops the last row",
    "url": "http://localhost:4100/projects/7d0c6a0e-…/issues/3f1e9b52-…",
    "project_id": "7d0c6a0e-0000-4000-8000-000000000001",
    "status": "triaged",
    "column": "Triaged",
    "work_class": "investigation",
    "report": {
      "what_happens": "Exports over 500 rows stop one row short.",
      "where": "Reports → Export, CSV only; XLSX is complete.",
      "why": "The exporter pages by 500 and stops when a page comes back full, so the last partial page is never fetched.",
      "scope": "Every CSV export over 500 rows, every account. A defect, not configuration: the harness fails on a fresh seed.",
      "workaround": "Export as XLSX.",
      "confidence": "High — reproduced on main, and the candidate change makes the harness pass.",
      "markdown": "TRIAGE REPORT\n\n## What happens\n…"
    },
    "confidence": "high",
    "harness": { "path": ".repro/PC-42/repro.mjs", "outcome": "reproduced" },
    "comments": 6,
    "rca_id": "9c1d2e3f-0000-4000-8000-000000000003",
    "origin": {
      "kind": "tammy",
      "ref": "https://acme.slack.com/archives/C0123456/p1760000000000100",
      "reporter": "Grace Hopper"
    }
  },
  "error_data": null,
  "message": null
}
```

`status` is where the card is in the workflow — `backlog`, `planning`,
`triaged`, `executing`, `reviewing`, `pushed_mr`, `closed`, `cleanup`, or
`other` for a column Prep Cook does not know — and `column` is the column's name
as the board shows it. `report`, `confidence` and `harness` are `null` until an
investigation has posted its report; `rca_id` is `null` until an RCA is filed
for the card. `404` for no such card.

Rather than polling, a filer can take the `investigation_completed` event
([notifications.md](notifications.md#investigation_completed)), which carries
the same report and the card's `origin` as `source`.

### Convert a card

```
POST /api/cards/{id}/convert
```

Turns an investigation card into a bug — or, less often, a story. The body:

```json
{ "work_class": "bug", "by": "Ada Lovelace" }
```

`by` is optional (it defaults to "an API caller", or "the board owner" for a
token-less request) and is recorded as given. The card's class tag is swapped
and everything else stays: its comments, attachments, RCA, harness and triage
report. Then:

- **To a bug, with a plan.** When the investigation's RCA verdict is at high
  confidence and its harness is still on disk, they are the plan: the
  conversion is recorded as the plan's approval (`approvals`, `source: api`)
  and the card moves to Execute, where `bug:executing` starts in the same
  worktree. Planning does not run again.
- **Otherwise** — a bug without that, or any story — the card moves to Plan and
  is planned from scratch as its new class.

```sh
curl -sS -X POST "http://localhost:$PORT/api/cards/$CARD_ID/convert" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"work_class": "bug", "by": "Ada Lovelace"}'
```

```json
{
  "success": true,
  "data": {
    "id": "3f1e9b52-0000-4000-8000-000000000002",
    "from": "investigation",
    "to": "bug",
    "column": "Execute",
    "plan_carried_over": true
  },
  "error_data": null,
  "message": null
}
```

The card gets a `convert:` comment, and a `card_converted` event goes to the
board's notification targets. On the board, the same action is **Convert to
bug** in the menu of the card's class label.

| Status | When |
| --- | --- |
| 200 | Converted. |
| 400 | `work_class` is not `bug` or `story`. |
| 401 | A wrong bearer token, or no token while the server has no board secret. |
| 404 | No such card. |
| 409 | The card is not an investigation, or the board lacks the column it would move to. |
| 503 | The board could not be reached. |

## Plan decisions

```
POST /api/approvals/{issue_id}
GET  /api/approvals/{issue_id}
```

Approve a plan, ask for changes, or re-plan, from outside the board; `GET`
lists every decision recorded on the card. The body is
`{ "decision": "approve" | "request_changes" | "replan", "comment"?: string, "by"?: string }`
and the answer the `ApprovalRecord` written. An investigation card takes only
`replan` (it has no gate; convert it instead). The full contract, with every
status, is in [approvals.md](approvals.md#the-api).

## Scripts and app runs

These drive a repository's `.prepcook/scripts.toml` ([scripts.md](scripts.md)).

| Endpoint | What it does |
| --- | --- |
| `GET /api/repos/{repo_id}/scripts` | The repository's parsed manifest, or the error that stopped it, plus `required_secrets_missing`. |
| `POST /api/repos/{repo_id}/scripts/run` | Runs one step in the repository's checkout: `{ "step": "setup" \| "build" \| "start" \| "ready" \| "seed" \| "test" \| "stop" \| "stand-up" }`. Returns `{ "run_id": … }` at once. `start` and `ready` tear the app down once it is ready. |
| `GET /api/script-runs/{run_id}` | That run's state; `report` is set once `finished_at` is. |
| `GET /api/script-runs/{run_id}/raw-logs/ws` | A WebSocket streaming the run's output. |
| `POST /api/app-runs` | `{ "worktree": "<path>" }`: stands the app up in a workspace's worktree for the agent running there, which owns it until its process ends. Refused when an app already runs for that worktree, the path is not in a workspace, or no agent is running there. Returns the run's `status` (URL, port, seed variable names) and the stand-up `report`. |
| `GET /api/app-runs/current?worktree=<path>` | The app running for that worktree, or `null`. |
| `POST /api/app-runs/{run_id}/restart` | Rebuilds and restarts a running app from the worktree's current files; same answer as `POST /api/app-runs`. |

Agents reach the last three through the `app_status` and `app_restart` tools
rather than over HTTP.
