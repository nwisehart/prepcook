# Configuration

Every setting Prep Cook keeps, grouped the way **Settings** groups them, with its
default. Settings are stored in `config.json` in the app data directory
(`~/.local/share/prepcook` on Linux, `~/Library/Application Support/ai.bloop.prepcook`
on macOS), currently in its `v10` shape. The Settings pages write that file; a
setting marked *file only* has no control on a page and is changed by editing
the file while Prep Cook is stopped. The orchestrator re-reads its settings on every
poll, so a change made in Settings applies without a restart.

Defaults below are what a new config starts with. "Unset" means the key is
`null` or absent, and the behaviour described applies.

## Orchestrator

**Settings → Orchestrator.** Global values; most can be overridden per board
([below](#per-board-overrides)).

| Key | Default | What it does |
| --- | --- | --- |
| `orchestrator.mode` | `off` | **Enable automatic agent launches**. `auto` scans the boards and runs the stages; `off` runs nothing on its own. |
| `orchestrator.poll_interval_secs` | `15` | **Poll interval**: seconds between scans. |
| `orchestrator.max_launches_per_tick` | `3` | **Max launches per tick**: new agent sessions started per scan, highest priority first. |
| `orchestrator.autonomy` | `off` | **Autonomy mode**. `full` is Prep mode: passing plans self-approve, verified fixes go to Review, approved reviews go to Push MR. `off` is Chef mode. |
| `orchestrator.autonomy_review_iterations` | `3` | **Review-fix rounds before escalating**: fix rounds before a card that keeps getting REQUEST CHANGES waits for a person. Applies in both modes. |
| `orchestrator.pattern_threshold` | `5` | **Pattern-review threshold**: linked RCAs on one graph node that file a pattern review ([rca-graph.md](rca-graph.md#pattern-reviews)). Older configs call it `investigation_threshold`, which is still read. |
| `orchestrator.default_board` | unset (the first board in `project_defaults`) | *File only.* The board `POST /api/cards` files into when the request names none ([api.md](api.md#file-a-card)). |
| `orchestrator.deployed_retention_days` | unset (off) | **Auto clean-up after**: days a card may sit in Close before it moves to Clean up. |
| `orchestrator.cleanup_retention_days` | unset (keep) | **Delete after**: days a card may sit in Clean up before it is deleted. Cards with a pinned workspace or a takeover are kept. |
| `orchestrator.workspace_retention_days` | `30` | **Workspace retention**: days after clean-up before a workspace's records, sessions and logs are deleted. Unset keeps them. |
| `orchestrator.digest_at_local` | unset (off) | *File only.* Local `HH:MM` at which the engineers' digest is sent to each board's [notification targets](#notifications). |
| `orchestrator.eob_at_local` | unset (off) | *File only.* Local `HH:MM` at which each in-flight ticket gets its daily update on the tracker ([working-alongside-people.md](working-alongside-people.md#the-daily-update)). |
| `orchestrator.daily_timezone` | unset (`America/Los_Angeles`) | *File only.* IANA zone the two times above are read in. |
| `orchestrator.project_defaults` | empty | **Automated projects**: the boards the orchestrator scans, keyed by board id, each with its repositories and overrides. A board needs at least one repository. |
| `orchestrator.seat_monitor` | `false` | *File only.* Probe the Claude subscription seat with a headless `claude -p` every six hours and notify the global [notification targets](#notifications) when its login has expired, for a shared runner. Off, Prep Cook never runs `claude` on its own; **Settings → Claude → Re-check now** still probes on demand. |
| `executor_profile` | Claude Code, default variant | **Settings → General → Default Coding Agent**: the agent every stage runs. |

## Tracker

**Settings → Tracker.** Without credentials no tracker is configured: every
card is made on the board, the ticket pickup does not poll, and nothing is
written to a ticket ([method.md](method.md#without-a-tracker)).

| Key | Default | What it does |
| --- | --- | --- |
| `tracker.kind` | `jira` | **Tracker**. `github_issues` and `linear` can be named but are not implemented yet; every call to them says so. |
| `tracker.site` | unset (`example.atlassian.net`) | **Site**: the Jira Cloud host. |
| `tracker.email` | unset | **Email** of the agent's tracker account. With the token, what makes the tracker configured. |
| `tracker.api_token` | unset | **API token** for that account. |
| `tracker.bot_identity` | unset (the token's account) | **Agent account id** the agent's own comments are recognised by. |
| `tracker.project_key` | unset (`BUG`) | **Project key**. |
| `tracker.import_filter` | unset (open-sprint `To Do` tickets assigned to you, by priority) | **Import filter**: the query the import dialog runs. `{project_key}` is substituted. |
| `tracker.enabled` | `false` | **Pick up new tickets automatically** for the board below. |
| `tracker.project_id` | unset | **Import into project**: the board picked-up tickets land on. |
| `tracker.pickup_filter` | unset (`project = {project_key} AND priority in (P0, P1) AND issuetype = Bug`) | **Pickup filter**: which new tickets are picked up. |
| `tracker.assigned_filter` | unset (off) | **Assigned pickup filter**: tickets assigned to the agent's account, imported and started whatever their priority. |
| `tracker.poll_secs` | `45` | **Poll interval** of the ticket pickup. |
| `tracker.status_mapping.planning` | unset (`In Progress`) | **Stage statuses**: where a ticket moves when Plan starts. Plan also assigns the ticket to the agent. |
| `tracker.status_mapping.executing`, `.reviewing`, `.pushed_mr`, `.done` | unset (no move) | Where a ticket moves when that stage starts; `done` is Clean up. |
| `tracker.status_mapping.triaged` | unset (no move) | **Triaged**: where an investigation's ticket moves when its card reaches Triaged ([method.md](method.md#investigations)). |
| `tracker.status_mapping.shipped` | `Merged`, `Deployed to Production`, `Done` | **Shipped statuses**: names that mean the work shipped, for reopen detection and the clean-up sweeps. |
| `tracker.blocker_priorities` | `P0`, `P1` | **Blocker priorities**: their merge requests are labelled `Blocker` and announced to the notification targets. |
| `tracker.approvers` | empty (nobody) | **Plan approvers**: who may decide a plan from a ticket comment. |
| `tracker.type_mapping` | `Bug`→bug, `Story`→story, `Task`→story, `Small Enhancement Request`→story, `Epic`→epic, `Sub-task`→subtask | *File only.* Issue type to work class (`bug`, `investigation`, `story`, `epic`, `subtask`); unmapped types are stories. Map a type such as `Support Question` to `investigation` to have its tickets reproduced and triaged rather than fixed. |

## Git host and integrations

**Settings → General** (Git, Pull Requests, Commits, GitHub Integration),
**Settings → Integrations** and **Settings → Worktree pool**.

| Key | Default | What it does |
| --- | --- | --- |
| `git_branch_prefix` | `prepcook` | **Branch Prefix** of every branch an agent creates. |
| `workspace_dir` | unset (the default location) | **Workspace Directory** worktrees are created in. |
| `pr_auto_description_enabled` | `true` | **Auto-generate PR description**: after Push MR opens a pull request, the implementing agent writes its title and description. |
| `pr_auto_description_prompt` | unset (the shipped prompt) | The prompt for that follow-up. |
| `commit_reminder_enabled` | `true` | **Commit reminder**: an agent that ends a turn with uncommitted changes is told to commit. |
| `commit_reminder_prompt` | unset (the shipped prompt) | The reminder's text. |
| `github.pat`, `github.oauth_token`, `github.username`, `github.primary_email`, `github.default_pr_base` | unset | **GitHub Integration**. Pull requests are opened with the `gh` CLI, signed in on this machine. |
| `integrations.auto_assign_reviewers` | `true` | **Auto-assign reviewers** from blame on the lines a merge request changes. |
| `integrations.reviewer_map` | empty | **Reviewer map**: git author email to host login. Unmapped authors are named in a comment instead of assigned. |
| `integrations.reviewer_exclude_emails` | empty | **Excluded author emails**, on top of the agent's own. |
| `integrations.bot_logins` | empty | **Ignored comment authors**: merge-request comments from these logins are not fed to the agent. |
| `integrations.sentry_access_token` | unset (`SENTRY_ACCESS_TOKEN`) | **Sentry → Access token** for the agent's Sentry MCP server, which agent sessions get only while a token is set here, in the environment or in the secrets store. |
| `integrations.sentry_host` | unset (sentry.io) | **Host** of a self-hosted Sentry. |
| `worktree_pool.enabled` | `false` | **Enable worktree pool**: reuse warm worktrees instead of creating one per workspace. |
| `worktree_pool.max_trees` | `8` | **Max worktrees per repository**. |
| `worktree_pool.root` | unset (the default location) | **Pool root directory**. |

## Notifications

**Settings → Integrations → Notifications.** Where orchestrator events go:
escalations, the daily digest, seat alerts, blocker merge requests and the
rest ([notifications.md](notifications.md) lists every event). Any number of
targets at once; each gets every event unless it names some.

| Key | Default | What it does |
| --- | --- | --- |
| `notifications.targets` | empty (none) | The targets, each `{ "kind": …, "events": […], …fields }`. `kind` is one of the providers below; the fields are that provider's. |
| `notifications.targets[].events` | unset (every event) | Event kinds this target receives; `["*"]` is every event too. |

A config written with the older `slack_webhook_url`, `slack_bot_token` and
`slack_channel` keys, or a board's `slack` override, is converted into Slack
targets the first time it is read.

<!-- BEGIN GENERATED: integrations (scripts/gen-config-docs.py) -->

### Notification providers

Generated from the providers' descriptors. A secret field is read from the secrets store under the name shown, then from the config, then from the environment variable of that name; a board's own target reads its config value first.

| `kind` | Field | Required | Secret name | What it is |
| --- | --- | --- | --- | --- |
| `slack` (Slack) | | | | Posts to a channel through an incoming webhook, a bot, or both; with a bot, escalation threads carry replies back to the agent and the digest keeps a channel canvas. |
| | `webhook_url` | no | `SLACK_WEBHOOK_URL` | **Incoming webhook URL**. A Slack incoming-webhook URL (https://hooks.slack.com/services/…). Needed unless a bot token and channel are set. |
| | `bot_token` | no | `SLACK_BOT_TOKEN` | **Bot token**. A bot token (xoxb-…) with chat:write, channels:history (or groups:history) and canvases:write; the bot must be in the channel. With a channel, escalations open a thread whose replies reach the blocked agent. |
| | `channel` | no |  | **Channel ID**. The channel the bot posts to, e.g. C0123456. |
| `webhook` (Webhook) | | | | POSTs every event as signed JSON to a URL you run, for wiring Prep Cook into anything else. |
| | `url` | yes |  | **URL**. Where events are POSTed. A 5xx or timeout is retried three times. |
| | `secret` | no | `WEBHOOK_SECRET` | **Signing secret**. Signs each body with HMAC-SHA256 in the X-PrepCook-Signature header. Unset sends unsigned requests. |
| `discord` (Discord) | | | | Posts each event as an embed to a Discord channel through its webhook. |
| | `webhook_url` | yes | `DISCORD_WEBHOOK_URL` | **Webhook URL**. The channel's webhook (Channel settings → Integrations → Webhooks), https://discord.com/api/webhooks/…. |
| `teams` (Microsoft Teams) | | | | Posts each event as an Adaptive Card to a Teams channel through an incoming webhook. |
| | `webhook_url` | yes | `TEAMS_WEBHOOK_URL` | **Webhook URL**. The URL of a Teams workflow that posts webhook requests to a channel, or of a channel's Incoming Webhook connector. |

<!-- END GENERATED: integrations -->

## Agent MCP servers and skills

Not settings, but where they end up. The agent sessions Prep Cook launches get
the `prepcook` MCP server (the `prepcook-mcp` binary beside the server, or in
`~/.local/bin` or `/usr/local/bin`) and, with a Sentry token, the Sentry one.
Prep Cook writes them to `agent-mcp.json` in the app data directory at startup
and when the Sentry settings change, and hands that file to each session:
Claude Code through `--mcp-config`, which adds the servers to the user's own
for that session only, and Codex as a thread config override (`prepcook`
only). The other agents have no per-session mechanism and get the servers only
from their own config.

The skills in `assets/agent-skills/` reach Claude Code sessions the same way:
Prep Cook writes them under `agent-skills/.claude/skills/` in the app data
directory, refreshing them as each session starts, and passes that directory
with `--add-dir`, which loads its skills for that session only. Nothing is
written into the worktree.

Prep Cook never writes to an agent's own config (Claude Code's user config and
`~/.claude/skills`, `~/.codex/config.toml` and the like) on its own, so
sessions outside Prep Cook load only what their user installed. **Settings →
MCP Servers** edits that config when you save there, and the toolkit's
`setup.sh` registers `prepcook` in Claude Code's user config and links the
`prepcook-*` skills into `~/.claude/skills` when you run it
([toolkit/README.md](../toolkit/README.md#setupsh)); nothing else does.

## App runs

*File only.* How the script runner stands apps up ([scripts.md](scripts.md)).

| Key | Default | What it does |
| --- | --- | --- |
| `app_runs.port_range` | `4200-4299` | Ports handed to app runs, inclusive. |
| `app_runs.max_parallel` | `2` | Apps up at once. A stand-up past this waits for the next scan. |

## Board

Not part of `config.json`: the board is configured from the environment, and
the board stack reads its own `.env` (`<app data dir>/board/.env`, written by
`prepcook up`). There is no sign-in: the server signs every request to the
board with the board's secret.

| Variable | Default | What it does |
| --- | --- | --- |
| `PREPCOOK_SHARED_API_BASE` | `http://127.0.0.1:4110` under `prepcook up`, else unset | The board's URL. Unset, the board features are off. |
| `PREPCOOK_BOARD_SECRET` (secret) | the board `.env`'s `PREPCOOK_JWT_SECRET` | The board's JWT secret, which the server signs its owner tokens with. `prepcook up` reads it from the `.env` of the board it starts. A server started by hand reads the environment variable, then `PREPCOOK_BOARD_SECRET` in the secrets store, then (for the board on `127.0.0.1:4110`) the board `.env`. Without one the server still starts, and the UI says there is no board secret. |
| `PREPCOOK_JWT_SECRET` (board `.env`) | generated | Standard base64 of at least 32 bytes; signs agent sessions' tokens and verifies owner tokens. |
| `DB_PASSWORD`, `ELECTRIC_ROLE_PASSWORD` (board `.env`) | generated | The board's database passwords. |

The board reads no sign-in or OAuth settings: `SELF_HOST_LOCAL_AUTH_EMAIL`,
`SELF_HOST_LOCAL_AUTH_PASSWORD` and `GITHUB_`/`GOOGLE_OAUTH_CLIENT_ID` and
`_SECRET` lines in a board `.env` are ignored.

## Approvals

| Key | Default | What it does |
| --- | --- | --- |
| `approvals.in_app_gate` | `lavish` | **Settings → Orchestrator → Plan approvals**. `lavish` puts Approve and Request changes buttons in the plan document; `none` leaves them out and the decision comes from the card, the ticket or the API ([approvals.md](approvals.md)). |
| `tracker.approvers` | empty | See [Tracker](#tracker). |
| `PREPCOOK_API_TOKEN` (secret) | unset (bearer access off) | The bearer token the approvals and card APIs accept from scripts and other services ([api.md](api.md)). Filing a card needs it. |

## Secrets

**Settings → Secrets**, or `prepcook secret set|list|unset`. Not part of
`config.json`: values live in `~/.config/prepcook/secrets.env` (mode `0600`).
`PREPCOOK_SECRETS_FILE` points Prep Cook at another file, and an environment variable
`PREPCOOK_SECRET_<NAME>` overrides an entry. A repository's `[secrets] requires`
names the ones its scripts need ([scripts.md](scripts.md#secrets)).

## Playbooks

| Key | Default | What it does |
| --- | --- | --- |
| `playbooks` | the shipped texts | **Settings → Playbooks**, scope Global: the prompt each stage's agent starts from, keyed `<work_class>:<stage>` (`investigation:planning` among them) plus `pattern:planning`, `reviewing`, `comment_reply` and `rca_analysis` ([playbooks.md](playbooks.md)). |
| `playbook_keys` | `1` | *Kept by Prep Cook.* Which key names `playbooks` uses. A config without it was saved before `investigation:planning` (the pattern review's text) became `pattern:planning` and `triage` became `comment_reply`; both are renamed, globally and in every board's overrides, the first time it is read. |

## Per-board overrides

Under `orchestrator.project_defaults["<board id>"]`, shown per board in
**Settings → Orchestrator → Automated projects → Board overrides**. Unset or
empty inherits the global value.

| Key | Default | What it does |
| --- | --- | --- |
| `repo_ids` | empty | **Repositories** checked out for this board's workspaces. Required. |
| `label` | unset | **Board label**: tagged on its RCAs as `board:<label>`. |
| `executor_profile` | unset (inherit) | *File only.* The coding agent for this board. |
| `autonomy` | unset (inherit) | Autonomy for this board. |
| `autonomy_review_iterations` | unset (inherit) | Review-fix rounds for this board. |
| `pattern_threshold` | unset (inherit) | Pattern-review threshold for this board (`investigation_threshold` in older configs). |
| `deployed_retention` | `inherit` | `off`, or `{ "days": n }`: days in Close before Clean up. |
| `cleanup_retention` | `inherit` | `off`, or `{ "days": n }`: days in Clean up before deletion. |
| `tracker.watcher_enabled` | unset (on only for `tracker.project_id`) | **Ticket pickup** for this board. |
| `tracker.jql_filter` | unset (inherit) | **Pickup filter** for this board. |
| `tracker.assigned_jql` | unset (inherit) | **Assigned pickup filter** for this board; blank inherits. |
| `tracker.project_key` | unset (inherit) | **Tracker project key**, when this board's tickets live elsewhere. |
| `notifications.enabled` | unset (on) | **Notifications**: `false` silences every target for this board. |
| `notifications.skip_global` | empty | Kinds of global target (`slack`, `webhook`, …) this board does not send to. |
| `notifications.targets` | empty | Targets only this board sends to, on top of the global ones it keeps; same shape as `notifications.targets`. |
| `merge_requests.auto_assign_reviewers` | unset (inherit) | **Assign reviewers from blame**. |
| `merge_requests.reviewer_map` | empty | **Reviewer map additions**, merged over the global map. |
| `merge_requests.reviewer_exclude_emails` | empty | **Exclude authors**, added to the global list. |
| `merge_requests.bot_logins` | empty | **Ignore MR comments from**, added to the global list. |
| `deploy_watch` | unset (off) | **Staging deploy job** and **branch**: records when each merged change reaches staging. Both are needed. |
| `playbooks` | empty | **Settings → Playbooks**, scope this board: texts for this board only. |

## Everything else

**Settings → General**, and state Prep Cook keeps for itself.

| Key | Default | What it does |
| --- | --- | --- |
| `theme` | `SYSTEM` | **Theme**. |
| `language` | `BROWSER` | **Language**. |
| `editor.editor_type` | `VS_CODE` | **Editor Type** that "open in editor" uses; `custom_command`, `remote_ssh_host` and `remote_ssh_user` are unset. |
| `notifications.sound_enabled`, `.push_enabled` | `true`, `true` | **Sound** and **Push Notifications**. |
| `notifications.sound_file` | `COW_MOOING` | **Sound**. |
| `send_message_shortcut` | `ModifierEnter` | **Send message with**. |
| `analytics_enabled` | `true` | **Enable Telemetry**. |
| `disclaimer_acknowledged`, `onboarding_acknowledged`, `remote_onboarding_acknowledged` | `false` | Whether the first-run screens were seen. |
| `showcases`, `last_app_version`, `show_release_notes` | empty, unset, `false` | What the app has already shown you. |
| `config_version` | `v10` | The shape of the file. An older file is upgraded when read. |
