# Playbooks

A playbook is the prompt an agent starts from when the orchestrator launches it
for a card. There is one per stage and work class, keyed `"<work_class>:<stage>"`
(`bug:planning`, `investigation:planning`, `story:executing`, …), plus four
that are not tied to a work class: `pattern:planning`, `reviewing`,
`comment_reply` and `rca_analysis`.

A card's work class comes from its metadata: an imported ticket gets it from
its issue type, and a card filed through `POST /api/cards` from its request. A
card made on the board gets it from its `bug`, `investigation`, `story`, `epic`
or `subtask` tag (the first in that order, when it has several), read when the
card first enters an automated column and recorded on the card; a card with
neither is not launched.

Two keys were renamed. `pattern:planning` was `investigation:planning`, the
recurring-root-cause review the knowledge graph files; that key now belongs to
investigation cards. `comment_reply` was `triage`. A config saved with the old
keys has them renamed the first time it is read, in the global map and in every
board's overrides ([configuration.md](configuration.md#playbooks)).

The shipped texts are in `crates/services/src/services/config/playbooks.rs`.
They name no product, machine or hosted service: everything that differs between
repositories comes from placeholders, filled when the agent is launched from the
card, the board's repository and the repository's `.prepcook/scripts.toml`.

## Placeholders

| Placeholder | Value |
| --- | --- |
| `{{ticket_key}}` | The card's tracker key, or the card's own id when it has none. `{{jira_key}}` is the same value. |
| `{{title}}`, `{{description}}` | The card's title and description. |
| `{{spec_path}}` | Where the plan document lives in the worktree: the path recorded on the card, else `docs/specs/<ticket_key>.html`. |
| `{{epic_key}}` | The epic a sub-task belongs to; empty otherwise. |
| `{{comments}}` | `comment_reply` only: the new tracker comments that started it. |
| `{{target_branch}}` | The branch work is based on and merged into: the repository's default target branch (else its current branch), as its remote-tracking ref when it has one, e.g. `origin/main`. For `reviewing`, the implementation workspace's target, freshly fetched. |
| `{{bot_name}}` | The author name the agent's commits carry: the repository's `user.name` (repository config, then the user's), else `Prepcook`. |
| `{{host_cli}}` | `glab` for a GitLab remote, `az repos` for Azure DevOps, otherwise `gh`. |
| `{{pr_noun}}` | `merge request` for GitLab, otherwise `pull request`. |
| `{{product.name}}` | `[profile] name`; without one, the repository's name. |
| `{{product.summary}}` | `[profile] summary`; empty without one. |
| `{{product.areas}}` | `[profile] areas`, comma-separated; may be empty. |
| `{{product.error_patterns}}` | `[profile] error_patterns`, comma-separated; may be empty. |
| `{{product.evidence_tools}}` | Generated for the board: one paragraph per evidence source ([evidence.md](evidence.md#what-the-agent-is-told)) saying what it is and how to query it, then `[profile] evidence_tools`, then "If none of these show the failure, the harness is your evidence." Empty when the board has no source and the profile no `evidence_tools`. |
| `{{product.evidence_sources}}` | The board's evidence sources' labels, one `- label` line each; empty without a source. |
| `{{gate_instructions}}` | How a planning agent hands its document to a person; see [The review gate](#the-review-gate). |

The repository is the board's first repository (Settings → Orchestrator →
boards). A board with none gets `this project` as the product name, `main` as
the target branch, `gh` and `pull request`.

A placeholder with no value is left as written. A value is inserted as it is
and not searched for placeholders, except `{{gate_instructions}}`, whose text
uses `{{spec_path}}`.

### Conditional passages

A passage that would read badly with an empty value goes in a block:

```
The product is {{product.name}}.{{#if product.areas}} The areas that matter: {{product.areas}}.{{/if}}
{{#if product.evidence_tools}}Where the failure leaves traces:

{{product.evidence_tools}}{{else}}The harness is your evidence.{{/if}}
```

`{{#if name}}` keeps its body when the value is not blank; `{{else}}` gives the
text used otherwise. Blocks do not nest.

## The product profile

The `[profile]` table of `.prepcook/scripts.toml` (schema in [scripts.md](scripts.md);
a complete example in [examples/scripts.toml](examples/scripts.toml)):

| Key | Used for |
| --- | --- |
| `name` | `{{product.name}}`. |
| `summary` | One paragraph on what the app is and who uses it; `bug:planning` gives it to the agent before it reproduces anything. |
| `areas` | The parts of the product a report or review is likely to name. `bug:planning` lists them, and `reviewing` grounds product findings in them. |
| `error_patterns` | Errors that mean the seed is broken rather than the app. A planning agent that meets one posts it on the card and stops instead of investigating it as the bug. |
| `evidence_tools` | A sentence or two about evidence that the configured sources do not cover — where older logs are kept, who can grant production access. Appended to the sources' paragraphs in `{{product.evidence_tools}}`; it is the appendix, not the whole story. |
| `[[profile.evidence]]` | The repository's own log files, as `log_files` sources ([evidence.md](evidence.md#a-repositorys-own-log-files)): each is described in `{{product.evidence_tools}}` after the board's sources and searched by `evidence_search`. |

The profile is read from the repository's checkout when the agent is launched.
A repository with no `.prepcook/scripts.toml`, or no `[profile]`, gets its own name
and empty values.

## The review gate

Every planning playbook ends by opening a review gate: `{{gate_instructions}}`.
Its text depends on **Settings → Orchestrator → Plan approvals**
(`approvals.in_app_gate` in the config):

- `lavish` (the default): the document carries Approve and Request changes
  buttons that send the reviewer's answer back through Lavish; the agent runs
  `npx lavish-axi <spec_path> --no-open` once and ends its turn.
- `none`: the document has no buttons. The agent ends its turn and the decision
  arrives as a follow-up message from a card comment, a tracker comment or the
  approvals API ([approvals.md](approvals.md)).

Either way, a follow-up containing `APPROVED` makes the agent record
`spec_approved = true`; anything else is feedback it applies before ending its
turn again.

An investigation card has no gate whatever the setting: for it,
`{{gate_instructions}}` says that posting the `TRIAGE REPORT` comment finishes
the card, that Prepcook moves it to Triaged by itself, and that the agent must
not open a review session, set `spec_approved` or plan a fix. Pages that fill
playbooks themselves get that text as `investigation_gate_instructions` from
`GET /api/playbooks/context`.

## The playbooks

Card tools named below (`update_issue`, `add_issue_comment`,
`list_issue_comments`, `record_review`, `app_status`, `app_restart`,
`tracker_add_comment`, `tracker_get_attachments`, `tracker_create_subtasks`,
`kb_*`, `graph_*`) are the Prepcook MCP server's.

The `tracker_*` tools need a ticket. When no tracker is configured, or the card
was made on the board and has none, they do nothing and say so in a successful
result (`"note": "no tracker is configured; the card is the only record —
nothing posted"`), so a playbook step that posts on the ticket does not fail the
session. The playbooks mark those steps "(when a tracker is configured)".

### `bug:planning`

- **Launched** when a bug card enters Plan, after Prepcook has stood the app up in
  the card's worktree ([scripts.md](scripts.md)).
- **Produces** a reproduction harness under `.repro/<ticket_key>/` (never
  committed), a `REPRODUCTION:` comment on the card and, when a tracker is
  configured, a plain-language one on the ticket, and a self-contained HTML root-cause analysis at `{{spec_path}}`
  with its verdict (`planning.kind = "rca"`, open questions, confidence,
  summary, client summary) recorded on the card.
- **Tools**: `app_status`, `app_restart`, `add_issue_comment`,
  `tracker_add_comment`, `update_issue`; the `repro-harness` skill.
- **Placeholders**: `ticket_key`, `title`, `description`, `spec_path`,
  `target_branch`, `pr_noun`, all `product.*`, `gate_instructions`.

### `story:planning`

- **Launched** when a story card enters Plan.
- **Produces** an HTML implementation spec at `{{spec_path}}` and its verdict
  (`planning.kind = "spec"`); the card cannot advance while open questions remain.
- **Tools**: `update_issue`.
- **Placeholders**: `ticket_key`, `title`, `description`, `spec_path`,
  `gate_instructions`.

### `epic:planning`

- **Launched** when an epic card enters Plan.
- **Produces** an HTML epic plan at `{{spec_path}}` with a proposed breakdown
  into independently shippable sub-tasks, and its verdict.
- **Tools**: `update_issue`.
- **Placeholders**: `ticket_key`, `title`, `description`, `spec_path`,
  `gate_instructions`.

### `subtask:planning`

- **Launched** when a sub-task card enters Plan.
- **Produces** an HTML spec for its slice of the epic at `{{spec_path}}`,
  consistent with `docs/specs/{{epic_key}}.html`, and its verdict.
- **Tools**: `update_issue`.
- **Placeholders**: `ticket_key`, `title`, `description`, `epic_key`,
  `spec_path`, `gate_instructions`.

### `investigation:planning`

- **Launched** when an investigation card enters Plan, after Prepcook has
  stood the app up, exactly as for a bug.
- **Produces** what `bug:planning` produces — the harness, the `REPRODUCTION:`
  comments, the RCA at `{{spec_path}}` and its verdict (`planning.kind =
  "rca"`) — without a proposed fix, then a `TRIAGE REPORT` comment on the card:
  `TRIAGE REPORT` alone on the first line, then `## What happens`, `## Where`,
  `## Why`, `## Scope`, `## Workaround` and `## Confidence` (high, medium or low
  first, then one line why). That comment moves the card to Triaged; there is
  no review gate ([method.md](method.md#investigations)).
- **Tools**: those of `bug:planning`.
- **Placeholders**: those of `bug:planning`; `gate_instructions` is the
  no-gate text above.

### `pattern:planning`

- **Launched** instead of the work-class playbook when a card in Plan carries
  a `pattern_node`: the knowledge graph linked enough RCAs to one recurring
  class to open a pattern review ([rca-graph.md](rca-graph.md#pattern-reviews)).
- **Produces** an HTML report at `{{spec_path}}` on why the class recurs and
  what would stop it (`planning.kind = "pattern"`). No code, no spec; a person
  decides what becomes work, and the card never passes the Executing gate.
- **Tools**: `kb_list_documents`, `kb_get_document`, `update_issue`.
- **Placeholders**: `ticket_key`, `title`, `description`, `spec_path`,
  `gate_instructions`.

### `bug:executing`

- **Launched** when a bug card enters Execute, in the planning worktree when it
  still exists, with the app stood up from the fix branch.
- **Produces** the smallest fix for the mechanism the RCA isolated, its
  regression tests, before/after screenshots under `.repro/` for a visible
  change, and commits rebased onto `{{target_branch}}`. It does not push.
- **Tools**: `app_restart`.
- **Placeholders**: `ticket_key`, `title`, `spec_path`, `target_branch`,
  `pr_noun`.

### `story:executing`, `subtask:executing`

- **Launched** when a story or sub-task card enters Execute.
- **Produce** the approved spec, implemented with its test plan, committed and
  rebased onto `{{target_branch}}`. They do not push.
- **Placeholders**: `ticket_key`, `title`, `spec_path`, `target_branch`,
  `pr_noun`; `subtask:executing` also `epic_key`.

### `epic:executing`

- **Launched** when an epic card enters Execute.
- **Produces** the sub-tasks the approved plan proposes, created in one
  `tracker_create_subtasks` call and imported as cards. Without a tracker the
  call creates nothing, and the agent lists the sub-tasks for a person to add.
- **Placeholders**: `ticket_key`, `title`, `spec_path`.

### `reviewing`

- **Launched** when a card enters Review, in a fresh workspace branched from
  the implementation branch, with no planning context. A
  `"<work_class>:reviewing"` entry, when present, is used instead. Never for an
  investigation card, which has nothing to review.
- **Produces** a `record_review` call with the verdict and every finding, and
  one `REVIEW: APPROVE` or `REVIEW: REQUEST CHANGES` comment on the card. It
  reads only the `REPRODUCTION:` and `REVIEW-REJECTED:` comments.
- **Tools**: `list_issue_comments`, `record_review`, `add_issue_comment`.
- **Placeholders**: `ticket_key`, `title`, `description`, `target_branch`,
  `pr_noun`, `product.areas`.

### `comment_reply`

- **Launched** when a person comments on the ticket of a card that has already
  shipped (only under autonomy Full). A board overrides it under the same key.
- **Produces** one plain-language answer on the ticket and a
  `TRIAGE: <a|b|c|d>` comment on the card. It changes no code and no status.
- **Tools**: `tracker_get_attachments`, `kb_list_documents`,
  `kb_get_document`, `tracker_add_comment`, `add_issue_comment`; `{{host_cli}}`
  to read what shipped.
- **Placeholders**: `ticket_key`, `title`, `description`, `comments`,
  `host_cli`, `pr_noun`.

### `rca_analysis`

- **Launched** from the board menu, and by the orchestrator when approved RCAs
  are waiting to be linked into the knowledge graph.
- **Produces** graph links and process tags for each unlinked RCA, and a
  summary that names any node linked to five or more documents as a
  pattern-review candidate.
- **Tools**: `kb_list_documents`, `kb_get_document`, `kb_tag_document`,
  `graph_nodes`, `graph_link`, `graph_create_node`.
- **Placeholders**: `bot_name` (to tell the orchestrator's own commits from a
  person's).

## Changing a playbook

**Settings → Playbooks** edits the texts. The **Scope** selector (shown once a
board is configured under Settings → Orchestrator) picks what is edited:

- **Global**: the texts every board uses (`playbooks` in the config). **Reset
  to default** restores the shipped text.
- **A board**: that board's overrides
  (`orchestrator.project_defaults[<board id>].playbooks`). A stage the board
  has not overridden shows the global text; editing it creates the override,
  and **Use global** removes it. An empty override also falls back to the
  global text.

Placeholders work the same in an edited text.

A saved config keeps every playbook text it was saved with. When Prepcook ships a
new stage, its key is added to the saved config on the next start; a text the
user has not changed is replaced only when the old text is listed in
`SUPERSEDED_PLAYBOOKS` in `config/versions/v10.rs`.

Agents started from the board by hand (with the orchestrator off) are filled
the same way: the page reads the board's values from
`GET /api/playbooks/context?project_id=<board id>`.

## On your own machine

The [toolkit](../toolkit/README.md) turns the `bug:planning`, `bug:executing`
and `reviewing` playbooks into Claude Code skills (`/prepcook-repro`, `/prepcook-rca`,
`/prepcook-fix`, `/prepcook-review`, `/prepcook-mr`, plus a hand-written
`/prepcook-self-review`) that run the same loop on an engineer's laptop, against the
app they stand up from the same `.prepcook/scripts.toml`, filing RCAs in the same
knowledge store through `prepcook-mcp`. They are generated from the texts in
`playbooks.rs` through a substitution table that records every difference
between an orchestrated agent and a person at a keyboard, so a playbook edit
reaches them on the next `python3 toolkit/generator/generate.py`; CI fails
when it has not been run.
