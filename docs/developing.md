# Developing Prepcook

This is the guide for changing Prepcook: how it is put together, where things
live in the repository, how to build and check it, and how to add to it. The
conventions agents and people follow while editing (naming, comments, generated
files) are in [AGENTS.md](../AGENTS.md); how to propose a change is in
[CONTRIBUTING.md](../CONTRIBUTING.md).

## Architecture

Prepcook is two services and the coding agents they start.

```
 browser (packages/local-web)
   │  HTML, /api/*                         │  /v1/* writes, /v1/shape reads
   ▼                                       ▼  (owner token from /api/auth/token)
┌──────────────────────────────┐     ┌──────────────────────────────────────┐
│ prepcook  (crates/server)    │────▶│ board service, Docker (crates/remote) │
│  native, port 4100           │ /v1 │  remote-server  :4110  Axum           │
│  ├ /api: Settings, cards,    │     │  electric        ElectricSQL shapes   │
│  │   approvals, evidence,    │     │  remote-db       Postgres 16          │
│  │   scripts, workspaces     │     └──────────────────────────────────────┘
│  ├ stage orchestrator, ticket│
│  │   pickup, MR monitor      │──── tracker, notification and evidence APIs
│  ├ script runner (app runs)  │
│  └ SQLite, config, secrets   │
└──────────────┬───────────────┘
               │ spawns, per card and stage
               ▼
┌──────────────────────────────┐     ┌──────────────────────────────────────┐
│ agent session (claude, …)    │────▶│ prepcook-mcp (crates/mcp), stdio      │
│  cwd: the card's git worktree│     │  card, app-run and evidence tools,    │
│  --mcp-config, --add-dir     │     │  calling the server's /api            │
└──────────────────────────────┘     └──────────────────────────────────────┘
```

**The native server** (`prepcook`, `crates/server`) serves the built UI,
answers `/api`, and runs the background work: the stage orchestrator
(`services/stage_orchestrator.rs`), which polls the boards and starts each
stage; the ticket pickup (`tracker_watcher.rs`); the merge-request monitor
(`pr_monitor.rs`); the seat monitor and the daily digest. It is the board's
single owner: it holds the board's secret and signs a short-lived owner token
for every request to it ([crates/remote/README.md](../crates/remote/README.md#authentication)).

**The board service** (`crates/remote`, its own Cargo workspace) holds the
board in Postgres and streams it to the UI through ElectricSQL. `prepcook up`
runs it in Docker from the published image, or from this checkout with
`--compose crates/remote/docker-compose.yml`. The UI reads the board through
Electric shapes and writes to the board's `/v1` API directly, with an owner
token the native server mints at `GET /api/auth/token`; everything else goes
through the native server.

### Where state lives

| What | Where |
| --- | --- |
| Boards, columns, cards (with their work class, verdicts, approvals, origin and triage report in `extension_metadata`), comments, tags, pull-request links, the knowledge store and the RCA graph | The board's Postgres (`crates/remote`) |
| Repositories, workspaces, sessions, execution processes and their logs | SQLite, `db.v2.sqlite` in the app data directory (`crates/db`) |
| Settings | `config.json` in the app data directory (`services/config/versions/v10.rs`) |
| Secrets | `~/.config/prepcook/secrets.env`, mode 600 (`services/secrets.rs`, `prepcook secret`) |
| The board's Compose file and generated `.env` | `board/` in the app data directory |
| The agents' MCP file and skills | `agent-mcp.json` and `agent-skills/` in the app data directory |
| Worktrees | `/var/tmp/prepcook/worktrees` on Linux, `$TMPDIR/prepcook/worktrees` on macOS |

The app data directory is `~/.local/share/prepcook` on Linux and
`~/Library/Application Support/prepcook` on macOS for a release
build, and `dev_assets/` in the checkout for a debug build
(`crates/utils/src/assets.rs`).

### What runs in the worktree

Each card's stage runs in a git worktree of the board's repository, on a
branch of its own. Before a Plan or Execute session starts, the orchestrator
stands the app up there from the repository's `.prepcook/scripts.toml`, on
the board's reproduction target ([scripts.md](scripts.md)), and the agent
reaches it through the `app_status` and `app_restart` tools. The agent writes
its reproduction harness under `.repro/<card key>/` (excluded from git through
`.git/info/exclude`) and its RCA or spec as `docs/specs/<card key>.html`; when
a planning verdict lands, the server files that document in the knowledge
store. Review runs in a fresh worktree branched from the implementation
branch, so it never sees the planning files.

### How sessions get MCP servers and skills

Nothing is installed into the agent's own configuration. At startup, and when
the evidence sources change, the server writes `agent-mcp.json` in the app
data directory (`services/agent_mcp.rs`): the `prepcook` server (the
`prepcook-mcp` binary beside `prepcook`) and the MCP server of each evidence
source that has one. Claude Code gets the file with `--mcp-config` and the
skills in `assets/agent-skills/` with `--add-dir`, both for that session only;
Codex gets the `prepcook` server as a thread config override; the other agents
read only their own configuration
([configuration.md](configuration.md#agent-mcp-servers-and-skills)).

## Repository layout

Rust workspace (`Cargo.toml`):

- `crates/server`: the `prepcook` binary: the HTTP API (`src/routes/`), the CLI (`src/cli/`: `up`, `down`, `status`, `secret`, `integrations describe`) and the `generate_types` binary.
- `crates/services`: everything the server does: the orchestrator, the config and its versions, the playbooks, the integration areas (`notifications/`, `tracker/`, `reproduction/`, `evidence/`, `integrations.rs`, `agents.rs`), the script runner, the card API's logic.
- `crates/executors`: the coding agents: one executor per agent, their descriptors, log normalisation, MCP wiring and `default_profiles.json`.
- `crates/mcp`: the `prepcook-mcp` binary, the MCP server agent sessions and the toolkit use.
- `crates/db`: the local SQLite models and migrations (SQLx, offline query data in `.sqlx`).
- `crates/api-types`: types shared by the server and the board service (cards, comments, work classes, triage reports, decisions).
- `crates/git`: git operations; `crates/git-host`: GitHub, GitLab and Azure DevOps (pull requests, comments).
- `crates/worktree-manager`, `crates/workspace-manager`: worktrees, the warm worktree pool, and the workspaces built on them.
- `crates/deployment`, `crates/local-deployment`: the `Deployment` trait the routes are written against, and its local implementation (processes, PTYs, containers).
- `crates/utils`: shared helpers (paths, responses, logs, version).
- `crates/client-info`, `crates/remote-info`: the server's own address and the board's, set once at startup.
- `crates/preview-proxy`: the separate server that serves app previews in the UI.
- `crates/ws-bridge`: WebSocket bridging between the UI and upstream streams.
- `crates/server-info`: an empty manifest, not a workspace member.

Separate workspace: `crates/remote`, the board service (its own
[AGENTS.md](../crates/remote/AGENTS.md) and [README.md](../crates/remote/README.md)).

Frontend (pnpm workspace):

- `packages/local-web`: the app's entry point (Vite, TanStack Router; routes in `src/routes/`). Its `dist/` is embedded in the server at compile time.
- `packages/web-core`: the app's pages, features, Settings sections, dialogs and i18n.
- `packages/ui`: the shared component library (`@prepcook/ui`).
- `packages/public`: static assets served with the app.

Everything else:

- `shared/`: generated TypeScript types (`types.ts`, `remote-types.ts`) and agent tool schemas. Never edited by hand.
- `toolkit/`: the local skills generated from the playbooks, and their generator.
- `assets/`: packaged assets (the agent skills, the brand). `dev_assets_seed/`: what a debug build's `dev_assets/` starts from.
- `npx-cli/`: the `prepcook` npm package, which downloads the release binaries.
- `scripts/`: dev and release helpers, the config-docs generator, the manual-testing helpers (`webhook-echo.mjs`, `fake-deploy.sh`, `fake-destroy.sh`) and the screenshot script.
- `docs/`: the documentation; `docs/images/` the screenshots.

## Build, run and test

You need Rust through `rustup` (`rust-toolchain.toml` picks the version), Node
20 or later and pnpm 10. Docker only for the board.

```sh
pnpm install
pnpm -C packages/local-web build                           # the server embeds the built UI
cargo build --release --bin prepcook --bin prepcook-mcp     # target/release/
```

To work on it:

```sh
pnpm run dev            # server and web app with hot reload, ports chosen for you
```

The server reads the board's address from `PREPCOOK_SHARED_API_BASE`
(`prepcook up` sets it to `http://127.0.0.1:4110`). Start a board first, with
`prepcook up --compose crates/remote/docker-compose.yml` from a release build
or `pnpm run remote:dev`.

Checks, all of which CI runs:

| Command | What it checks |
| --- | --- |
| `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` | The Rust workspace. `pnpm run backend:lint` also builds the `qa-mode` feature. |
| The same with `--manifest-path crates/remote/Cargo.toml` | The board service. |
| `pnpm run generate-types:check`, `pnpm run remote:generate-types:check` | `shared/types.ts` and `shared/remote-types.ts` match the Rust types. Regenerate with `pnpm run generate-types` (and `remote:generate-types`). |
| `pnpm run ui:check`, `web-core:check`, `local-web:check` | TypeScript. |
| `pnpm run local-web:lint`, `ui:lint` | ESLint. |
| `pnpm --filter @prepcook/<package> run format:check` | Prettier, per package. |
| `node scripts/check-unused-i18n-keys.mjs` | Every i18n key is used. |
| `pnpm run local-web:legacy-path-guard` | No import from a retired frontend path. |
| `python3 toolkit/generator/generate.py --check` and `python3 -m unittest discover -s toolkit/generator -v` | The generated skills match the playbooks ([Playbooks and the toolkit](#playbooks-and-the-toolkit)). |
| `cargo deny check licenses` | Dependency licences, both workspaces (`deny.toml`). |

Two more are run by hand before pushing, because they need the binary:
`python3 scripts/gen-config-docs.py --check` (the provider tables in
[configuration.md](configuration.md)) and `pnpm -C packages/local-web build`.
`pnpm run format`, `pnpm run lint` and `pnpm run check` run most of the above
at once.

Both Rust workspaces build with `SQLX_OFFLINE=true` from the committed query
data. After changing a query, `pnpm run prepare-db` (local) or
`pnpm run remote:prepare-db` (board, needs Postgres) refreshes it.

CI is `.github/workflows/test.yml`: jobs `rust`, `licenses`, `frontend`,
`toolkit` and `secrets` (gitleaks), on every pull request and push to `main`.

## The factory pattern

Every integration area is built the same way, so a new provider is a file and
a handful of enum arms, and Settings and the documentation follow from it.

- **One module per area** under `crates/services/src/services/<area>/`:
  `mod.rs` holds the trait, the kind enum, the `build()` function and the
  types every provider answers in; each provider is one file.
- **A trait and an enum of providers.** The trait (`Notifier`, `Tracker`,
  `ReproTarget`, `EvidenceSource`) is dispatched with `enum_dispatch` over an
  enum with one variant per provider (`NotifierProvider`, `TrackerProvider`,
  `ReproTargetKind`, `EvidenceProvider`). The kind enum (`NotifierKind`,
  `TrackerKind`, `ReproductionTarget`, `EvidenceKind`) is the config string.
- **`build()` gives callers nothing to call when nothing is configured**:
  `None` for notifications, ticketing and reproduction (a repository with no
  manifest), an empty set for evidence. A provider missing what it needs is
  left out, and logged, rather than built half-configured.
- **A source that rebuilds on change.** Callers do not build providers; they
  ask a source for the current one (`TrackerSource::current()`,
  `NotificationSource::current(board)`, `EvidenceSources::current(…)`,
  `ReproductionSource::current(…)`). The source resolves the area's config
  section with its secrets, compares it with what it built last, and rebuilds
  only when it differs, so a change in Settings or the secrets store applies
  on the next call without a restart.
- **A descriptor per provider.** Each provider has `fn descriptor() ->
  Descriptor` (`services/integrations.rs`): `kind`, `display_name`, a
  one-sentence `summary`, `fields` (each a `key`, `label`, `kind` of `Text`,
  `Secret`, `Url`, `Select { options }`, `Bool` or `Number`, `required` and
  `help`), the `secrets` it reads (derived from its secret fields), a
  `docs_url`, and `available` (false for a provider that is named but not
  implemented, like Linear). `integrations::descriptors()` collects every
  area's, and the coding agents' as the `agents` area.
- **Secrets through the store.** A secret field's store name is
  `<KIND>_<FIELD>` upper-cased (`secret_name`; a field already named after its
  kind is not prefixed twice). `resolve_secret` reads it from the secrets
  store, then the config, then the environment (`StoreFirst`); a board's own
  entry reads its config value first (`ConfigFirst`).
- **A connection test.** Each provider's `test()` makes one cheap read (or,
  where a service offers nothing to read, sends a test message) and says what
  it found. `POST /api/integrations/{area}/{kind}/test`, with Settings' unsaved
  `fields` in the body, runs it through `integrations::test`.
- **Settings is generated.** `GET /api/integrations/descriptors` feeds one
  generic component, `IntegrationSection`
  (`packages/web-core/src/shared/dialogs/settings/settings/IntegrationSection.tsx`),
  which each area's section (`NotificationsSettingsSection`,
  `TicketingSettingsSection`, `EvidenceSettingsSection`,
  `ReproductionSettingsSection`, `CodingAgentsSettingsSection`) renders with
  its own entries; a new field or provider appears without frontend work.
- **The documentation is generated.** `prepcook integrations describe --json`
  prints the descriptors, and `scripts/gen-config-docs.py` writes the provider
  tables between the `BEGIN GENERATED` and `END GENERATED` markers in
  [configuration.md](configuration.md); `--check` fails when they are stale.
  It runs `cargo run --bin prepcook` unless `PREPCOOK_BIN` names a binary.
- **Config fields are added to `v10.rs` with `#[serde(default)]`.** A new
  config version is only for a field whose meaning changes.
- **Tested against recordings.** Every provider is tested without the
  network: HTTP providers against `wiremock` with the service's recorded
  responses, CLI-driven ones through a recorded runner (`GhRunner` in
  `github_issues.rs`), file and script ones against temporary directories.

### Add a notification provider

Providers live in `crates/services/src/services/notifications/`
([notifications.md](notifications.md#adding-a-provider)).

1. **Create `notifications/<kind>.rs`** with a struct and
   `new(&TargetSettings, …) -> Result<Self, NotifyError>`, reading its fields
   with `settings.get("<key>")` and answering `NotifyError::NotConfigured` for
   a missing required one.
2. **Its `descriptor()`**: `Descriptor::new("<kind>", "<Name>", "<one
   sentence>", vec![Field::new(…)…], "<docs url>")`, with `FieldKind::Secret`
   for credentials (the store name follows from kind and key).
3. **Implement `Notifier`**: `kind()`, `send(&Event)` (start from
   `Event::headline()`, `body()` and `link()`), `conversational()` (`None`
   unless the service can read replies back, then implement `Conversational`
   too), and `test()`.
4. **In `notifications/mod.rs`**: `pub mod <kind>;`, a variant in
   `NotifierKind` (and `ALL`, `as_str`, `descriptor`) and in
   `NotifierProvider`, and its arm in `build_provider`.
5. **Tests** in the file, against `wiremock` (see `webhook.rs` and
   `test_support` in `mod.rs` for the settings and context helpers).
6. **Regenerate**: `pnpm run generate-types` (the kind is a TypeScript union),
   `python3 scripts/gen-config-docs.py`, and a section in
   [notifications.md](notifications.md).

Targets are config entries `{ "kind": "<kind>", "events": […], …fields }` under
`notifications.targets`; no config struct changes.

### Add a ticketing provider

Providers live in `crates/services/src/services/tracker/`.

1. **Config fields.** A tracker's settings are keys of the flat `ticketing`
   section, `TrackerConfig` in `config/versions/v10.rs`: add each new one with
   `#[serde(default)]`, and a variant in `TrackerKind` (with `ALL`, `label`,
   `as_str`, and `mode()` returning `TicketingMode::Tickets`).
2. **Create `tracker/<kind>.rs`** with a struct built from a resolved
   `TrackerConfig`, a `descriptor()` naming its fields by their config keys,
   and an `impl Tracker`: `is_configured`, `issue_url`, `is_valid_key`,
   `test`, `search`/`find`/`fetch`/`get_issue`, `comments`/`post_comment`,
   `transition`/`enter_stage` (map each `TicketStage` through the
   `status_mapping`), `assignee`/`assign_to_self`, `identity`,
   `assignable_users` and the rest. An operation the service has no API for
   answers `TrackerError::Unsupported`.
3. **In `tracker/mod.rs`**: `pub mod <kind>;`, a variant in `TrackerProvider`,
   and arms in `descriptor` and `build_provider`. A secret field also needs its
   slot in `secret_slot`, so `resolve` can fill it from the store.
4. **Settings**: add the new keys to `EMPTY_TICKETING_CONFIG` in
   `TicketingSettingsSection.tsx` (the type check fails until you do).
5. **Tests** against `wiremock` (`jira.rs`'s `fixture_tests`) or a recorded
   runner for a CLI (`Recorded` in `github_issues.rs`).
6. **Regenerate** the types and the config docs.

### Add an incident source

An incident tool is a ticketing provider whose `TrackerKind::mode()` is
`TicketingMode::Incidents`, as PagerDuty is (`tracker/pagerduty.rs`,
[incidents.md](incidents.md)). Follow the ticketing steps, then:

- `mode()` returns `Incidents` for the new kind, and `INCIDENT_KINDS` in
  `TicketingSettingsSection.tsx` lists it, so Settings offers it under
  **Incidents**. `build()` refuses a kind whose mode does not match
  `ticketing.mode`.
- `search`/`find` answer open incidents (the pickup imports each new one as an
  investigation; `origin_for` gives the card an `incident` origin from its
  URL).
- `post_comment` adds a note; `enter_stage` acknowledges on `Planning`, and on
  `Triaged` resolves only when `incidents.resolve_on_triage` is set; `Done`
  resolves.
- It never pages: no escalation, no added responders, no urgency changes.
- Tests cover the stage mapping with and without `resolve_on_triage`
  (`pagerduty.rs` has the pattern).

### Add an evidence source

Providers live in `crates/services/src/services/evidence/`
([evidence.md](evidence.md#adding-a-provider)).

1. **Create `evidence/<kind>.rs`** with a struct built from `SourceSettings`
   and a `descriptor()` whose fields are wrapped by `with_common_fields`
   (`label` first; `areas` and `notes` last).
2. **Implement `EvidenceSource`**: `kind`, `label`, `describe` (the paragraph
   the agent gets in `{{product.evidence_tools}}`), `mcp_server` (an MCP
   server for agent sessions, or `None`), `search` (`Ok(None)` when there is
   no API Prepcook drives as configured) and `test`.
3. **In `evidence/mod.rs`**: `pub mod <kind>;`, a variant in `EvidenceKind`
   (`ALL`, `as_str`, `descriptor`, `display_name`) and in `EvidenceProvider`,
   and its arm in `build_provider`.
4. **Tests** against `wiremock` (`sentry.rs`) or temporary files
   (`log_files.rs`), and a snapshot of `describe()`.
5. **Regenerate** the types and the config docs.

### Add a reproduction target

Targets live in `crates/services/src/services/reproduction/`
([scripts.md](scripts.md)).

1. **Create `reproduction/<kind>.rs`** with a struct over the shared
   `ScriptRunner`, a `descriptor()`, and a `test(&AppRunsConfig)` for its
   global settings.
2. **Implement `ReproTarget`**: `kind`, `try_reserve` (its parallelism
   limit), `stand_up`, `seed`, `test`, `verify`, `run_step`, `tear_down`,
   `restart`, `session_env` (the `PREPCOOK_APP_URL` and seed variables an
   agent gets) and `status`. Every target hands back the same `AppRun`, so the
   harness, the verify and the agent tools see a URL whatever the target.
3. **In `reproduction/mod.rs`**: a variant in `ReproductionTarget` (`ALL`,
   `as_str`) and in `ReproTargetKind`, and arms in `descriptor`, `build`
   (refuse a manifest that does not describe the target, as `remote` does
   without `[app.remote]`), `for_kind` and `test_target`.
4. **The manifest**: a target that needs its own commands gets a table in the
   `.prepcook/scripts.toml` schema (`services/scripts/`) and a section in
   [scripts.md](scripts.md).
5. **Tests** against a temporary worktree with a manifest whose steps are
   shell commands (`remote.rs` and `mod.rs` have the pattern).
6. **Regenerate** the types and the config docs.

### Add a coding agent

An agent is an executor in `crates/executors`.

1. **The executor**, `crates/executors/src/executors/<agent>.rs`: a struct
   implementing `StandardCodingAgentExecutor` (`spawn`, `spawn_follow_up`,
   `normalize_logs`, `default_mcp_config_path`, availability) and its
   `descriptor()` (`AgentDescriptor::new`): display name, summary, the command
   a session starts from, how it signs in, how its sessions get MCP servers
   (`McpWiring`), and its economy variant if it has a cheaper model.
2. **The `CodingAgent` enum** in `executors/mod.rs`: a variant for it. The
   compiler then points at the matches that must name it: `capabilities`, and
   `BaseCodingAgent::descriptor` in `crates/executors/src/descriptor.rs`.
   `get_mcp_config` needs an arm only when its MCP servers live under a key
   other than `mcpServers`.
3. **`crates/executors/default_profiles.json`**: its `DEFAULT` variant, plus
   any variant the descriptor names as its economy variant.
4. **Tests**: log normalisation against recorded output lines (as `claude.rs`
   and `codex/normalize_logs.rs` do); `descriptor.rs`'s tests check every
   agent's descriptor against its profiles.
5. **Regenerate**: `pnpm run generate-types` and
   `python3 scripts/gen-config-docs.py`. **Settings → Agents** and the
   coding-agents table in configuration.md come from the descriptor.

Which agent runs which stage is `agents.stages.<stage>` (`services/agents.rs`,
[configuration.md](configuration.md#agents)).

## Playbooks and the toolkit

The stage prompts are in `crates/services/src/services/config/playbooks.rs`,
keyed `"<work_class>:<stage>"` plus `pattern:planning`, `reviewing`,
`comment_reply` and `rca_analysis`. They name no product or machine:
`{{placeholders}}` and `{{#if}}` blocks are filled when a session is launched,
from the card, the board's repository and its `.prepcook/scripts.toml`
(`services/playbook_context.rs`; the table is in
[playbooks.md](playbooks.md#placeholders)). A saved config keeps the texts it
was saved with; a shipped text a user has not changed is replaced only when
the old text is listed in `config/versions/superseded_playbooks.rs`.

The toolkit's skills (`toolkit/skills/`) are generated from those texts:
`toolkit/generator/generate.py` takes each skill's preamble
(`generator/preambles/`), runs the playbook through the substitution table
(`generator/substitutions.py`, every difference between an orchestrated agent
and a person at a keyboard, with its reason) and cuts it at the skill's anchor.
After any playbook edit, run `python3 toolkit/generator/generate.py` and commit
the result; CI runs `--check` and the table's tests. An improvement someone
makes to a generated skill is promoted back as a playbook change, usually with
a matching substitution rule, so it reaches both the orchestrated agents and
the toolkit on the next generation ([toolkit/README.md](../toolkit/README.md#improving-a-skill)).
`prepcook-self-review` is hand-written and edited in place.

## Extension points: work classes and the card API

**The investigation class** shows how a work class is built, and a new one
follows it. A class is a `WorkClass` variant (`crates/api-types/src/issue_ext.rs`,
with its tag word in `as_str`/`parse` and its place in the tag order); its
stages are playbooks keyed by it (`investigation:planning`); and the
orchestrator decides where it stops. An investigation's planning ends with a
`TRIAGE REPORT` comment, parsed by `parse_triage_report` into a
`TriageReport`, recorded on the card, sent as `investigation_completed`
(`notifications::Event`) and copied to the tracker; the card then moves to the
Triaged column, and `POST /api/cards/{id}/convert` (`services/cards.rs`) turns
it into a bug, carrying the plan over when the RCA is at high confidence and
the harness is still on disk ([method.md](method.md#investigations)).

**The card API** (`POST /api/cards`, `GET /api/cards/{id}`,
`POST /api/cards/{id}/convert`; [api.md](api.md#cards)) is how other services
put work on the board. Its routes are `crates/server/src/routes/cards.rs` and
its logic `services/cards.rs`, behind a small board trait so it is tested
without a board. A filer identifies itself with `origin` (`OriginKind`:
`tammy`, `incident`, `api`); a new kind of filer that the card should name is
a variant there. It is a stable contract: change it additively, and keep
[api.md](api.md) in step.

## Releasing

1. Set the version in `crates/server/Cargo.toml`, `package.json` and
   `npx-cli/package.json`; the workflow refuses a tag that does not match all
   three.
2. Run [manual-testing.md](manual-testing.md) on a release build.
3. Push an annotated tag `v<version>`; its message becomes the release notes.
4. `.github/workflows/release.yml` builds the web app once, then the binaries
   on macOS arm64, macOS x64 and Linux x64, packages each with
   `scripts/package-release.sh` into `prepcook-<version>-<target>.tar.gz`
   (`prepcook`, `prepcook-mcp`, `LICENSE`, `NOTICE`), and publishes them with
   `SHA256SUMS` as a GitHub release. In parallel it builds the board image from
   `crates/remote/Dockerfile` and pushes `ghcr.io/nwisehart/prepcook-board`
   tagged with the version and `latest`.
5. The first time the image is pushed, GitHub makes the package private: set
   its visibility to public in the package's settings, or `prepcook up` cannot
   pull it for anyone not signed in to ghcr.io. `prepcook up` always pulls the
   tag matching its own version, never `latest`.

`pnpm run build:npx` (`local-build.sh`) packages this machine's binaries the
same way, into `npx-cli/dist/`, to try the npm CLI without a release.

## Manual testing

The automated tests cover the pieces; [manual-testing.md](manual-testing.md)
covers the whole, by hand, from a cold clone: the board, a bug with no tracker,
investigations, every integration area, Tammy and the clean-up. Run it before
every release, and the sections a change touches before merging it.
