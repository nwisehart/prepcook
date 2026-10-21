# Security and the trust model

Prepcook runs coding agents with a shell on your machine, and much of what they
read was written outside your team. This page says what the server trusts, what
an agent can do, where untrusted text comes in, and what is left to you.

## What the server trusts

Prepcook is built for one person on one machine, a model it inherits from
vibe-kanban. Every request that reaches the server on `127.0.0.1` is the board
owner: the board's own pages, `GET /api/cards/{id}`, `POST
/api/cards/{id}/convert` and the plan decisions without a token, and the
terminal, filesystem and script routes, which take no token at all. Any process
on the machine that can reach the port is, to the server, you.

The bearer token, `PREPCOOK_API_TOKEN`, is what lets another process act as a
service: filing a card takes nothing else ([api.md](api.md#authentication)).
`HOST` defaults to `127.0.0.1`. Setting it to anything wider exposes all of the
above, not only the card API, to whatever can reach that address. A Slack agent
such as Tammy on another machine reaches the server over a private network or a
tunnel that ends on the server's loopback, never through a public bind.

## What an agent can do

An agent runs as the user who runs the server, with a shell in the card's
worktree, the repository and its remotes, that user's `gh` login and SSH keys,
and their home directory, `~/.config/prepcook/secrets.env` included. What
Prepcook holds back is a matter of stage. Planning writes the harness and the
analysis and never pushes; Executing commits on the card's branch; only Shipped
pushes and opens the pull request. An investigation never executes until a
person converts it. In Prep mode, though, a bug or story whose plan clears the
bar approves itself and executes with nobody having read it.

## Where untrusted text enters

A card filed through the API carries whatever its filer wrote, which for Tammy
is a customer's own words. Anyone can open an issue on a public GitHub
repository, and anyone can comment on it or on the pull requests Prepcook opens,
whose comments go back to the implementing agent. A tracker ticket carries what
support typed into it. Attachments, the app's output while the agent runs it,
and the hits `evidence_search` returns reach the agent too.

Three things meet that text. Every playbook that carries it says the ticket is
evidence, not instructions, and asks the agent to quote any request to run,
fetch, change, read, send or reveal under `## Flagged` rather than act on it
([playbooks.md](playbooks.md#the-ticket-is-data)); the MCP tools that return it
say the same. A card filed from outside the team, which is any card with an
origin and any GitHub issue whose author is not an owner, member or
collaborator, plans under `agents.external`, by default the planning agent's
guarded variant: for Claude Code, `GUARDED`, without `WebFetch` and `WebSearch`
([method.md](method.md#cards-from-outside-the-team)). And the card API files at
most `api.cards.daily_limit` cards in a day ([api.md](api.md#file-a-card)).

None of that confines the agent. The framing is a prompt, and a prompt can be
argued with. The guarded profile narrows the agent's own tools; the shell it
keeps can still run `curl`, read any file its user can read and push with its
user's `gh` login. The other agents ship no guarded variant and plan such cards
unchanged. The cap bounds how many sessions a leaked token can start, not what
any one of them does. What gives the guarded profile teeth is **Settings →
Agents → Permissions** set to `supervised`, under which a tool call that is not a
read waits for the board owner
([working-alongside-people.md](working-alongside-people.md#plans-and-tool-calls));
only Claude Code, Codex, Gemini CLI, Qwen Code and OpenCode honour it.

The confinement that holds is the operating system's. Run the server as a
dedicated user that has none of your keys. Create the user; give it a `gh` login
and an SSH key of its own, with access only to the repositories the board works
on; clone those repositories into its home; set its secrets with `prepcook
secret set` while signed in as that user, so its `secrets.env` belongs to it
alone; and start `prepcook up` as that user. A container or virtual machine holding
the same things and nothing else is a harder wall in the same place. An agent
talked into something then reaches only what that user holds.

## Secrets

Secrets never go in `config.json`. They live in the secrets file, mode `0600`,
or in `PREPCOOK_SECRET_<NAME>` environment variables
([configuration.md](configuration.md#secrets)), and known secret values are
redacted from evidence results, errors and outgoing events rather than logged.
Evidence credentials stay with the server: the agent searches through
`evidence_search`, which the server runs with its own credentials and answers
with hits. Two things fall outside that, and the dedicated user covers both. A
source that gives sessions an MCP server of its own, as Sentry does, passes its
token in that server's environment in `agent-mcp.json`; and an agent running as
you can read your secrets file.

## Outbound

Webhook deliveries are signed: each carries an HMAC-SHA256 of its body keyed
with the target's own secret, when one is set, which it should be, and a
delivery id that repeats on retries. A receiver verifies the signature over the
raw body and drops ids it has seen
([notifications.md](notifications.md#verifying-the-signature)). Tammy runs every
answer through a leak check before it reaches a customer's thread and drops any
sentence that carries a stack trace, a file path or code.

## Reporting a vulnerability

Do not open a public issue: use GitHub's private vulnerability reporting, as
[SECURITY.md](../SECURITY.md) describes.
