<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/prepcook-logo-dark.svg">
    <img src="assets/brand/prepcook-logo.svg" alt="Prepcook" width="318">
  </picture>
</p>

A prep cook does the prep so the chef can cook. Prepcook does the prep on a
bug — reproduces it, explains it, fixes it, reviews the fix, opens the pull
request — and the engineer who owns the ticket signs off. When they take the
ticket over, Prepcook steps aside. Each ticket is a card on a local board,
and each stage of the card is worked by a coding agent (Claude Code) in its
own git worktree.

## Quickstart

This takes a bug from report to pull request on Pantry, a small demo app, with
no tracker: the card is the only record. You need macOS or Linux with Docker
(and Compose), Node 20 or later, git, Claude Code installed and signed in
(`claude` on your `PATH`), and the GitHub CLI signed in (`gh auth login`, which
also lets git push over HTTPS) for the pull request at the end.

Your own steps take about 20 minutes. The agents' working time comes on top,
most of it in Plan, and varies with the model and the machine; expect 30 to 60
minutes end to end.

### 1. Install and start Prepcook (about 10 minutes)

Download the tarball for your platform from the
[releases page](https://github.com/nwisehart/prepcook/releases)
(`prepcook-<version>-<target>.tar.gz`, with `SHA256SUMS` beside it), unpack it and
put `prepcook` and `prepcook-mcp` on your `PATH`:

```sh
tar -xzf prepcook-<version>-macos-arm64.tar.gz
cp prepcook-<version>-macos-arm64/prepcook prepcook-<version>-macos-arm64/prepcook-mcp ~/.local/bin/
```

Building from a checkout instead (Rust and pnpm as well) takes well over the
budget on most laptops:

```sh
pnpm install
pnpm -C packages/local-web build      # the server embeds the built UI
cargo build --release --bin prepcook --bin prepcook-mcp
```

Then:

```sh
prepcook up
```

`prepcook up` writes the board's Docker Compose file and a generated `.env` into
the app data directory and starts the board. The first time, Docker pulls the
board's images (Postgres, ElectricSQL and `ghcr.io/nwisehart/prepcook-board`):
several hundred megabytes, a few minutes on a typical connection, and the step
most likely to blow the budget. `prepcook up` then waits up to two minutes for the
board to answer, starts the Prepcook server and opens the UI in your browser.
There is nothing to sign in to: the server signs each request to the board with
the board's own secret, `PREPCOOK_JWT_SECRET` in that `.env`, and acts as the
board's owner. Keep the `.env` private: it holds that secret and the board's
database passwords. For a board on another machine, set `PREPCOOK_SHARED_API_BASE`
to its URL and `PREPCOOK_BOARD_SECRET` to its `PREPCOOK_JWT_SECRET`, in the
environment or with `prepcook secret set PREPCOOK_BOARD_SECRET`.

`prepcook up --no-browser` leaves the browser alone. `prepcook down` stops the board and
keeps its data (`--volumes` deletes it), and `prepcook status` says what is running
and whether the board accepts the server's secret.

### 2. Add the demo repository (about 5 minutes)

Fork [github.com/nwisehart/prepcook-demo](https://github.com/nwisehart/prepcook-demo) on
GitHub and clone your fork, so that `origin` is the fork:

```sh
git clone https://github.com/<you>/prepcook-demo.git ~/code/prepcook-demo
```

In Prepcook, **Settings → Repositories → Add Repository** and pick the clone. The
**Scripts** card shows what Prepcook read from the repository's
`.prepcook/scripts.toml`: `npm ci` to set up, `node server.js` to start, `/health`
as the ready check, `node scripts/seed.js` to seed and `npm test` to test, plus
Pantry's product profile. [docs/scripts.md](docs/scripts.md) describes the
file.

Click **Test stand-up**. Prepcook runs setup, start, the ready check and the seed in
your clone, on a port from 4200–4299, then stops the app. Success is the outcome
`ok` with every step at exit code 0; it takes about a minute, most of it
`npm ci`. Anything else names the failing step and shows its output. The run
leaves `node_modules/` and `data/` in the clone, both ignored by git.

### 3. Plan: reproduce and explain (about 5 minutes, then 10 to 20 waiting)

Make a board for it and turn the orchestrator on:

1. **Settings → Projects → Add Project**, named `Pantry`.
2. **Settings → Orchestrator**: switch on **Enable automatic agent launches**.
   Under **Automated projects**, add `Pantry` and tick the `prepcook-demo`
   repository. Leave autonomy off: this is Chef mode, where you move the cards.
   Save.

Open the Pantry board and click **New Issue**. Copy the first line of the
demo's [BUG.md](https://github.com/nwisehart/prepcook-demo/blob/main/BUG.md)
(without the `#`) into the title and the rest into the description. Add the
`bug` tag, which tells Prepcook which playbook the card takes, and make yourself
the assignee: you and Prepcook are both the board's owner, `owner@localhost`, and
Prepcook only works cards assigned to it.
Create it; it lands in **To Do**. Drag it to **Plan**.

Within one poll (15 seconds) Prepcook claims the card, makes a git worktree for it,
and stands Pantry up there from `.prepcook/scripts.toml`. Then a Claude Code session
starts with the `bug:planning` playbook. It reproduces the bug first: a
Playwright harness under `.repro/` in the worktree that shares the Weekend list
with `Ann@Example.com` and asserts the result, run against the app until it
fails for the reported reason. The first run downloads Playwright's Chromium.
The reproduction is posted on the card as a `REPRODUCTION:` comment. Only then
does the agent look for the cause, and it writes the root-cause analysis (RCA)
as an HTML page at `docs/specs/<card id>.html` in the worktree, with its
confidence and open questions recorded on the card. Plan usually takes 10 to
20 minutes. [docs/reproduce-first.md](docs/reproduce-first.md) explains why it
works in that order.

When the RCA is ready, the card shows **Review plan**. It opens the RCA in
Lavish, a review page with **Approve** and **Request changes** buttons; the same
draft is filed under the board's **Knowledge** menu. Approve it there, or comment
`APPROVE` on the card. Once the approval is recorded and the RCA's confidence is
high, the card moves to **Execute** by itself. If the confidence is lower, the
card says so and stays in Plan; ask for what is missing with a
`REQUEST CHANGES: <what>` comment. [docs/approvals.md](docs/approvals.md) has
every way to decide.

### 4. Execute, Review, Push MR, Clean up (about 5 minutes, then 15 to 30 waiting)

**Execute.** A new session in the same worktree, with the app stood up from the
fix branch, reads the RCA, runs the harness to watch it fail, and makes the
smallest change to the mechanism the RCA isolated. It adds regression tests,
runs them and the harness until both pass, and commits on a branch rebased onto
`main`. It does not push. Allow 5 to 15 minutes.

When the session has finished, check the branch the way Prepcook checks it before a
merge: comment `VERIFY:` on the card. Prepcook stands the app up fresh in the fix's
worktree, runs `npm test`, tears it down, and posts the result on the card. (In
Prep mode this pre-merge verify runs on its own and gates the move to Review.)

**Review.** Drag the card to **Review**. A new agent reviews the change in a
fresh worktree, without the RCA or the planning session; it reads only the
diff, the code around it, and the `REPRODUCTION:` comment. It records its
findings and posts `REVIEW: APPROVE` or `REVIEW: REQUEST CHANGES` on the card. A
`REQUEST CHANGES` goes back to the implementing agent as a fix round and is
reviewed again on its own, up to three rounds. Allow 5 to 10 minutes per round.
[docs/review.md](docs/review.md) covers what the reviewer weighs.

**Push MR.** After `REVIEW: APPROVE`, drag the card to **Push MR**. Prepcook pushes
the branch to your fork and opens a pull request there with `gh`, its first
comment the reproduction, so whoever reviews it can run the harness. The
implementing agent then writes the pull request's title and description. Merge
it on GitHub; within a few minutes the card moves to **Close**.

**Clean up.** Drag the card from Close to **Clean up**. Prepcook runs the
repository's clean-up script, when it has one, and archives the card's
worktrees. The approved RCA stays in the knowledge store.

[docs/method.md](docs/method.md) describes the whole method, including Prepcook
mode, where the cards move themselves.

### 5. Then

- **Connect a tracker.** **Settings → Integrations → Ticketing** (Jira or
  GitHub Issues) imports tickets, picks new ones up on its own, and lets
  approvers decide plans from the ticket. In incidents mode the source is
  PagerDuty, and open incidents become investigations
  ([docs/incidents.md](docs/incidents.md)). Every setting is in
  [docs/configuration.md](docs/configuration.md), and how Prepcook behaves on
  a shared ticket in
  [docs/working-alongside-people.md](docs/working-alongside-people.md).
- **Use the toolkit.** The same loop at your terminal, without the board:
  [toolkit/README.md](toolkit/README.md).

## How it works

A card moves through five stages.

**Plan.** Prepcook stands the app up in the card's worktree and an agent
reproduces the bug first, with a harness that fails for the reported reason.
Only then does it write the root-cause analysis: what happens, why, and how
sure it is. A person approves the analysis, asks for changes, or sends it back.

**Execute.** A new agent session makes the smallest fix for the mechanism the
analysis isolated and keeps at it until the reproduction passes, adding
regression tests. It commits on a branch rebased onto the target branch and
does not push.

**Review.** A separate agent reviews the change in a fresh workspace, with
only the diff and the ticket and none of the planning context, and records a
verdict with its findings. When it asks for changes, the findings go back to
the implementation agent to fix or rebut.

**Push MR.** Prepcook pushes the branch and opens the pull request (GitHub) or
merge request (GitLab), then feeds human review comments on it back to the
implementation agent.

**Clean-up.** Prepcook runs the repository's clean-up scripts, archives the
card's worktrees and closes the ticket. Every approved root-cause analysis is
kept in a knowledge store and linked into a graph of root-cause classes, so a
cause that keeps coming back becomes visible and can open a pattern review of
its own.

Not every card is a fix. An **investigation** card — one a support agent files
through `POST /api/cards` for a customer's problem, say — runs Plan exactly as a
bug does, then posts a fixed-shape triage report (what happens, where, why,
scope, workaround, confidence) and stops in a **Triaged** column, with no
approval to wait for; an `investigation_completed` event carries the report
back to whoever filed it. An engineer who wants it fixed converts the card to a
bug, and when the investigation's RCA and harness are good enough to be the
plan, it goes straight to Execute ([docs/method.md](docs/method.md#investigations),
[docs/api.md](docs/api.md)).

Prepcook runs in one of two modes. In **Chef mode** Prepcook works your own tickets:
you move cards between columns, approve plans in Lavish (the review page each
plan opens in), and stories and epics get a spec or a breakdown into
sub-tasks the same way bugs get a root-cause analysis. In **Prep mode** it
works on its own: new tickets are picked up and triaged as they arrive, cards
move themselves, and plan approvals come from the tracker ticket or the
approvals API. [docs/approvals.md](docs/approvals.md) covers every way a plan
is decided, and [docs/playbooks.md](docs/playbooks.md) the prompt each stage's
agent starts from and how to change it. [docs/method.md](docs/method.md) is the
longer version of this section, with links to the rest of the docs.

Cards live on the board, a separate service (Postgres, ElectricSQL and the
board server) that `prepcook up` runs in Docker. The Prepcook server reads and
writes them as the board's single owner: it signs a short-lived owner token
with the board's secret for every request, so there is no account, sign-in or
stored session. Agent users, the automation identities an admin creates on the
board, are separate: each signs in with its own email and secret.

## The local toolkit

The same method also works without the board. The toolkit is a set of Claude
Code skills generated from the orchestrator's own stage playbooks, so a ticket
worked by hand at your terminal gets the same reproduction harness, root-cause
analysis and pull request as one Prepcook worked, and its analysis is filed in the
same knowledge store. See [toolkit/README.md](toolkit/README.md).

## Development

```sh
pnpm install
pnpm run dev                 # server and web app with hot reload, ports chosen for you
pnpm run check               # type checks, frontend and Rust
pnpm run lint                # ESLint and clippy
pnpm run format              # cargo fmt and Prettier
cargo test --workspace
```

The server reads the board's address from `PREPCOOK_SHARED_API_BASE`
(`prepcook up` sets it to `http://127.0.0.1:4110`). To run the board from source
rather than the published image, use `pnpm run remote:dev`, or
`prepcook up --compose crates/remote/docker-compose.yml`;
[crates/remote/README.md](crates/remote/README.md) has the details.
[AGENTS.md](AGENTS.md) describes the repository layout and conventions.

## License

Apache License 2.0; see [LICENSE](LICENSE). Prepcook is derived from
[vibe-kanban](https://github.com/BloopAI/vibe-kanban) by BloopAI; see
[NOTICE](NOTICE).
