#!/usr/bin/env node
// Captures the screenshots the README and docs/images/README.md list, from a
// running Prepcook, light then dark, at 1440×900 and device scale 2 (so each
// PNG is 2880×1800). `--seed` first puts a sample Pantry board on it.
//
//   PREPCOOK_API_TOKEN=… node scripts/screenshots.mjs [--seed] [--url http://127.0.0.1:4100]
//     [--repo ../prepcook-demo] [--only board,card-rca] [--out docs/images]
//     [--chromium <path>] [--mask <text>]… [--list]
//
// The server: --url, else the one `prepcook status` reports running.
// PREPCOOK_API_TOKEN is required (it files the cards and proves the server is
// the one the token belongs to); --list needs neither.
//
// Browser: Playwright's Chromium (`npx playwright install chromium`), or any
// Chromium build named with --chromium.
//
// What --seed does, through the local server's API only:
//   - registers the --repo checkout (POST /api/repos) unless it already is;
//   - finds the board named "Pantry" (GET /api/remote/projects), or creates it
//     on the board service the way Settings → Projects does (POST /v1/projects
//     with the owner token from GET /api/auth/token; the token is never
//     printed), which gives it the default columns;
//   - files six cards with POST /api/cards and walks each to its column one
//     move at a time with PATCH /api/remote/issues/{id}, as the workflow
//     rules require: one in To Do, two in Plan (a bug waiting at the gate and
//     a triaged investigation), one each in Execute and Review, and one in
//     Shipped with its pull request merged. It posts their comments with
//     POST /api/remote/issue_comments: the bug in Plan gets a REPRODUCTION
//     comment and an RCA comment, the investigation a TRIAGE REPORT, the
//     Review card a REVIEW comment (texts in scripts/fixtures/). The states
//     the board reads go in the card's metadata (the same PATCH): the bug's
//     planning verdict, the investigation's triage record and the Shipped
//     card's `merged_at`. The Shipped card's merged pull request is recorded
//     on the board service (POST /v1/pull_requests with the owner token; it
//     points at the demo repository's pull request #1, which need not exist).
//     A card whose title is already on the board is left alone, so seeding
//     twice is safe;
//   - adds one webhook notification target (http://127.0.0.1:4999/, every
//     event) with PUT /api/config, only when no target is configured.
// Seeding refuses to run while the orchestrator is on (Settings → Orchestrator
// → Enable automatic agent launches, `orchestrator.mode`): a card moved into
// Plan, Execute or Review would start an agent. Switch it off for the capture.
//
// What --seed cannot do: the API has no way to make a workspace or an agent
// session (only the orchestrator starts those), to file an RCA in the
// knowledge store (that happens when a real planning verdict lands), or to
// record a Test stand-up run without running the repository's scripts. So
// workspace-review is skipped when no workspace exists, knowledge shows the
// store as it is, and quickstart-stand-up shows the Scripts card with its
// panel as the tester left it (run Test stand-up first for the full shot).
//
// Theme: the app follows `config.theme`, and only SYSTEM follows the
// browser. The script answers the page's GET /api/info with the theme set to
// SYSTEM and emulates prefers-color-scheme, so both variants are taken
// without changing the saved setting.
//
// Masking: before each capture, any text node and any input or textarea value
// containing the home directory, `owner@localhost`, `git config user.email`
// or a --mask value is covered by an absolutely positioned
// `div.prepcook-shot-mask` the script adds to <body> (and removes after).
// Nothing in the app's own DOM is changed.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const FIXTURES = path.join(ROOT, "scripts", "fixtures");
const VIEWPORT = { width: 1440, height: 900 };
const SCALE = 2;
const THEMES = ["light", "dark"];
const BOARD_NAME = "Pantry";
const BOARD_COLOR = "145 45% 38%";
const WEBHOOK_URL = "http://127.0.0.1:4999/";
const MASK_CLASS = "prepcook-shot-mask";

// The six seeded cards. `column` is the column's status word
// (crates/services/src/services/cards.rs `status_word`; Shipped is
// `pushed_mr`), and `metadata` builds what the card's state needs on top of
// what filing it wrote.
const CARDS = {
  backlog: {
    column: "backlog",
    title: "Let an owner stop sharing a list",
    work_class: "story",
    description:
      'Once a list is shared there is no way to take it back: the help pages tell people to make a new list instead. An owner should be able to remove someone from a list under **Share**, after which the list leaves that person\'s "Shared with me".',
  },
  planning: {
    column: "planning",
    title: "Sharing a list fails for a user who exists",
    work_class: "bug",
    // The description is the demo repository's BUG.md, below its title.
    comments: ["pantry-reproduction.md", "pantry-rca.md"],
    // Waiting at the gate: the RCA's verdict is in and nobody approved it.
    metadata: (now) => ({
      planning: {
        kind: "rca",
        confidence: "high",
        open_questions: 0,
        summary:
          "Sharing looks the email up case-sensitively, so a differently capitalised address finds nobody.",
        recorded_at: now,
      },
    }),
  },
  triaged: {
    // Triaged is the card's state: it stays in Plan.
    column: "planning",
    title: "Items my partner ticked off still show as to buy",
    work_class: "investigation",
    origin: {
      kind: "tammy",
      ref: "https://pantry-household.slack.com/archives/C0PANTRYHELP/p1760000000000100",
      reporter: "Ann",
    },
    description:
      'From #pantry-help: "Ben was in the shop ticking things off on his phone, and on my laptop at home the list still said everything was to buy. Is the sharing broken?"',
    comments: ["pantry-triage-report.md"],
    metadata: (now) => ({
      triage: {
        report: triageReport(fixture("pantry-triage-report.md", {})),
        confidence: "high",
        recorded_at: now,
        notified_at: now,
        completed_at: now,
      },
    }),
  },
  executing: {
    column: "executing",
    title: "Quantity 0 is saved as 1 without a word",
    work_class: "bug",
    description:
      'Adding "Eggs" with quantity 0 saves it with quantity 1 and says nothing. The form should refuse the 0 and say why, as it does for a blank name.',
  },
  reviewing: {
    column: "reviewing",
    title: "Renaming a list to a blank name does nothing and says nothing",
    work_class: "bug",
    description:
      "Under **Manage**, renaming a list to spaces only reloads the page with the old name and no message. It should keep the old name and say that a list needs a name.",
    comments: ["pantry-review.md"],
  },
  done: {
    column: "pushed_mr",
    title: "Sign-in rejects an email typed with a trailing space",
    work_class: "bug",
    description:
      'Pasting the email from the welcome message adds a trailing space, and sign-in answers "Wrong email or password."',
    pullRequest: { number: 1, status: "merged" },
    metadata: (now) => ({ merged_at: now }),
  },
};

// The columns a card walks through, in order: the workflow moves one column
// at a time.
const WALK = ["backlog", "planning", "executing", "reviewing", "pushed_mr"];

// Every shot: the file stem, what it shows, where it is used, and how to
// reach it. `take` gets the page and the resolved board.
const SHOTS = [
  {
    name: "board",
    shows:
      "The Pantry board: To Do, Plan (a bug at the gate and a triaged investigation), Execute, Review and Shipped (merged)",
    usedIn: "README: under the opening paragraph",
    take: async (page, ctx) => {
      requireBoard(ctx);
      await open(page, ctx, `/projects/${ctx.board.id}`);
      await settle(page, () =>
        page.getByText(CARDS.planning.title, { exact: true }).first(),
      );
    },
  },
  {
    name: "card-rca",
    shows: "The Plan card open, its RCA comment in view",
    usedIn: "README: Quickstart, step 3 (Plan)",
    take: async (page, ctx) => {
      const card = requireCard(ctx, "planning");
      await open(page, ctx, `/projects/${ctx.board.id}/issues/${card.id}`);
      await settle(
        page,
        () => page.getByText("RCA: sharing looks the email up").first(),
        { scroll: true },
      );
    },
  },
  {
    name: "card-triaged",
    shows:
      "The triaged investigation in Plan, its amber header and TRIAGE REPORT in view",
    usedIn: "none yet (docs/method.md#investigations)",
    take: async (page, ctx) => {
      const card = requireCard(ctx, "triaged");
      await open(page, ctx, `/projects/${ctx.board.id}/issues/${card.id}`);
      await settle(page, () => page.getByText("TRIAGE REPORT").first(), {
        scroll: true,
      });
    },
  },
  {
    name: "workspace-review",
    shows: "A workspace's Review panel (only when a workspace exists)",
    usedIn: "none yet (docs/review.md)",
    take: async (page, ctx) => {
      const workspace = await latestWorkspace(ctx);
      if (!workspace) {
        throw new Skip(
          "no workspace exists: only the orchestrator makes one; work a card through Plan first",
        );
      }
      await open(page, ctx, `/workspaces/${workspace.id}`);
      await settle(page);
      // `v r` toggles the Review panel (packages/web-core/src/shared/keyboard).
      await page.keyboard.press("v");
      await page.keyboard.press("r");
      await settle(page);
    },
  },
  {
    name: "settings-integrations",
    shows: "Settings → Integrations → Notifications, with one webhook target",
    usedIn: "README: Integrations",
    take: (page, ctx) => openSettings(page, ctx, "Notifications"),
  },
  {
    name: "settings-ticketing",
    shows: "Settings → Integrations → Ticketing",
    usedIn: "none yet (docs/incidents.md)",
    take: (page, ctx) => openSettings(page, ctx, "Ticketing"),
  },
  {
    name: "settings-evidence",
    shows: "Settings → Integrations → Evidence",
    usedIn: "none yet (docs/evidence.md)",
    take: (page, ctx) => openSettings(page, ctx, "Evidence"),
  },
  {
    name: "settings-agents",
    shows: "Settings → Integrations → Agents",
    usedIn: "none yet (docs/configuration.md#agents)",
    take: (page, ctx) => openSettings(page, ctx, "Agents"),
  },
  {
    name: "settings-repositories",
    shows: "Settings → Repositories, the demo repository and its Scripts card",
    usedIn: "none yet (docs/scripts.md)",
    take: async (page, ctx) => {
      await openSettings(page, ctx, "Repositories");
      await selectRepo(page, ctx);
      await settle(page, () => page.getByText("App steps").first(), {
        scroll: true,
      });
    },
  },
  {
    name: "quickstart-stand-up",
    shows:
      "The Scripts card with the Test stand-up panel (after a run, when the tester has made one)",
    usedIn: "README: Quickstart, step 2",
    take: async (page, ctx) => {
      await openSettings(page, ctx, "Repositories");
      await selectRepo(page, ctx);
      const panel = page.getByText(/Test run: stand-up/).first();
      const button = page.getByRole("button", { name: "Test stand-up" });
      await settle(
        page,
        async () => ((await panel.count()) ? panel : button.first()),
        { scroll: true },
      );
    },
  },
  {
    name: "knowledge",
    shows: "The board’s Knowledge page (the RCA graph)",
    usedIn: "none yet (docs/rca-graph.md)",
    take: async (page, ctx) => {
      requireBoard(ctx);
      await open(page, ctx, `/projects/${ctx.board.id}/knowledge`);
      await settle(page, () => page.getByText(/Knowledge/).first());
    },
  },
];

class Skip extends Error {}

class Fatal extends Error {
  constructor(message, code = 1) {
    super(message);
    this.code = code;
  }
}

function usage() {
  return `usage: node scripts/screenshots.mjs [--seed] [--url <server>] [--repo <path>]
       [--only <shot,…>] [--out <dir>] [--chromium <path>] [--mask <text>]… [--list]`;
}

function parseOptions() {
  let parsed;
  try {
    // `pnpm screenshots -- --seed` hands the script the `--` too.
    const args = process.argv.slice(2);
    if (args[0] === "--") args.shift();
    parsed = parseArgs({
      args,
      options: {
        url: { type: "string" },
        seed: { type: "boolean", default: false },
        repo: { type: "string", default: path.join("..", "prepcook-demo") },
        only: { type: "string" },
        out: { type: "string", default: path.join("docs", "images") },
        chromium: { type: "string" },
        mask: { type: "string", multiple: true, default: [] },
        list: { type: "boolean", default: false },
        help: { type: "boolean", short: "h", default: false },
      },
    }).values;
  } catch (error) {
    throw new Fatal(`${error.message}\n${usage()}`, 2);
  }
  const known = new Set(SHOTS.map((shot) => shot.name));
  const only = parsed.only
    ? parsed.only
        .split(",")
        .map((name) => name.trim())
        .filter(Boolean)
    : [];
  const unknown = only.filter((name) => !known.has(name));
  if (unknown.length) {
    throw new Fatal(
      `unknown shot ${unknown.join(", ")}; the shots are ${[...known].join(", ")}`,
      2,
    );
  }
  return {
    ...parsed,
    // Relative paths are taken from the repository root, so the script
    // behaves the same from `pnpm screenshots` and from any directory.
    repo: path.resolve(ROOT, parsed.repo),
    out: path.resolve(ROOT, parsed.out),
    shots: only.length
      ? SHOTS.filter((shot) => only.includes(shot.name))
      : SHOTS,
  };
}

function printList(shots, out) {
  const rel = path.relative(process.cwd(), out) || ".";
  console.log(
    `${shots.length} shot(s), each light then dark, ${VIEWPORT.width}×${VIEWPORT.height} at ${SCALE}x, into ${rel}/:\n`,
  );
  for (const shot of shots) {
    console.log(`${shot.name}`);
    console.log(`  files:   ${shot.name}.png, ${shot.name}-dark.png`);
    console.log(`  shows:   ${shot.shows}`);
    console.log(`  used in: ${shot.usedIn}`);
  }
}

// --- The server -----------------------------------------------------------

function serverFromStatus() {
  let output;
  try {
    output = execFileSync("prepcook", ["status"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 15_000,
    });
  } catch (error) {
    const why =
      error.code === "ENOENT"
        ? "`prepcook` is not on PATH"
        : `\`prepcook status\` failed: ${error.message.split("\n")[0]}`;
    throw new Fatal(`${why}; pass the server with --url`, 2);
  }
  const running = output.match(/^server:\s+running on port \d+ \((\S+)\)/m);
  if (!running) {
    const line =
      output.split("\n").find((l) => l.startsWith("server:")) ?? output.trim();
    throw new Fatal(
      `Prepcook's server is not running (prepcook status: ${line.trim()}). Start it with \`prepcook up\`, or pass --url.`,
      2,
    );
  }
  return running[1];
}

function api(base, token) {
  async function call(method, route, { body, auth = false } = {}) {
    const headers = { Accept: "application/json" };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    if (auth) headers.Authorization = `Bearer ${token}`;
    let response;
    try {
      response = await fetch(new URL(route, base), {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
      });
    } catch (error) {
      throw new Fatal(
        `${method} ${route}: the server at ${base} did not answer (${error.cause?.code ?? error.cause?.errors?.[0]?.code ?? error.name})`,
        2,
      );
    }
    const text = await response.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      // A non-JSON answer is reported with its status below.
    }
    return { status: response.status, json, text };
  }
  async function data(method, route, options) {
    const { status, json, text } = await call(method, route, options);
    if (status >= 400 || (json && json.success === false)) {
      const why = json?.message ?? text.slice(0, 200);
      throw new Fatal(`${method} ${route} answered ${status}: ${why}`);
    }
    return json?.data ?? json;
  }
  return { call, data };
}

async function checkServer(client, base) {
  const health = await client.call("GET", "/api/health");
  if (health.status !== 200) {
    throw new Fatal(
      `${base} answered ${health.status} to /api/health; is it a Prepcook server?`,
      2,
    );
  }
  // A card id that cannot exist: 404 means the token was accepted, 401 that
  // it was not.
  const probe = await client.call(
    "GET",
    "/api/cards/00000000-0000-0000-0000-000000000000",
    { auth: true },
  );
  if (probe.status === 401) {
    throw new Fatal(
      `the server refused PREPCOOK_API_TOKEN: ${probe.json?.message ?? "unauthorised"}. Use the value stored with \`prepcook secret set PREPCOOK_API_TOKEN\`.`,
      2,
    );
  }
}

// --- Seeding ----------------------------------------------------------------

// The column a status word names (cards.rs `status_word`).
function statusWord(column) {
  const name = column.trim().toLowerCase();
  if (["to do", "unassigned", "backlog"].includes(name)) return "backlog";
  if (["plan", "planning"].includes(name)) return "planning";
  if (name === "triaged") return "triaged";
  if (["execute", "executing"].includes(name)) return "executing";
  if (["review", "reviewing"].includes(name)) return "reviewing";
  if (["shipped", "done", "push mr", "pushed mr"].includes(name))
    return "pushed_mr";
  if (["close", "closed", "deployed"].includes(name)) return "closed";
  if (["clean up", "clean-up", "cleanup"].includes(name)) return "cleanup";
  return "other";
}

async function findBoard(client) {
  const { projects } = await client.data("GET", "/api/remote/projects");
  return projects.find((project) => project.name === BOARD_NAME) ?? null;
}

async function createBoard(client, info) {
  const base = info.shared_api_base;
  if (!base) throw new Fatal("the server reports no board address");
  const { organizations } = await client.data("GET", "/api/organizations");
  if (!organizations?.length) throw new Fatal("the board has no organization");
  const { access_token: ownerToken } = await client.data(
    "GET",
    "/api/auth/token",
  );
  const response = await fetch(new URL("/v1/projects", base), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${ownerToken}`,
    },
    body: JSON.stringify({
      organization_id: organizations[0].id,
      name: BOARD_NAME,
      color: BOARD_COLOR,
    }),
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) {
    throw new Fatal(
      `the board refused to create "${BOARD_NAME}" (${response.status}); add it under Settings → Projects and run again`,
    );
  }
  const board = await findBoard(client);
  if (!board) throw new Fatal(`"${BOARD_NAME}" was created but is not listed`);
  return board;
}

// Records the card's pull request on the board service, the way the
// orchestrator does once it has opened one. The URL names the demo
// repository's remote; nothing on the host is read.
async function recordPullRequest(client, info, repo, issueId, card) {
  const base = info.shared_api_base;
  if (!base) throw new Fatal("the server reports no board address");
  const { access_token: ownerToken } = await client.data(
    "GET",
    "/api/auth/token",
  );
  const { number, status } = card.pullRequest;
  const response = await fetch(new URL("/v1/pull_requests", base), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${ownerToken}`,
    },
    body: JSON.stringify({
      url: `${repoWebUrl(repo)}/pull/${number}`,
      number,
      status,
      merged_at: status === "merged" ? new Date().toISOString() : null,
      merge_commit_sha: null,
      target_branch_name: "main",
      issue_id: issueId,
    }),
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) {
    console.warn(
      `seed: the board refused the pull request for "${card.title}" (${response.status}); the card shows no chip`,
    );
  }
}

// The demo repository's web URL from its `origin`, for the seeded pull
// request's link.
function repoWebUrl(repo) {
  try {
    const remote = execFileSync(
      "git",
      ["-C", repo, "remote", "get-url", "origin"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    ).trim();
    const match = remote.match(/github\.com[:/]([^/]+\/[^/.]+)/);
    if (match) return `https://github.com/${match[1]}`;
  } catch {
    // Falls through to the upstream demo repository.
  }
  return "https://github.com/nwisehart/prepcook-demo";
}

// A TRIAGE REPORT comment's sections, as the orchestrator records them
// (`parse_triage_report`).
function triageReport(markdown) {
  const section = (heading) => {
    const match = markdown.match(
      new RegExp(`## ${heading}\\n+([\\s\\S]*?)(?=\\n## |$)`),
    );
    return match ? match[1].trim() : "";
  };
  return {
    what_happens: section("What happens"),
    where: section("Where"),
    why: section("Why"),
    scope: section("Scope"),
    workaround: section("Workaround"),
    confidence: section("Confidence"),
    markdown: markdown.trim(),
  };
}

async function columns(client, boardId) {
  const { project_statuses: statuses } = await client.data(
    "GET",
    `/api/remote/project-statuses?project_id=${boardId}`,
  );
  const byWord = new Map();
  for (const status of [...statuses].sort(
    (a, b) => a.sort_order - b.sort_order,
  )) {
    const word = statusWord(status.name);
    if (!byWord.has(word)) byWord.set(word, status);
  }
  return byWord;
}

async function boardIssues(client, boardId) {
  const { issues } = await client.data(
    "GET",
    `/api/remote/issues?project_id=${boardId}`,
  );
  return issues;
}

function fixture(name, vars) {
  const text = readFileSync(path.join(FIXTURES, name), "utf8");
  return text.replace(/\{\{(\w+)\}\}/g, (whole, key) => vars[key] ?? whole);
}

function bugReport(repo) {
  const file = path.join(repo, "BUG.md");
  if (!existsSync(file)) {
    throw new Fatal(
      `${file} is missing: --repo must point at a clone of prepcook-demo`,
    );
  }
  return readFileSync(file, "utf8").replace(/^# [^\n]*\n+/, "");
}

function repoCommit(repo) {
  try {
    return execFileSync("git", ["-C", repo, "rev-parse", "--short", "HEAD"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "HEAD";
  }
}

async function ensureRepo(client, repo) {
  const repos = await client.data("GET", "/api/repos");
  const known = repos.find((r) => path.resolve(r.path) === repo);
  if (known) return known;
  console.log(`seed: registering ${path.basename(repo)}`);
  return client.data("POST", "/api/repos", { body: { path: repo } });
}

async function ensureWebhookTarget(client, config) {
  if (config.notifications?.targets?.length) return;
  console.log(`seed: adding a webhook notification target (${WEBHOOK_URL})`);
  const next = structuredClone(config);
  next.notifications = {
    ...next.notifications,
    targets: [{ kind: "webhook", url: WEBHOOK_URL, events: ["*"] }],
  };
  await client.data("PUT", "/api/config", { body: next });
}

async function seed(client, opts) {
  const info = await client.data("GET", "/api/info");
  const mode = info.config?.orchestrator?.mode;
  if (mode && mode !== "off") {
    throw new Fatal(
      `the orchestrator is on (orchestrator.mode = "${mode}"), and the seeded cards in Plan, Execute and Review would start agents. Switch off Settings → Orchestrator → Enable automatic agent launches, save, and run again.`,
    );
  }
  if (!existsSync(opts.repo)) {
    throw new Fatal(
      `--repo ${opts.repo} does not exist; clone prepcook-demo there or pass its path`,
    );
  }
  const repo = await ensureRepo(client, opts.repo);
  let board = await findBoard(client);
  if (!board) {
    console.log(`seed: creating the "${BOARD_NAME}" board`);
    board = await createBoard(client, info);
  }
  const byWord = await columns(client, board.id);
  const existing = new Set(
    (await boardIssues(client, board.id)).map((issue) => issue.title),
  );
  const vars = {
    commit: repoCommit(opts.repo),
    now: `${new Date().toISOString().slice(0, 16).replace("T", " ")} UTC`,
  };

  for (const [role, card] of Object.entries(CARDS)) {
    if (existing.has(card.title)) {
      console.log(`seed: ${role}: "${card.title}" is already on the board`);
      continue;
    }
    const column = byWord.get(card.column);
    if (!column) {
      console.warn(
        `seed: ${role}: the board has no ${card.column} column; "${card.title}" stays in To Do`,
      );
    }
    const created = await client.data("POST", "/api/cards", {
      auth: true,
      body: {
        project_id: board.id,
        title: card.title,
        description:
          role === "planning" ? bugReport(opts.repo) : card.description,
        work_class: card.work_class,
        origin: card.origin ?? {
          kind: "api",
          ref: "pantry-feedback",
          reporter: "Ben",
        },
        metadata: { seeded_by: "scripts/screenshots.mjs" },
        start: false,
      },
    });
    if (column) {
      for (const word of WALK.slice(1, WALK.indexOf(card.column) + 1)) {
        const step = byWord.get(word);
        if (!step) continue;
        await client.data("PATCH", `/api/remote/issues/${created.id}`, {
          body: { status_id: step.id },
        });
      }
    }
    if (card.pullRequest) {
      await recordPullRequest(client, info, opts.repo, created.id, card);
    }
    if (card.metadata) {
      const issue = (await boardIssues(client, board.id)).find(
        (i) => i.id === created.id,
      );
      await client.data("PATCH", `/api/remote/issues/${created.id}`, {
        body: {
          extension_metadata: {
            ...(issue?.extension_metadata ?? {}),
            ...card.metadata(new Date().toISOString()),
          },
        },
      });
    }
    for (const name of card.comments ?? []) {
      await client.data("POST", "/api/remote/issue_comments", {
        body: {
          issue_id: created.id,
          message: fixture(name, { ...vars, key: created.key }),
          parent_id: null,
        },
      });
    }
    console.log(
      `seed: ${role}: filed ${created.key} "${card.title}" in ${column?.name ?? "To Do"}`,
    );
  }
  await ensureWebhookTarget(client, info.config);
  return repo;
}

// --- Resolving what the shots show ------------------------------------------

async function resolveContext(client, opts) {
  const board = await findBoard(client);
  const cards = {};
  if (board) {
    const issues = await boardIssues(client, board.id);
    for (const [role, card] of Object.entries(CARDS)) {
      const issue = issues.find((i) => i.title === card.title);
      if (issue) cards[role] = issue;
    }
  }
  const repos = await client.data("GET", "/api/repos").catch(() => []);
  const repo =
    repos.find((r) => path.resolve(r.path) === opts.repo) ?? repos[0] ?? null;
  return { client, board, cards, repo };
}

function requireBoard(ctx) {
  if (!ctx.board) {
    throw new Skip(`there is no "${BOARD_NAME}" board; run with --seed`);
  }
}

function requireCard(ctx, role) {
  requireBoard(ctx);
  const card = ctx.cards[role];
  if (!card) {
    throw new Skip(
      `"${CARDS[role].title}" is not on the board; run with --seed`,
    );
  }
  return card;
}

async function latestWorkspace(ctx) {
  const workspaces = await ctx.client
    .data("GET", "/api/workspaces")
    .catch(() => []);
  return (
    (workspaces ?? [])
      .filter((w) => !w.archived)
      .sort((a, b) => String(b.updated_at).localeCompare(a.updated_at))[0] ??
    null
  );
}

// --- Driving the page ---------------------------------------------------------

async function open(page, ctx, route) {
  await page.goto(new URL(route, ctx.base).toString(), {
    waitUntil: "domcontentloaded",
  });
}

// Waits for the page to show `target` (a locator, or a function returning
// one), then for the network and animations to quiet down. A target that
// never shows is a warning, not a failure: the capture still says what the
// page looked like.
async function settle(page, target, { scroll = false } = {}) {
  await page
    .waitForLoadState("networkidle", { timeout: 5_000 })
    .catch(() => {});
  if (target) {
    try {
      const locator = await target();
      await locator.waitFor({ state: "visible", timeout: 15_000 });
      if (scroll) await locator.scrollIntoViewIfNeeded();
    } catch {
      page.prepcookWarnings.push("the expected content did not appear");
    }
  }
  await page.waitForTimeout(800);
}

async function openSettings(page, ctx, section) {
  await open(page, ctx, ctx.board ? `/projects/${ctx.board.id}` : "/");
  await settle(page);
  const nav = page
    .locator("nav")
    .getByRole("button", { name: section, exact: true });
  // `g s` opens Settings from anywhere in the app.
  await page.keyboard.press("Escape");
  await page.keyboard.press("g");
  await page.keyboard.press("s");
  try {
    await nav.first().waitFor({ state: "visible", timeout: 10_000 });
  } catch {
    throw new Error("Settings did not open (the `g s` shortcut)");
  }
  await nav.first().click();
  await settle(page);
}

// The Repositories section picks the repository through a dropdown: its
// trigger shows the placeholder (or the current name) and the names appear
// only once the menu is open.
async function selectRepo(page, ctx) {
  if (!ctx.repo) return;
  const name = ctx.repo.display_name || path.basename(ctx.repo.path);
  const chosen = page.getByRole("button", { name, exact: true }).first();
  if (await chosen.count()) return; // already selected
  const trigger = page
    .getByRole("button", { name: "Choose a repository to configure" })
    .first();
  if (!(await trigger.count())) return;
  await trigger.click();
  const item = page.getByRole("menuitem", { name, exact: true }).first();
  if (await item.count()) {
    await item.click();
    await settle(page);
  } else {
    await page.keyboard.press("Escape").catch(() => {});
  }
}

// --- Masking --------------------------------------------------------------

function maskNeedles(extra) {
  const needles = new Set(["owner@localhost", ...extra]);
  const home = os.homedir();
  if (home && home !== "/") needles.add(home);
  try {
    const email = execFileSync("git", ["config", "--get", "user.email"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    if (email) needles.add(email);
  } catch {
    // No git identity: nothing to mask.
  }
  return [...needles].filter((needle) => needle.length > 2);
}

// Runs in the page: covers every occurrence of a needle with an overlay.
function applyMasks({ needles, className }) {
  const rects = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    for (const needle of needles) {
      let at = node.data.indexOf(needle);
      while (at !== -1) {
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + needle.length);
        rects.push(...range.getClientRects());
        at = node.data.indexOf(needle, at + needle.length);
      }
    }
  }
  for (const field of document.querySelectorAll("input, textarea")) {
    if (needles.some((needle) => String(field.value).includes(needle))) {
      rects.push(field.getBoundingClientRect());
    }
  }
  for (const rect of rects) {
    if (!rect.width || !rect.height) continue;
    const cover = document.createElement("div");
    cover.className = className;
    Object.assign(cover.style, {
      position: "fixed",
      left: `${rect.left - 2}px`,
      top: `${rect.top - 1}px`,
      width: `${rect.width + 4}px`,
      height: `${rect.height + 2}px`,
      background:
        "repeating-linear-gradient(45deg, #8a8a8a 0 6px, #9a9a9a 6px 12px)",
      borderRadius: "3px",
      zIndex: "2147483647",
      pointerEvents: "none",
    });
    document.body.appendChild(cover);
  }
  return rects.length;
}

function removeMasks(className) {
  for (const cover of document.querySelectorAll(`div.${className}`)) {
    cover.remove();
  }
}

// --- Capturing -------------------------------------------------------------

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    throw new Fatal(
      "Playwright is not installed: run `pnpm install`, then `npx playwright install chromium`",
      2,
    );
  }
}

async function capture(opts, ctx) {
  const { chromium } = await loadPlaywright();
  let browser;
  try {
    browser = await chromium.launch({ executablePath: opts.chromium });
  } catch (error) {
    throw new Fatal(
      `Chromium did not start: ${error.message.split("\n")[0]}. Run \`npx playwright install chromium\`, or pass a Chromium build with --chromium.`,
      2,
    );
  }
  mkdirSync(opts.out, { recursive: true });
  const needles = maskNeedles(opts.mask);
  const results = [];
  try {
    for (const theme of THEMES) {
      const context = await browser.newContext({
        viewport: VIEWPORT,
        deviceScaleFactor: SCALE,
        colorScheme: theme,
        // Dialogs fade in; a capture must not catch one halfway.
        reducedMotion: "reduce",
      });
      // The app applies config.theme, and only SYSTEM follows the browser.
      await context.route("**/api/info", async (route) => {
        const response = await route.fetch();
        let json;
        try {
          json = await response.json();
        } catch {
          return route.fulfill({ response });
        }
        if (json?.data?.config) json.data.config.theme = "SYSTEM";
        await route.fulfill({ response, json });
      });
      const page = await context.newPage();
      for (const shot of opts.shots) {
        const file = path.join(
          opts.out,
          `${shot.name}${theme === "dark" ? "-dark" : ""}.png`,
        );
        const label = `${shot.name} (${theme})`;
        page.prepcookWarnings = [];
        try {
          await shot.take(page, ctx);
          const masked = await page.evaluate(applyMasks, {
            needles,
            className: MASK_CLASS,
          });
          await page.screenshot({ path: file });
          await page.evaluate(removeMasks, MASK_CLASS);
          const notes = [...page.prepcookWarnings];
          if (masked) notes.push(`${masked} area(s) masked`);
          console.log(
            `captured ${label} → ${path.relative(process.cwd(), file)}${notes.length ? ` (${notes.join("; ")})` : ""}`,
          );
          results.push({ shot: shot.name, theme, ok: true });
        } catch (error) {
          if (error instanceof Skip) {
            console.log(`skipped ${label}: ${error.message}`);
            results.push({ shot: shot.name, theme, skipped: true });
          } else {
            console.error(`failed ${label}: ${error.message.split("\n")[0]}`);
            results.push({ shot: shot.name, theme, ok: false });
          }
        }
        // Close whatever the shot opened (a dialog, a panel) before the next.
        await page.keyboard.press("Escape").catch(() => {});
      }
      await context.close();
    }
  } finally {
    await browser.close();
  }
  return results;
}

async function main() {
  const opts = parseOptions();
  if (opts.help) {
    console.log(usage());
    return 0;
  }
  if (opts.list) {
    printList(opts.shots, opts.out);
    return 0;
  }
  const token = process.env.PREPCOOK_API_TOKEN?.trim();
  if (!token) {
    throw new Fatal(
      "PREPCOOK_API_TOKEN is not set: export the value you stored with `prepcook secret set PREPCOOK_API_TOKEN`",
      2,
    );
  }
  const base = opts.url ?? serverFromStatus();
  const client = api(base, token);
  await checkServer(client, base);
  if (opts.seed) await seed(client, opts);
  const ctx = { ...(await resolveContext(client, opts)), base };
  const results = await capture(opts, ctx);
  const failed = results.filter((r) => r.ok === false);
  const skipped = results.filter((r) => r.skipped);
  const taken = results.filter((r) => r.ok);
  console.log(
    `\n${taken.length} captured, ${skipped.length} skipped, ${failed.length} failed`,
  );
  return failed.length ? 1 : 0;
}

main().then(
  (code) => process.exit(code),
  (error) => {
    console.error(
      `screenshots: ${error instanceof Fatal ? error.message : error.stack}`,
    );
    process.exit(error instanceof Fatal ? error.code : 1);
  },
);
