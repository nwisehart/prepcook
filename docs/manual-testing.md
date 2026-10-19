# Manual testing

The automated tests cover each piece on its own. This playbook covers the
whole: a person standing Prepcook up from a cold clone and working real cards
through it, the way a new user would, before every release. Run it on macOS
or Linux.

Section 1 sets everything up. Every later section starts from where section 1
leaves off and can be run on its own, in any order; the one exception is the
before-and-after check in section 10, whose "before" is taken in
[Before you start](#before-you-start).

Each step says what to run or click, what you should see (the exact strings,
where the code has them), roughly how long it takes, and what to collect if
it does not go that way. Labels in **bold** are what the screen shows.

## Before you start

You need:

- Docker with Compose (`docker compose version`), running.
- Node 22 or later, pnpm 10, git, `curl`, `jq` 1.6 or later and `openssl`.
- Rust through `rustup` (the repository's `rust-toolchain.toml` picks the
  version).
- Claude Code installed and signed in: `claude` on your `PATH`, and one
  `claude` session run to the prompt.
- The GitHub CLI signed in: `gh auth status` says "Logged in", with the
  `repo` scope.
- For the optional parts: a Slack workspace where you can install apps
  (section 9), a second GitHub account (section 7), a Jira site, a PagerDuty
  developer account or a Sentry account.

The commands below assume this layout; change the paths if yours differs:

```
~/code/prepcook        this repository
~/code/prepcook-demo   your fork of nwisehart/prepcook-demo (section 1)
~/code/tammy           Tammy (section 9)
```

Every shell you test from needs these. Put them in a file and source it:

```sh
export PREPCOOK_SRC=~/code/prepcook
export PANTRY=~/code/prepcook-demo
export PC=http://127.0.0.1:4100                                       # the server
export PC_DATA=~/.local/share/prepcook                                # Linux
# export PC_DATA="$HOME/Library/Application Support/prepcook" # macOS
export PATH="$PREPCOOK_SRC/target/release:$PATH"
[ -f ~/.config/prepcook/manual-test.env ] && { set -a; . ~/.config/prepcook/manual-test.env; set +a; }
```

`~/.config/prepcook/manual-test.env` holds the two test secrets; step 1.4
creates it.

Then take the "before" picture of your Claude Code configuration. Prepcook
never writes to it (see
[configuration.md](configuration.md#agent-mcp-servers-and-skills)), and
section 10 checks that it still has not:

```sh
mkdir -p ~/prepcook-manual-test
ls -la ~/.claude/skills/ > ~/prepcook-manual-test/skills.before 2>&1
jq -S '[.. | objects | .mcpServers? // empty | keys[]] | unique' ~/.claude.json \
  > ~/prepcook-manual-test/mcp.before
ls -la ~/.claude.json ~/.claude/skills/
```

### When a step fails

Write down the section, the step and the time, and collect:

| What | Where |
| --- | --- |
| The server's log | The terminal running `prepcook up`, and `$PC_DATA/logs/server.log`. Orchestrator lines start `Stage orchestrator:`. |
| The board's log | `docker compose --project-name prepcook logs remote-server` |
| What is running | `prepcook status` |
| A card | Its comments, and `curl -sS "$PC/api/cards/<card id>" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \| jq` |
| An agent session | The card's **Workspaces** list, then the session's log. The worktrees are under `/var/tmp/prepcook/worktrees` on Linux and `$TMPDIR/prepcook/worktrees` on macOS. |
| A Settings test run | The run panel's table and output (each step's last 60 lines). |
| Webhook deliveries | The output of `scripts/webhook-echo.mjs` (section 4). |

At the end of every section, check that the API answered no request with a
server error. Every 5xx is logged as `API request failed`:

```sh
grep -c 'API request failed' "$PC_DATA/logs/server.log"   # the same count as before the section
```

The card and board ids that steps ask for are in the board's URLs:
`$PC/projects/<board id>/issues/<card id>`.

## 1. Build and start

**About 60 minutes from a cold clone**, most of it compiling; about 20 with a
released binary.

### 1.1 Build from source (20 to 40 minutes)

```sh
git clone https://github.com/nwisehart/prepcook.git ~/code/prepcook
cd ~/code/prepcook
pnpm install
pnpm -C packages/local-web build       # the server embeds the built UI
cargo build --release --bin prepcook --bin prepcook-mcp
prepcook help
```

You should see the release build finish with no warnings, and `prepcook help`
print the usage, starting `usage: prepcook [command]` and listing `up`,
`down`, `status`, `secret` and `integrations describe`. `prepcook-mcp` sits
beside `prepcook` in `target/release`, which is where the server looks for it
first.

If it fails: the build output, `rustc --version`, `node --version`.

### 1.2 Start the board from source (15 to 30 minutes the first time)

Until a release has published the board's image, this is the only way to run
it: `--compose` points `prepcook up` at the compose file that builds the
board from this checkout.

```sh
cd ~/code/prepcook
BACKEND_PORT=4100 prepcook up --compose crates/remote/docker-compose.yml
```

`BACKEND_PORT` fixes the server's port at 4100, the port the docs, the card
links and Tammy assume; without it the server takes any free port and says
which. Leave this terminal running and use another for the rest.

You should see, in this order:

1. The first time only: `<app data dir>/board/.env holds the board's generated
   passwords and secret; keep it private.`
2. Compose building the `remote-server` image (most of the time) and starting
   `remote-db`, `electric` and `remote-server`.
3. `waiting for the board at http://127.0.0.1:4110 ... <n>s`, counting up,
   then `the board is up at http://127.0.0.1:4110`.
4. The server's log, including `Main server on :4100, Preview proxy on :<port>`.
5. ``Prepcook is running at http://127.0.0.1:4100 (Ctrl+C to stop it;
   `prepcook down` stops the board)``, and the UI opening in your browser
   (`--no-browser` leaves the browser alone).

You should not see:

- anything about installing or registering an MCP server: no
  `claude mcp add`, no line naming `~/.claude.json`. Agent sessions get the
  `prepcook` server per session, through `--mcp-config`;
- `Failed to send signal` warnings: the SIGINT, SIGTERM, SIGKILL ladder
  stopping a process group that would not stop;
- `API request failed` with `status=500`, or any `ERROR` line.

If the board does not answer within two minutes, `prepcook up` stops with
`the board did not answer at http://127.0.0.1:4110/v1/health within 120 s`.
Collect the board's log.

### 1.3 The board's `.env` and the owner (2 minutes)

```sh
ls -l "$PC_DATA/board/.env"
grep -v '^#' "$PC_DATA/board/.env" | cut -d= -f1     # names only, never the values
prepcook status
```

You should see the file at mode `-rw-------`; the names `DB_PASSWORD`,
`ELECTRIC_ROLE_PASSWORD`, `PREPCOOK_JWT_SECRET` and `PUBLIC_BASE_URL`; and:

```
board:  up at http://127.0.0.1:4110
server: running on port 4100 (http://127.0.0.1:4100)
auth:   the board accepts this machine's secret (as owner@localhost)
```

`owner@localhost` is you: the board's owner, the identity the server signs
every request with. There is nothing to sign in to in the UI.

If `auth:` says `the board refuses the board secret from …`, the server holds
a secret from somewhere other than this `.env` (an old `PREPCOOK_BOARD_SECRET`
in the environment or the secrets store). Collect `prepcook secret list` and
`env | grep -c PREPCOOK_BOARD_SECRET`.

### 1.4 The test secrets (2 minutes)

The card, approval and evidence APIs take a bearer token, and webhook
deliveries are signed with a secret. Generate both into a file only you can
read, and give them to Prepcook without printing them:

```sh
mkdir -p ~/.config/prepcook
( umask 077
  printf 'PREPCOOK_API_TOKEN=%s\nWEBHOOK_SECRET=%s\n' \
    "$(openssl rand -hex 32)" "$(openssl rand -hex 32)" > ~/.config/prepcook/manual-test.env )
set -a; . ~/.config/prepcook/manual-test.env; set +a
printf %s "$PREPCOOK_API_TOKEN" | prepcook secret set PREPCOOK_API_TOKEN
printf %s "$WEBHOOK_SECRET" | prepcook secret set WEBHOOK_SECRET
prepcook secret list
```

You should see `stored PREPCOOK_API_TOKEN in <home>/.config/prepcook/secrets.env`
and the same for `WEBHOOK_SECRET`; `prepcook secret list` prints the names
only, then `(file: …)`. The server reads the store on every request, so
nothing needs restarting. Check the token:

```sh
curl -sS -X POST "$PC/api/cards" -H 'Content-Type: application/json' -d '{}' | jq -r .message
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/cards" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" -H 'Content-Type: application/json' -d '{}'
```

The first prints `filing a card needs Authorization: Bearer <PREPCOOK_API_TOKEN>`;
the second `400` (the token is accepted, the empty body is not). Nothing is
written.

### 1.5 Stop, start and wipe (5 minutes)

Do this now, while the board has nothing on it.

1. **Ctrl+C** in the server's terminal. You should see `Shutdown signal
   received` and the process exit within a few seconds, with no `Failed to
   send signal` warning.
2. `prepcook status` says `server: not running (last ran on port 4100)`; the
   board is still up.
3. `prepcook down`: Compose stops the three containers. `prepcook status`
   then says `board:  not answering at http://127.0.0.1:4110` and
   `auth:   unknown (the board is not answering)`.
4. `prepcook down --volumes`: also removes the `prepcook_remote-db-data` and
   `prepcook_electric-data` volumes. Running it with the board already
   stopped is fine.
5. `BACKEND_PORT=4100 prepcook up --compose crates/remote/docker-compose.yml`
   again. This time there is no `.env` line (the file is kept, and the new
   database is created with its passwords), the image is not rebuilt, and the
   board is up in under a minute.

`prepcook down` with no `--volumes` keeps the data: cards, the knowledge
store and the projects survive it.

### 1.6 After a release: plain `prepcook up` (10 minutes)

Once a release has published `ghcr.io/nwisehart/prepcook-board:<version>`,
test the route the README gives new users. Stop the source-built board first
(`prepcook down`; the data is kept, and both routes use the same Compose
project and volumes), then download the release for your platform, check it
and put it ahead of `target/release` on your `PATH`:

```sh
sha256sum -c SHA256SUMS --ignore-missing          # macOS: shasum -a 256 -c SHA256SUMS --ignore-missing
tar -xzf prepcook-<version>-<target>.tar.gz
export PATH="$PWD/prepcook-<version>-<target>:$PATH"
BACKEND_PORT=4100 prepcook up
grep 'image:' "$PC_DATA/board/docker-compose.yml"
```

You should see Docker pull the board's image instead of building one, the
same output as step 1.2 from step 3 on, and the compose file naming
`ghcr.io/nwisehart/prepcook-board:<version>`, never `:latest`. Before the
first release this pull fails (the image does not exist): that is why step
1.2 exists.

### 1.7 The Pantry repository and board (10 minutes)

Every later section works on Pantry, the demo app. Fork
[nwisehart/prepcook-demo](https://github.com/nwisehart/prepcook-demo) and
clone your fork so that `origin` is the fork:

```sh
gh repo fork nwisehart/prepcook-demo --clone=false
git clone "https://github.com/$(gh api user -q .login)/prepcook-demo.git" "$PANTRY"
```

In the UI:

1. **Settings → Repositories → Add Repository**, and pick `$PANTRY`. The
   **Scripts** card shows what Prepcook read from `.prepcook/scripts.toml`:
   under **App steps**, `npm ci`, `node server.js`, the URL `/health`,
   `node scripts/seed.js` and `npm test`; under **Product profile**, Pantry.
2. **Settings → Projects → Add Project**, named `Pantry`. New boards get the
   columns **To Do**, **Plan**, **Triaged**, **Execute**, **Review**,
   **Push MR**, **Close** and **Clean up**.
3. **Settings → Orchestrator**: switch on **Enable automatic agent launches**;
   under **Automated projects**, add `Pantry` and tick `prepcook-demo` under
   **Repositories**. Leave **Autonomy mode** off (Chef mode). **Save**:
   `Orchestrator settings saved. They apply on the next poll.`

Note the board's id from its URL for the steps that ask for it:

```sh
export BOARD=<board id>
```

## 2. Pantry bug, no tracker

**About 25 minutes of your own steps, plus 30 to 60 minutes of agent time.**
This is the README's [Quickstart](../README.md#quickstart) run as a test:
one bug, reported on the board, taken to a merged pull request on your fork,
with no tracker configured.

### 2.1 Test stand-up (2 minutes)

**Settings → Repositories**, select `prepcook-demo`, and on the **Scripts**
card click **Test stand-up**.

You should see a panel titled **Test run: stand-up (local)**, then
**Outcome:** `ok` and a table of **Step**, **Exit code**, **Duration** and
**Timed out** with the rows `setup`, `start`, `ready` and `seed`, each exit
code `0`, none timed out. It takes about a minute, most of it `npm ci`. The
clone now has `node_modules/`, `data/` and `logs/app.log`, all ignored by git
(`git -C "$PANTRY" status` is clean).

If the outcome is anything else, it names the failing step; the table shows
that step's output. Collect it, and `curl -sS "$PC/api/repos/<repo id>/scripts" | jq`
(the repository's id is in `curl -sS "$PC/api/repos" | jq '.data[] | {id, path}'`).

### 2.2 The card (3 minutes, then 10 to 20 waiting)

Open the Pantry board and click **New Issue**. Copy the first line of
`$PANTRY/BUG.md` (without the `#`) into the title and the rest into the
description, add the `bug` tag, and make yourself (`owner@localhost`) the
assignee. **Create Issue**: the card lands in **To Do**. Drag it to **Plan**,
and note its id:

```sh
export CARD=<card id>
```

Within one poll (15 seconds) you should see a workspace appear in the card's
**Workspaces** list and a Claude Code session start in it. Then, over 10 to
20 minutes:

- a `REPRODUCTION:` comment: the harness shares the Weekend list with
  `Ann@Example.com` and fails because the page says "No user with that
  email". The harness is under `.repro/<card key>/` in the worktree;
- the root-cause analysis, as `docs/specs/<card key>.html` in the worktree,
  with its confidence and open questions recorded on the card;
- a **Review plan** button on the card.

If no workspace appears within a minute: check that the orchestrator is on,
the repository ticked, the card tagged `bug` and assigned to you. Collect the
`Stage orchestrator:` lines from the server's log.

### 2.3 The plan decision (5 minutes)

A plan can be approved three ways, and each release should exercise a
different one: in Lavish this time, with the card comment the next, through
the API the time after.

- **Lavish.** **Review plan** opens the RCA in Lavish; click **Approve**.
- **The card.** Comment `APPROVE` on the card.
- **The API.**

  ```sh
  curl -sS -X POST "$PC/api/approvals/$CARD" \
    -H "Authorization: Bearer $PREPCOOK_API_TOKEN" -H 'Content-Type: application/json' \
    -d '{"decision": "approve", "by": "Manual tester"}' | jq
  ```

  You should see `"success": true` and the `ApprovalRecord` in `data`: `"decision": "approve"`, `"source": "api"`, `"by": "Manual tester"`.

Whichever you used, the card gets a `gate:` comment and, with the RCA at
high confidence, moves to **Execute** by itself. If the confidence is lower,
the card says `Approved, but not advancing: this RCA's confidence is <level>
and Execute requires high.` and stays in Plan; ask for what is missing with a
`REQUEST CHANGES: <what>` comment.

Then check the record and the refusals:

```sh
curl -sS "$PC/api/approvals/$CARD" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" | jq '.data'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/approvals/$CARD" \
  -H 'Authorization: Bearer not-the-token' -H 'Content-Type: application/json' -d '{"decision": "approve"}'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/approvals/$CARD" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" -H 'Content-Type: application/json' -d '{"decision": "approve"}'
```

You should see one record whose `source` is `lavish`, `card` or `api`; then
`401` for the wrong token; then, once the card is in Execute, `409` (approve
is only taken in Plan).

### 2.4 Execute and the pre-merge verify (5 to 15 minutes, then 2)

A new session starts in the same worktree, with Pantry stood up from the fix
branch. It runs the harness and watches it fail, makes the change, adds
regression tests, and commits on a branch rebased onto `main`. It does not
push: `git -C <worktree> log --oneline origin/main..HEAD` shows the commits,
and the fork has no new branch.

When the session has finished, comment `VERIFY:` on the card. You should see
``verify: standing `<branch>` up in its worktree — the result lands here``,
then, a couple of minutes later, a comment starting `VERIFY: ok` with
**Verify — `ok`** and the steps it ran. Anything but `ok` names the step that
failed; collect the comment.

### 2.5 Review (5 to 10 minutes a round)

Drag the card to **Review**. A workspace named `Review: …` appears, and its
session reads the diff, the code around it and the `REPRODUCTION:` comment,
nothing from planning. You should see `REVIEW: APPROVE` or
`REVIEW: REQUEST CHANGES` on the card. A request for changes starts a fix
round in the implementation workspace and a fresh review after it, up to
three rounds.

### 2.6 Push MR (5 minutes, then the merge)

Drag the card to **Push MR**. The card shows **Pushing…**, then **Pushed**.
On your fork (`gh pr list -R <you>/prepcook-demo`) there is a pull request
from a `prepcook/…` branch into `main`. Its first comment is the
reproduction, and within a few minutes the implementing agent rewrites its
title and description.

Merge it (`gh pr merge <number> -R <you>/prepcook-demo --squash`, or on
GitHub). Within a few minutes the card moves to **Close**.

**Push failed** comes with a hint on the card; collect the server's log.

### 2.7 Clean up and the knowledge store (3 minutes)

Drag the card from **Close** to **Clean up**. Pantry's manifest has no
clean-up script, so Prepcook only archives the card's workspaces.

Open the board's project menu (**…** beside the board's name) and choose
**Knowledge**. The page, **Knowledge — RCA graph**, counts the card's RCA
under **RCAs stored** and lists it under **All RCAs**.

## 3. Investigation card

**About 15 minutes of your own steps, plus 10 to 20 minutes of Plan and 5 to
15 of Execute.** An investigation is filed by another service through the
card API, explained but not fixed, and converted to a bug when someone wants
it fixed ([method.md](method.md#investigations), [api.md](api.md#cards)).
Two cards are filed, so that one can be converted from the board and the
other through the API; they run side by side.

### 3.1 Take the Triaged column away (2 minutes)

To see what a board made before the column existed does, remove it: open the
board's project menu (**…**), **Edit project settings**, and under
**Project Statuses** use **Delete status** on **Triaged** (a column with
cards in it offers **Move issues first** instead).

### 3.2 File the cards (3 minutes)

```sh
file_card() {
  jq -n --rawfile bug "$PANTRY/BUG.md" --arg ref "$1" --arg board "$BOARD" '{
      project_id: $board,
      title: "Sharing a list fails for a user who exists",
      description: ($bug | sub("^# [^\n]*\n+"; "")),
      work_class: "investigation",
      origin: { kind: "api", ref: $ref, reporter: "Manual tester" },
      start: true
    }' |
    curl -sS -X POST "$PC/api/cards" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
      -H 'Content-Type: application/json' -d @-
}
A=$(file_card manual-test-a | tee /dev/stderr | jq -r .data.id)
B=$(file_card manual-test-b | tee /dev/stderr | jq -r .data.id)
```

You should see two `"success": true` envelopes, each with the card's `id`,
`url` and `key`. On the board both cards are in **Plan**, tagged
`investigation`, and read "Filed by the API from manual-test-a" (and `-b`).
Within a poll each gets a workspace and a session started from the
`investigation:planning` playbook.

Then the refusals, none of which writes anything:

```sh
curl -sS -X POST "$PC/api/cards" -H 'Content-Type: application/json' -d '{}' | jq -r .message
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/cards" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' -d '{"title": "x", "work_class": "chore", "origin": {"kind": "api", "ref": "x"}}'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/approvals/$A" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' -d '{"decision": "approve"}'
```

You should see `filing a card needs Authorization: Bearer <PREPCOOK_API_TOKEN>`,
then `400` (no such work class), then `409` (an investigation has no approval
gate).

### 3.3 Planning and the report (10 to 20 minutes)

Each card gets a `REPRODUCTION:` comment and an RCA, as a bug does, then a
comment that opens `TRIAGE REPORT` with `## What happens`, `## Where`,
`## Why`, `## Scope`, `## Workaround` and `## Confidence`. Read the card as
it goes:

```sh
curl -sS "$PC/api/cards/$A" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  | jq '.data | {status, column, work_class, confidence, harness, origin, report: (.report != null)}'
```

Before the report: `"status": "planning"`, `"column": "Plan"`,
`"work_class": "investigation"`, `"origin": {"kind": "api", "ref": "manual-test-a", …}`
and `"report": false`. After it: `"report": true`, a `confidence`, and
`harness` with a `.repro/<key>/…` path and `"outcome": "reproduced"`.

### 3.4 No Triaged column: `needs-column` (2 minutes)

With the report recorded and no column to move to, each card stays in
**Plan**, tagged `needs-column`, and the server logs once per card:

```
Stage orchestrator: <key> has its triage report, but board <board id> has no Triaged column; it stays in Planning tagged 'needs-column' until one is added between Plan and Execute
```

`investigation_completed` has already gone to the notification targets; only
the move waits.

Put the column back: **Edit project settings → Project Statuses → Add
column**, rename the new column to `Triaged` with **Edit name**, drag it
between **Plan** and **Execute**, and save. Within a poll both cards move to
**Triaged**, lose the `needs-column` tag and get the comment
`triage: report recorded — moved to Triaged; convert the card to a bug to fix it`.
`GET /api/cards/$A` now says `"status": "triaged"`, `"column": "Triaged"`.

### 3.5 Convert to bug, on the board and through the API (5 minutes, then Execute)

On card A, open the menu of its `investigation` label and choose **Convert
to bug**. For card B:

```sh
curl -sS -X POST "$PC/api/cards/$B/convert" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' -d '{"work_class": "bug", "by": "Manual tester"}' | jq
```

You should see `"from": "investigation"`, `"to": "bug"`,
`"column": "Execute"` and `"plan_carried_over": true` when the RCA is at high
confidence and the harness is still on disk; otherwise `"column": "Plan"` and
`false`, and the card is planned again as a bug. Both cards get a `convert:`
comment and their label reads `bug`.

With the plan carried over, each card is in **Execute** and a
`bug:executing` session starts in the planning worktree: the card's
**Workspaces** list shows the same workspace with a new session, and no new
planning session runs. Converting again is refused:

```sh
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "$PC/api/cards/$B/convert" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" -H 'Content-Type: application/json' -d '{"work_class": "bug"}'
```

prints `409`. Take the cards on through section 2's steps 2.4 to 2.7, or
drag them to **Close** and then **Clean up**.

## 4. Notifications

**About 20 minutes, plus waiting for the events the other sections raise.**
Events go to a webhook you run here, which prints each delivery and checks
its signature ([notifications.md](notifications.md)).

### 4.1 The echo (1 minute)

[`scripts/webhook-echo.mjs`](../scripts/webhook-echo.mjs) answers every
delivery with `200`, prints its event, delivery id and signature verdict and
the body, and checks `X-PrepCook-Signature` against `WEBHOOK_SECRET` from its
environment:

```sh
cd "$PREPCOOK_SRC"
node scripts/webhook-echo.mjs            # --port 4999 is the default
```

You should see `webhook-echo on http://127.0.0.1:4999/ (answering 200; checking signatures)`.
If it says `WEBHOOK_SECRET unset`, source `~/.config/prepcook/manual-test.env`
first.

### 4.2 The target and Test connection (3 minutes)

**Settings → Integrations → Notifications**: choose **Webhook** under
**Add**, click **Add**, and fill in **URL** `http://127.0.0.1:4999/` and
**Events** `*`. Leave **Signing secret** blank: the target then signs with
`WEBHOOK_SECRET` from the secrets store. **Save**:
`Notification targets saved.`

Click **Test connection**. You should see `Delivered a ping (HTTP 200)`, and
the echo print:

```
event=ping delivery=<uuid> signature ok
{
  "type": "ping",
  "severity": "info",
  …
  "data": {}
}
```

### 4.3 Retries and a bad signature (3 minutes)

Stop the echo and start it failing:

```sh
node scripts/webhook-echo.mjs --fail
```

**Test connection** now takes about four seconds and shows
`HTTP 500: failing on purpose (--fail)`; the echo prints the same
`delivery=<uuid>` four times, the retries 0.5, 1 and 2 seconds apart. Then start it with the wrong secret:

```sh
WEBHOOK_SECRET=wrong node scripts/webhook-echo.mjs
```

**Test connection** succeeds (the echo answers 200 regardless) and the echo
prints `SIGNATURE MISMATCH`. Restart it plainly before going on.

### 4.4 Events from doing things

There is no "send now" for any event: each is raised by the work it
describes. Trigger the ones you can and check each delivery says
`signature ok`, its `X-PrepCook-Event` matches `type`, and
`prepcook.board_url` is `http://localhost:4100/projects/<board id>`.

| Event | How to raise it | What the delivery carries |
| --- | --- | --- |
| `investigation_completed` | File an investigation (section 3.2); 10 to 20 minutes. | The report's sections, `harness.outcome` `reproduced`, `confidence`, and `source` `{ "kind": "api", "ref": "manual-test-a", … }`. |
| `card_converted` | Convert it (section 3.5). | `from` `investigation`, `to` `bug`, `by`, and `column` `Execute` or `Plan`. |
| `blocker_mr_opened` | Before dragging a bug card into **Push MR** (section 2.6), set its priority to **Urgent** (**Change Priority**). | `mr_url` and `summary`. The pull request is labelled `Blocker` when the fork has that label (`gh label create Blocker -R <you>/prepcook-demo`); without it the server logs `labelling MR !<n> Blocker failed (does the label exist in the project?)` and the event is sent all the same. |
| `replan_requested` | With a card in **Push MR**, comment on the card: `MR-FEEDBACK: NEEDS-REPLAN` on the first line and a reason on the second. This is the verdict the implementing agent writes when merge-request review rejects the approach; posting it yourself tests the delivery without waiting on its judgement. Read within five minutes. | `reason` (your second line) and `mr_url`. The card is tagged `needs-replan` and gets a comment saying a person decides whether it goes back to Plan. Clear it with a `MR-FEEDBACK: ADDRESSED` comment. |
| `runner_fault` | A card on a board whose stand-up fails for a setup reason: section 6.2's board set to remote with no `[app.remote]`, with a bug card dragged to **Plan**. | `step` `Planning app stand-up`, `outcome` `target-unconfigured`, and `detail`. Once per card and outcome. |
| `daily_digest` | The schedule, which is *file only*: stop the server, set the time, start it again (below). Weekdays only. | `date`, `timezone`, and the sections `needs_merge`, `needs_answer` and `not_moving`, always all three. |

For the digest, with the server stopped:

```sh
tmp=$(mktemp)
jq '.orchestrator.digest_at_local = "00:01" | .orchestrator.daily_timezone = "Europe/London"' \
  "$PC_DATA/config.json" > "$tmp" && cat "$tmp" > "$PC_DATA/config.json" && rm "$tmp"
```

(use your own IANA zone), then start the server as in step 1.2. On the first
poll after the time has passed, each automated board sends its digest, once a
day. Set `digest_at_local` back to `null` the same way afterwards.

The seat events (`seat_limit_hit`, `seat_exhausted`, `seat_expired`,
`seat_recovered`) cannot be raised on demand and are not part of this run.

### 4.5 Slack (optional, 5 minutes)

With a Slack incoming webhook, put its URL in the secrets store
(`prepcook secret set SLACK_WEBHOOK_URL`), add a **Slack** target with the
other fields blank, and **Test connection**. A message arrives in the
webhook's channel. The events above then arrive there too, rendered for
Slack.

## 5. Evidence

**About 10 minutes.** Pantry's manifest names its own log file under
`[[profile.evidence]]`; Prepcook describes it to the planning agents and
searches it with `evidence_search` ([evidence.md](evidence.md)). The search
window defaults to the last 24 hours, so run a **Test stand-up** (section
2.1) first if you have not today: it leaves `/health` requests in
`$PANTRY/logs/app.log`.

### 5.1 The paragraph the agents get (2 minutes)

```sh
curl -sS "$PC/api/playbooks/context?project_id=$BOARD" \
  | jq -r '.data.vars["product.evidence_tools"], "---", .data.vars["product.evidence_sources"]'
```

You should see three paragraphs:

```
Pantry app log (Log files): `logs/app.log`, relative to the worktree. Read it with tail, grep or jq, or call `evidence_search` with source "Pantry app log": it returns the lines that contain the pattern, ignoring case, leaving out a line whose timestamp falls outside the window and keeping one that has none. Look for the failing request and the error logged after it, around the time you ran the harness. One JSON line per request (method, path, status, duration, user id) and one per error, with the stack trace.

There is no production log access: the app's own log is the only record of a request.

If none of these show the failure, the harness is your evidence.
---
- Pantry app log
```

The same paragraphs are in the prompt a planning session starts from: open
any Pantry card's planning workspace from its **Workspaces** list and find
`Pantry app log (Log files)` in the session's first message.

### 5.2 `POST /api/evidence/search` (3 minutes)

```sh
jq -n --arg wt "$PANTRY" --arg board "$BOARD" \
  '{pattern: "/health", source: "Pantry app log", limit: 3, project_id: $board, worktree: $wt}' |
  curl -sS -X POST "$PC/api/evidence/search" -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
    -H 'Content-Type: application/json' -d @- | jq '.data'
```

You should see up to three `hits`, newest first, each with
`"source": "Pantry app log"`, a `message` that is one of Pantry's JSON log
lines with `"path":"/health"`, its time in `at`, and
`"fields": { "file": "logs/app.log", "line": <n> }`; and in `sources`, one
entry with `"searched": true` and no `error`. Then:

- `"source": "Sentry"` (no such source here) answers `400`, and `message`
  lists the configured labels.
- `"pattern": "no-such-text-anywhere"` answers `200` with `"hits": []`.

### 5.3 From a session (5 minutes)

Pick a card whose work is done (in **Triaged** or **Close**), open its
workspace and send the session a follow-up: "Call the evidence_search tool
with pattern "share" and source "Pantry app log", and show me the raw
result." The session log shows the `evidence_search` call and hits from the
worktree's own `logs/app.log`: the requests the harness made.

### 5.4 Sentry (optional, 10 minutes)

**Settings → Integrations → Evidence**: add **Sentry**, put a token in the
secrets store (`prepcook secret set SENTRY_ACCESS_TOKEN`) and fill in
**Organization slug** and **Project slug**; **Test connection**, then
**Save**. Step 5.1 now shows a Sentry paragraph before Pantry's, and a new
agent session has a `sentry` MCP server.

## 6. Reproduction, remote target

**About 20 minutes, plus 10 to 20 of Plan.** A board set to the `remote`
target deploys each card's branch with `[app.remote]` and works against the
URL the deploy reports ([scripts.md](scripts.md#remote-targets)). Two scripts
stand in for a preview-environment CLI on this machine:

- [`scripts/fake-deploy.sh`](../scripts/fake-deploy.sh) starts Pantry from
  the worktree in the background on a free port, waits for `/health`, and
  appends `PREPCOOK_APP_URL` and `PREPCOOK_TARGET_REF` to
  `$PREPCOOK_ENV_OUT`;
- [`scripts/fake-destroy.sh`](../scripts/fake-destroy.sh) stops the process
  `$PREPCOOK_TARGET_REF` names.

Both keep a `<ref>.pid` and `<ref>.log` per deploy in
`${TMPDIR:-/tmp}/prepcook-fake-deploy`.

### 6.1 Try the scripts by hand (2 minutes)

```sh
cd "$PANTRY"
out=$(mktemp)
PREPCOOK_ENV_OUT=$out "$PREPCOOK_SRC/scripts/fake-deploy.sh"
cat "$out"
curl -sS "$(sed -n 's/^PREPCOOK_APP_URL=//p' "$out")/health"; echo
PREPCOOK_TARGET_REF=$(sed -n 's/^PREPCOOK_TARGET_REF=//p' "$out") "$PREPCOOK_SRC/scripts/fake-destroy.sh"
```

You should see `fake-deploy: deployed fake-manual<pid> (pid <n>) from <path>`
and the URL; the two lines `PREPCOOK_APP_URL=http://127.0.0.1:<port>` and
`PREPCOOK_TARGET_REF=fake-manual<pid>`; `/health` answering
`{"status":"ok"}` (or `SeedMissing` before a seed); and
`fake-destroy: stopped fake-manual<pid> (pid <n>)`.

### 6.2 A remote board without `[app.remote]` (3 minutes)

**Settings → Integrations → Reproduction**: under **Boards**, set Pantry to
**Remote deploy** and **Save**. Back on **Settings → Repositories →
prepcook-demo**, the **Scripts** card's **Reproduction target** is remote
and says the manifest has no `[app.remote]`. Click **Test stand-up**: the
panel is **Test run: stand-up (remote)**, **Outcome:** `target-unconfigured`
and **No step ran.**

A bug card dragged into **Plan** now does not stand anything up. The card
gets `app: could not be stood up — **target-unconfigured** (the setup's
problem, not the branch's); started the Planning session without it`,
`runner_fault` goes to the notification targets (section 4.4), and the
planning session starts without an app. Move that card straight to **Close**
before 6.3.

### 6.3 Add `[app.remote]` (5 minutes)

New workspaces branch from `origin/<target branch>`, so the manifest change
has to reach your fork. Keep it off `main`, on a branch of its own:

```sh
cd "$PANTRY"
git switch -c remote-test
cat >> .prepcook/scripts.toml <<EOF

# The remote target, faked on this machine (docs/manual-testing.md in Prepcook).
[app.remote]
start = "$PREPCOOK_SRC/scripts/fake-deploy.sh"
ready = { url = "/health", timeout_secs = 60 }
seed = "node scripts/seed.js"
test = "npm test"
stop = "$PREPCOOK_SRC/scripts/fake-destroy.sh"
EOF
git commit -am "Fake remote target for manual testing"
git push -u origin remote-test
```

The heredoc writes this checkout's absolute path into the manifest. Then
**Settings → Repositories → prepcook-demo**: set **Default Target Branch** to
`remote-test` and **Save Repository Settings**; on the **Scripts** card click
**Reload**. The `[app]` seed is not used remotely, so `[app.remote]` names it
again; the deployed server reads the data file the seed writes in the same
worktree.

### 6.4 Test stand-up and a card on the remote target (5 minutes, then 10 to 20)

**Test stand-up**: **Test run: stand-up (remote)**, **Outcome:** `ok`, with
the rows `setup`, `start`, `ready` and `seed` (no `build`: a remote deploy
builds where it deploys). Afterwards
`ls "${TMPDIR:-/tmp}/prepcook-fake-deploy"/*.pid` finds nothing: the stand-up
destroyed what it deployed.

Then a bug card from `BUG.md`, as in section 2.2, dragged to **Plan**. While
it plans:

```sh
ls "${TMPDIR:-/tmp}/prepcook-fake-deploy"/          # one fake-<ref>.pid
curl -sS "$PC/api/app-runs/current?worktree=<the card's worktree>" | jq '.data'
```

The run's `target_kind` is `remote`, it has no port, and its URL is the one
in the deploy's log, outside 4200–4299. The `REPRODUCTION:` comment comes
from a harness run against that URL. When the planning session ends, the
`.pid` file is gone and the process with it.

### 6.5 Put it back (2 minutes)

**Settings → Integrations → Reproduction**: Pantry back to
**Default (This machine)**;
**Default Target Branch** back to `main`; `git -C "$PANTRY" switch main`.
Drag the card to **Close**, then **Clean up**.

## 7. Ticketing: GitHub Issues

**About 20 minutes of your own steps, plus 30 to 60 of agent time.** Cards
come from issues in a scratch repository, and each stage shows on the issue
as a label ([method.md](method.md#tickets-or-incidents)). The code still
lives in your Pantry fork; only the tickets are in the scratch repository.

### 7.1 The scratch repository and the source (5 minutes)

```sh
gh repo create "$(gh api user -q .login)/prepcook-scratch" --private
```

**Settings → Integrations → Ticketing**: under **Source** choose
**Tickets**, add **GitHub Issues**, and set **Repository** to
`<you>/prepcook-scratch`. Leave **Agent login** and the filters blank (the
pickup filter defaults to `state:open label:bug`). **Test connection**:
`gh is signed in as <you>; <you>/prepcook-scratch has 0 open issue(s).`

Under **Ticket pickup**, tick **Pick up new tickets automatically** and set
**Import into project** to Pantry. Under **Workflow → Stage statuses**,
leave **Planning** blank (it falls back to `In Progress`) and set
**Executing** `executing`, **Reviewing** `in review`, **Pushed MR**
`pr open` and **Clean-up** `done`. **Save**:
`Ticketing settings saved.`

### 7.2 File an issue (2 minutes, then 10 to 20 waiting)

```sh
sed 1,2d "$PANTRY/BUG.md" > /tmp/bug-body.md
gh issue create -R "$(gh api user -q .login)/prepcook-scratch" --label bug \
  --title "Sharing a list fails for a user who exists" --body-file /tmp/bug-body.md
```

Within the pickup's poll (45 seconds) a card keyed `PREPCOOK_SCRATCH-1`
appears in **Plan**, assigned to you, and planning starts. On the issue: the
label `In Progress`, your account as assignee, and, once the harness has run,
a plain-language comment on the reproduction.

### 7.3 The decision from the issue (5 minutes)

On a tracker ticket the decision is a comment whose first line is
`RCA: APPROVED` (or `RCA: CHANGES — <reasons>`, `REPLAN: <reasons>`);
`APPROVE` is the card's form ([approvals.md](approvals.md#the-tracker-ticket)).
Only accounts ticked under **Plan approvers** can decide, and never the
orchestrator's own: the account `gh` is signed in as, which is yours.

- From your own account, comment `RCA: APPROVED`. It is not acted on: the
  card stays in Plan.
- With a second GitHub account: add it as a collaborator on the scratch
  repository; **Settings → Integrations → Ticketing → Plan approvers →
  Import from tracker**, tick it and **Save**; comment `RCA: APPROVED` from
  it. Within about two minutes Prepcook answers on the issue, the card moves
  to **Execute** and the label changes to `executing`.
- Without one, approve on the card or in Lavish (section 2.3).

### 7.4 Labels, the pull request link and the close (with section 2's steps)

Take the card through section 2's steps 2.4 to 2.7. On the issue you should
see the label follow the stages (`executing`, `in review`, `pr open`); after
Push MR a comment `**Merge request !<n>**: <pull request URL>`; and on
**Clean up**, the label `done` and the issue closed.

### 7.5 Jira (optional, 20 minutes)

The same with **Jira**: **Site**, **Email**, the token in the store
(`prepcook secret set JIRA_API_TOKEN`), **Project key**, and **Stage
statuses** named after your workflow's statuses. **Test connection**, file a
Bug in the project with priority P0 or P1 (the default pickup filter), and
decide with `RCA: APPROVED` from an account ticked under **Plan approvers**.

### 7.6 PagerDuty incidents (optional, 30 minutes)

In a PagerDuty developer account, make one service with an **Events API v2**
integration (its integration key is the routing key) and a dedicated user
for Prepcook on no schedule. Then **Settings → Integrations → Ticketing**:
**Source** **Incidents**, add **PagerDuty**, **User id** that user,
**Service ids** the service; the REST API key in the store
(`prepcook secret set PAGERDUTY_API_TOKEN`). **Test connection** reads the
user. Tick **Pick up new tickets automatically** into Pantry, and **Resolve
the incident when it is triaged** if you want to see that too. **Save**.

Trigger an incident without printing the key:

```sh
read -rs PD_ROUTING_KEY
jq -n --arg key "$PD_ROUTING_KEY" '{
    routing_key: $key, event_action: "trigger",
    payload: { summary: "Sharing a list fails for a user who exists", source: "pantry", severity: "error" }
  }' | curl -sS -X POST https://events.pagerduty.com/v2/enqueue -H 'Content-Type: application/json' -d @-
```

You should see `"status": "success"`; within 45 seconds an investigation card
in **Plan** whose origin is the incident's page; the incident acknowledged by
the Prepcook user; after 10 to 20 minutes the card in **Triaged** and the
triage report added to the incident as a note; and, with the box ticked, the
incident resolved. Nobody is paged by Prepcook at any point.

### 7.7 Put it back (2 minutes)

Remove the source (**Remove** on the GitHub Issues, Jira or PagerDuty block)
and **Save**, so the other sections run with no tracker.

## 8. Agents

**About 5 minutes, plus one Execute session.**

### 8.1 Test on Claude Code (1 minute)

**Settings → Integrations → Agents**. Under **Coding agents**, the Claude
Code block shows **Recent Usage Detected** and, for **Variants**,
`DEFAULT, SONNET (economy)`. Click **Test**: within 10 seconds,
`<what claude --version prints>; sign-in found`. `no sign-in found (…)`
means Claude Code has not been signed in on this machine.

### 8.2 A different variant for Executing (2 minutes, then one session)

Under **Stages**, **Executing** reads `Default (Claude Code · Sonnet)`: unset,
Execute runs the cheaper variant. Set it to `Claude Code` and **Save**:
`Agent settings saved.` `jq '.agents.stages.executing' "$PC_DATA/config.json"`
names `CLAUDE_CODE` with no variant.

Take a card into **Execute** (section 2.3, or a conversion in section 3.5).
Near the top of the Execute session's log, `System initialized with model:
<id>` now names the same model as the card's planning session, where before
it named a Sonnet model (unless your account's own default is Sonnet). A
follow-up keeps its session's agent, so only a new session shows the change.

Set **Executing** back to `Default` and **Save**.

## 9. Tammy

**About 30 minutes of your own steps, plus 10 to 20 of Plan.** Tammy is the
Slack agent that files investigations; the Quickstart in her own README
(`~/code/tammy/README.md`) is the procedure, and this section is what to
check along it. She expects Prepcook at `http://localhost:4100` and the Pantry
board with its **Triaged** column.

### 9.1 The menu, offline (5 minutes)

```sh
cd ~/code/tammy
pnpm install
cp tammy.example.toml tammy.toml
pnpm tammy index ../prepcook-demo/docs
env -u ANTHROPIC_API_KEY pnpm tammy ask "how do I share a list"
```

The first index downloads the embedding model (about 25 MB) and logs
`<n> added, 0 updated, 0 unchanged, 0 removed; <n> document(s), <n> chunk(s) in …`.
`tammy ask` without Claude configured (`provider = "api"`, as the example
has it, and no `ANTHROPIC_API_KEY` yet) prints the passages she would read,
best first with scores and citations, the top one from Pantry's sharing page,
and no answer. Nothing goes to Slack or Prepcook.

### 9.2 The Slack app (optional without a workspace, 10 minutes)

As Tammy's README step 1: the app from the manifest in her `docs/setup.md`,
the `xapp-…` and `xoxb-…` tokens, `#pantry-help` and `#pantry-team` with
Tammy invited to both, and their ids in `tammy.toml`.

### 9.3 Prepcook's side (3 minutes)

Tammy uses the same token and signing secret as the rest of this run. Give
them to her without printing them (if the file already exists, skip the
`install` line and only append):

```sh
mkdir -p ~/.config/tammy
install -m 600 /dev/null ~/.config/tammy/secrets.env
printf 'PREPCOOK_API_TOKEN=%s\nTAMMY_WEBHOOK_SECRET=%s\n' "$PREPCOOK_API_TOKEN" "$WEBHOOK_SECRET" \
  >> ~/.config/tammy/secrets.env
```

and add `TAMMY_SLACK_BOT_TOKEN`, `TAMMY_SLACK_APP_TOKEN` and
`ANTHROPIC_API_KEY` in an editor. In Prepcook, add a second **Webhook**
target: **URL** `http://127.0.0.1:3978/prepcook/webhook`, **Events**
`investigation_completed, card_converted`, **Signing secret** blank; **Save**.

### 9.4 The whole loop (10 minutes, then 10 to 20 waiting)

`pnpm start`. You should see `connected to Slack (Socket Mode)` and
`webhook listening on http://127.0.0.1:3978/prepcook/webhook`; Tammy's
target's **Test connection** in Prepcook says `Delivered a ping (HTTP 202)`.

1. **A question.** In `#pantry-help`, `@Tammy how do I share a list?`. She
   replies in the thread, "_Tammy is writing…_" edited into the steps from
   Pantry's sharing page, with _From the menu: Sharing a list with someone_
   underneath.
2. **A problem report.** A new thread, as her README's step 6 words it. She
   asks her follow-up questions once, numbered; answer them. She files the
   card and says she has taken it to the kitchen; `#pantry-team` gets
   "New order PC-… : …"; her log says `problem_report, filed PC-…`. On the
   board the card is in **Plan**, tagged `investigation`, "Filed by Tammy
   from thread".
3. **The report.** When the card reaches **Triaged**, she posts in the
   customer's thread in plain English: what happens and why, that it is on
   our side, the lower-case workaround, and no paths, code or promises.
   `#pantry-team` gets the short version. The echo from section 4, if it is
   still a target, shows the same `investigation_completed`.
4. **The conversion.** In `#pantry-team`, `/tammy convert <key>`. The card
   goes to **Execute** (or **Plan**), and the customer's thread gets one line
   saying the kitchen has taken it on as a bug, with no date.

If a step stalls, collect Tammy's log (`TAMMY_LOG_LEVEL=debug pnpm start`
adds the events she ignored), `curl -sS http://127.0.0.1:3978/health`, and
the card.

### 9.5 On a Claude seat (optional, 15 minutes)

Repeat 9.4 with `[brain] provider = "seat"`, following the "API key or
seat" part of her README, and `ANTHROPIC_API_KEY` removed from her secrets.
The loop is the same; at `TAMMY_LOG_LEVEL=debug` each turn's token line also
carries the `claude` CLI's cost estimate, which only the seat reports.

## 10. Cleanup

**About 10 minutes.**

1. **Settings → Integrations → Notifications**: **Remove** both webhook
   targets (and Slack), **Save**. Remove anything left under **Ticketing**,
   **Evidence** and **Reproduction**, and set **Agents** back to its
   defaults.
2. Remove the test secrets, and any optional ones you set:

   ```sh
   for name in PREPCOOK_API_TOKEN WEBHOOK_SECRET SLACK_WEBHOOK_URL SENTRY_ACCESS_TOKEN \
               JIRA_API_TOKEN PAGERDUTY_API_TOKEN; do prepcook secret unset "$name"; done
   prepcook secret list
   ```

   Each set one prints `removed <name> from …`; the list names none of them.
3. Stop the echo and Tammy, and **Ctrl+C** the server. No fake deploy is
   left running: `ls "${TMPDIR:-/tmp}/prepcook-fake-deploy"/*.pid` finds
   nothing (otherwise `scripts/fake-destroy.sh <ref>` for each).
4. Stop the board and delete its data:

   ```sh
   prepcook down --volumes
   docker volume ls | grep prepcook        # nothing
   prepcook status                         # board not answering, server not running
   ```

5. GitHub: delete the scratch repository
   (`gh repo delete <you>/prepcook-scratch --yes`, after
   `gh auth refresh -s delete_repo` if `gh` lacks the scope), the
   `remote-test` branch (`git -C "$PANTRY" push origin --delete remote-test`),
   and close any pull request still open on the fork.
6. Nothing was installed into Claude Code, so there is nothing to
   `claude mcp remove`. Check:

   ```sh
   ls -la ~/.claude/skills/ 2>&1 | diff ~/prepcook-manual-test/skills.before - && echo 'skills untouched'
   jq -S '[.. | objects | .mcpServers? // empty | keys[]] | unique' ~/.claude.json \
     | diff ~/prepcook-manual-test/mcp.before - && echo 'MCP servers untouched'
   ls -la ~/.claude.json ~/.claude/skills/
   ```

   Both print "untouched". `~/.claude.json` itself will have a new size and
   time: Claude Code rewrites it on every session, which is why its MCP
   servers are compared rather than the file.
7. `rm -rf ~/prepcook-manual-test ~/.config/prepcook/manual-test.env`. For the
   next run to start cold, also remove the app data directory
   (`rm -rf "$PC_DATA"`), which holds the configuration, the local database
   and the board's `.env`.
