# Evidence

A reproduction harness shows that a failure happens; where it ran for real
there is usually more: an error tracker grouping the exceptions, a log service
holding the request that failed, the log file the app wrote while the harness
drove it. Prepcook calls each of these places an **evidence source**. Every
configured source

- gets a paragraph in the planning playbooks (`{{product.evidence_tools}}`),
  telling the agent what it is, how to query it and what to look for;
- may give agent sessions an MCP server (Sentry's, today);
- can be searched on the agent's behalf with the `evidence_search` tool, which
  merges what every source found into one list, newest first.

Sources live in the configuration, several at once, globally and per board; a
repository can also name the log files it writes in its own
`.prepcook/scripts.toml`.

## Configuring sources

**Settings → Integrations → Evidence**, or `evidence.sources` in `config.json`
([configuration.md](configuration.md#evidence) has every provider's fields):

```json
{
  "evidence": {
    "sources": [
      { "kind": "sentry", "org": "acme", "project": "pantry-web", "environment": "production" },
      { "kind": "datadog", "label": "Web logs", "site": "datadoghq.eu", "query": "service:pantry-web" },
      { "kind": "cloudwatch", "label": "Worker logs", "log_group": "/ecs/pantry-worker", "region": "eu-west-1" }
    ]
  }
}
```

Every source takes three optional fields besides its provider's own:

- `label` — what the agent and `evidence_search` call it. Unset is the
  provider's name ("Sentry", "Log files"); give two sources of one kind their
  own labels.
- `areas` — comma-separated `[profile] areas` the source covers. A search
  narrowed to an area skips sources that name other areas; unset covers every
  area.
- `notes` — a sentence or two appended to the source's paragraph: what is
  logged there, how long it is kept, which fields matter.

Secret fields (tokens and keys) are best kept out of the file: Prepcook reads
them from the secrets store first, under `<KIND>_<FIELD>`
(`SENTRY_ACCESS_TOKEN`, `DATASET_READ_TOKEN`, `DATADOG_API_KEY`,
`DATADOG_APPLICATION_KEY`), then from the config, then from the environment
variable of that name. A source missing a setting it needs is left out of the
playbooks and of searches, and the server logs why.

A board changes its sources under
`orchestrator.project_defaults["<board id>"].evidence`, the way it changes its
notification routing: `enabled: false` turns every source off for it (its
agents are told the harness is their only evidence), `skip_global:
["sentry"]` drops the global sources of a kind, and `sources` adds sources
only that board uses. Skipping a kind and adding a source of that kind
replaces the global one. A board's own source reads its configured secret
before the store, so two boards can read two Sentry organizations.

### A repository's own log files

Log file paths are a fact of the repository, so its manifest can name them
under `[profile]` (see [scripts.md](scripts.md)):

```toml
[[profile.evidence]]
kind = "log_files"
label = "Pantry app log"
paths = ["logs/app.log"]
notes = "One JSON line per request (method, path, status, duration, user id) and one per error, with the stack trace."
```

Only `log_files` entries are allowed there, and no secret field: services with
accounts and credentials belong in Settings. The repository's sources come
after the board's in every list.

### Upgrading from the Sentry settings

`integrations.sentry_access_token` and `integrations.sentry_host`, where older
configs kept the Sentry connector, become one `sentry` source the first time
the config is read (when it has none yet), and are dropped on the next save. A
token kept only in the environment or the secrets store, with neither field
set, no longer adds the Sentry MCP server by itself: add a Sentry source,
whose token is then read from there. A v9 config's Scalyr settings
(`scalyr_read_token`, `scalyr_server`) become a `dataset` source.

## Providers

| `kind` | Reads | `evidence_search` | MCP server |
| --- | --- | --- | --- |
| `sentry` | Sentry's REST API, `GET /api/0/projects/{org}/{project}/issues/` | yes, with `org` and `project` set | `@sentry/mcp-server`, with the token baked in |
| `dataset` | DataSet (Scalyr) query API, `POST /api/query` | yes | no |
| `datadog` | Datadog Logs Search API v2, `POST /api/v2/logs/events/search` | yes | no |
| `cloudwatch` | `FilterLogEvents`, through the `aws` CLI on this machine | yes | no |
| `log_files` | the files themselves, on this machine | yes | no |

The pattern a search is given is matched as a literal phrase everywhere:
quoted for Sentry, DataSet, Datadog and CloudWatch (where it is
case-sensitive), and as a case-insensitive substring in log files.

### Sentry

Needs `access_token` (a user auth token with `org:read`, `project:read` and
`event:read`); `host` for a self-hosted Sentry. With `org` and `project`,
`search` lists the project's issues whose last event falls in the window and
that match the pattern, each as a hit with its title and culprit, `short_id`,
`level`, `count`, `user_count`, `first_seen` and `status`, linked to the issue;
`environment` narrows them to one environment. Without `org` and `project` the
agent has the MCP server only.

`describe()` tells the agent what Sentry holds (issues with stack traces,
tags and the release each was first seen in), that the Sentry MCP server reads
an issue in full, and to look for an issue first seen around the report and
cite its short id in the RCA.

The MCP server is written into the agent sessions' MCP file (see
[configuration.md](configuration.md#agent-mcp-servers-and-skills)) for every
Sentry source, global or a board's; a second source with different settings
gets `sentry-2`, and so on.

### DataSet

Needs `read_token` (a "Read Logs" token); `server` for another region
(`https://app.eu.scalyr.com`); `filter`, a DataSet filter every search is
narrowed by. `search` runs a log query over the window, newest first, and each
hit carries the event's message, `severity`, `thread` and attributes, linked to
the same query in DataSet.

### Datadog

Needs `api_key` and `application_key` (with `logs_read_data`); `site` for
another Datadog site (`datadoghq.eu`, `us5.datadoghq.com`); `query`, search
terms every search is narrowed by (`service:web env:production`). Each hit
carries the log's message, `id`, `service`, `host`, `status` and attributes,
linked to the same search in Datadog's log explorer.

### CloudWatch Logs

Needs `log_group`; `region` and `profile` choose the AWS CLI's region and
profile. Prepcook drives the `aws` CLI rather than linking the AWS SDK, the
way the GitHub Issues tracker drives `gh`: the CLI's own configuration (a
profile, SSO, an instance role) supplies the credentials, so there is nothing
to enter, and the SDK's dependency tree stays out of the build. The CLI must be
installed and signed in on the machine Prepcook runs on. Each hit carries the
event's message, `log_stream` and `event_id`, linked to its log stream in the
console when the region is set. Test connection looks the log group up.

### Log files

Needs `paths`: files or globs (`logs/*.log`, `logs/**/*.log`), relative to the
agent's worktree or absolute. `search` reads the last 32 MB of each matching
file and returns the lines containing the pattern, ignoring case. A line whose
timestamp falls outside the window is left out and a line without one is
kept; the timestamp is a JSON line's `time` (or `timestamp`, `ts`,
`@timestamp`, `date`, `datetime`, `t`) field, else an RFC 3339 or
`YYYY-MM-DD HH:MM:SS` time at the start of the line, read as UTC without a
zone. Each hit carries `file` and `line`, newest (last written) first.

`describe()` gives the agent the paths, says it can read them itself or search
them, and to look for the failing request and the error logged after it. For
the Pantry example ([examples/scripts.toml](examples/scripts.toml)) the
paragraph is:

> Pantry app log (Log files): `logs/app.log`, relative to the worktree. Read it
> with tail, grep or jq, or call `evidence_search` with source "Pantry app
> log": it returns the lines that contain the pattern, ignoring case, leaving
> out a line whose timestamp falls outside the window and keeping one that has
> none. Look for the failing request and the error logged after it, around the
> time you ran the harness. One JSON line per request (method, path, status,
> duration, user id) and one per error, with the stack trace.

## What the agent is told

`{{product.evidence_tools}}` is generated for the board the card is on: one
paragraph per source from its `describe()`, in configuration order (global,
then the board's, then the repository's); then the manifest's
`[profile] evidence_tools`, when set, as an appendix for whatever the sources
do not say; then "If none of these show the failure, the harness is your
evidence." With no source and no appendix it is empty, and the playbooks say
instead that the harness is the only evidence.
`{{product.evidence_sources}}` is the sources' labels as a bullet list; the
investigation playbook asks for them in the triage report's `## Where`.
[playbooks.md](playbooks.md) lists both placeholders.

## `evidence_search`

An MCP tool of `prepcook-mcp` in agent and global mode:

```
evidence_search {
  pattern: string,        // literal phrase: an error message, an id, a route
  since?: "2h" | ISO,     // a span back from now (s, m, h, d, w) or a time; default 24h before until
  until?: "2h" | ISO,     // default now
  source?: string,        // one source, by label or kind; default every source
  limit?: number,         // 1 to 200; default 50
  area?: string,          // a [profile] area; skips sources that cover others
  project_id?: uuid       // the board; default the workspace's board
}
```

It posts to `POST /api/evidence/search` (see [api.md](api.md#evidence)) with
the agent's worktree, so `log_files` paths resolve in it, and returns:

```json
{
  "hits": [
    {
      "at": "2026-10-18T10:00:00Z",
      "source": "Pantry app log",
      "message": "{\"time\":\"2026-10-18T10:00:00.000Z\",\"level\":\"error\",\"error\":\"SeedMissing\",…}",
      "link": null,
      "fields": { "file": "logs/app.log", "line": 214 }
    }
  ],
  "truncated": false,
  "sources": [
    { "label": "Pantry app log", "kind": "log_files", "searched": true, "hits": 1,
      "truncated": false, "error": null, "note": null }
  ]
}
```

Hits are newest first, hits without a time after every dated one, at most
`limit` across all sources; `truncated` says more matched than came back. A
source that fails is reported in `sources` with its error and never fails the
search; naming a source that does not exist does, with the configured labels
in the message. Messages are clipped to 2,000 characters and pass through the
secrets redaction every log line does.

## Adding a provider

1. A file in `crates/services/src/services/evidence/` with a struct that
   implements `EvidenceSource` (`kind`, `label`, `describe`, `mcp_server`,
   `search`, `test`) and a `descriptor()` whose fields start and end with
   `with_common_fields`. `search` returns `Ok(None)` when the provider has no
   API Prepcook drives as configured.
2. A variant in `EvidenceKind` (its config string, display name and
   descriptor) and in `EvidenceProvider`, and its arm in `build_provider`.
3. Tests against recorded responses (`wiremock`, or a recorded runner for a
   CLI), and a `describe()` snapshot.
4. `python3 scripts/gen-config-docs.py` to add it to the reference.

Settings, Test connection and the configuration reference pick it up from the
descriptor.
