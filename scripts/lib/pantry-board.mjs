// The Pantry sample board and the browser plumbing that scripts/screenshots.mjs
// and scripts/board-video.mjs share: the server and its API, the seeding
// (`--seed` in both, through the local server's API only; screenshots.mjs's
// header says what it files and what it cannot), the context the board is
// resolved into, page driving, masking, and Chromium with the app following
// the browser's colour scheme.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
);
const FIXTURES = path.join(ROOT, "scripts", "fixtures");
export const VIEWPORT = { width: 1440, height: 900 };
export const SCALE = 2;
export const BOARD_NAME = "Pantry";
const BOARD_COLOR = "145 45% 38%";
const WEBHOOK_URL = "http://127.0.0.1:4999/";
export const MASK_CLASS = "prepcook-shot-mask";
// Every seeded card carries this marker under its metadata, whichever script
// seeded it; refiling a card deletes it only when the marker is there.
const SEEDED_BY = "scripts/screenshots.mjs";

// The six seeded cards. `column` is the column's status word
// (crates/services/src/services/cards.rs `status_word`; Shipped is
// `pushed_mr`), and `metadata` builds what the card's state needs on top of
// what filing it wrote.
export const CARDS = {
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
export const WALK = [
  "backlog",
  "planning",
  "executing",
  "reviewing",
  "pushed_mr",
];

export class Skip extends Error {}

export class Fatal extends Error {
  constructor(message, code = 1) {
    super(message);
    this.code = code;
  }
}

// --- The server -----------------------------------------------------------

export function serverFromStatus() {
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

export function api(base, token) {
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

export async function checkServer(client, base) {
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
export function statusWord(column) {
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

export async function findBoard(client) {
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
// repository's remote; nothing on the host is read. A merged one is dated
// `mergedAt` (the card's own `merged_at`, so the chip and the status line
// agree), else now. True when the board took it.
async function recordPullRequest(client, info, repo, issueId, card, mergedAt) {
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
      merged_at:
        status === "merged" ? (mergedAt ?? new Date().toISOString()) : null,
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
  return response.ok;
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
export function triageReport(markdown) {
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

export async function columns(client, boardId) {
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

export async function boardIssues(client, boardId) {
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

// --- What a seeded card holds -----------------------------------------------

// The card's pull requests as the board service lists them for it (the
// board's own pull-request rows, which the card's chip is read from).
export async function cardPullRequests(client, issueId) {
  const { pull_requests: pullRequests } = await client.data(
    "GET",
    `/api/remote/pull-requests?issue_id=${issueId}`,
  );
  return pullRequests ?? [];
}

export async function cardComments(client, issueId) {
  const { issue_comments: comments } = await client.data(
    "GET",
    `/api/remote/issue_comments?issue_id=${issueId}`,
  );
  return comments ?? [];
}

function firstLine(text) {
  return (
    String(text ?? "")
      .split("\n")
      .find((line) => line.trim())
      ?.trim() ?? ""
  );
}

// A seeded comment's first line, which names it ("REPRODUCTION:", "RCA: …",
// "TRIAGE REPORT", "REVIEW: APPROVE") and holds no fixture variable.
export function commentHeading(name) {
  return firstLine(fixture(name, {}));
}

// The metadata keys the seed writes for `card` and the values it gives
// them: the work class filing records (the triaged state and the plan gate
// both read it) and whatever `card.metadata` builds.
export function seededMetadata(card, now) {
  return { work_class: card.work_class, ...(card.metadata?.(now) ?? {}) };
}

// What the board lacks of what the seed defines for `card`, given what the
// card holds: its `metadata` (extension_metadata), its `comments` and its
// `pullRequests`. `metadata` lists the missing keys (a key that is absent or
// null, or a work class other than the seeded one); `comments` the fixture
// names with no comment of the same first line; `pullRequest` is true when
// the seeded pull request's number is not among the card's. Values the card
// holds are never compared beyond that, so a card that has moved on (its
// plan decided, its report edited) is not rewritten.
export function missingState(card, { metadata, comments, pullRequests }) {
  const held = metadata ?? {};
  const missing = { metadata: [], comments: [], pullRequest: false };
  for (const key of Object.keys(seededMetadata(card, ""))) {
    const ok =
      key === "work_class"
        ? held.work_class === card.work_class
        : held[key] !== undefined && held[key] !== null;
    if (!ok) missing.metadata.push(key);
  }
  const headings = new Set((comments ?? []).map((c) => firstLine(c.message)));
  for (const name of card.comments ?? []) {
    if (!headings.has(commentHeading(name))) missing.comments.push(name);
  }
  if (card.pullRequest) {
    missing.pullRequest = !(pullRequests ?? []).some(
      (pr) => pr.number === card.pullRequest.number,
    );
  }
  return missing;
}

export function isComplete(missing) {
  return (
    !missing.metadata.length && !missing.comments.length && !missing.pullRequest
  );
}

// The missing pieces in words, for the seed's log line.
export function describeMissing(card, missing) {
  const parts = [];
  if (missing.metadata.length) {
    parts.push(`metadata ${missing.metadata.join(", ")}`);
  }
  for (const name of missing.comments) {
    parts.push(`the "${commentHeading(name)}" comment`);
  }
  if (missing.pullRequest) {
    const { number, status } = card.pullRequest;
    parts.push(`its ${status} pull request #${number}`);
  }
  return parts.join(", ");
}

// Whether the card's plan waits at the gate, so Plan offers Approve: the
// rules of cardSignals.ts `pendingPlanGate` (a planning verdict, no approved
// spec, no later decision that sent it back, not an investigation, not
// taken over).
export function planGatePending(meta) {
  if (!meta || meta.work_class === "investigation") return false;
  if (meta.human_takeover?.detected_at) return false;
  const planning = meta.planning;
  if (!planning || typeof planning !== "object") return false;
  if (meta.spec?.approved_at) return false;
  const recordedAt = Date.parse(planning.recorded_at);
  const approvals = Array.isArray(meta.approvals) ? meta.approvals : [];
  const latest = approvals[approvals.length - 1];
  if (
    latest &&
    typeof latest === "object" &&
    latest.decision !== "approve" &&
    (Number.isNaN(recordedAt) || (Date.parse(latest.at) || 0) >= recordedAt)
  ) {
    return false;
  }
  return true;
}

// The pull requests taken together as the board's chip reads them
// (workflowTransitions.ts `prState`): any open one makes the card "open",
// else any merged one "merged", else "closed"; null with none.
export function pullRequestState(pullRequests) {
  if (!pullRequests?.length) return null;
  if (pullRequests.some((pr) => pr.status === "open")) return "open";
  if (pullRequests.some((pr) => pr.status === "merged")) return "merged";
  return "closed";
}

// Writes back what `missing` names on the seeded card `issue`, and returns
// what it restored in words (empty when the board already held it all).
async function restoreSeeded(client, info, opts, issue, card, missing, vars) {
  const restored = { ...missing, pullRequest: false };
  const held = issue.extension_metadata ?? {};
  if (missing.metadata.length) {
    const seeded = seededMetadata(card, new Date().toISOString());
    const next = { ...held };
    for (const key of missing.metadata) next[key] = seeded[key];
    await client.data("PATCH", `/api/remote/issues/${issue.id}`, {
      body: { extension_metadata: next },
    });
  }
  if (missing.pullRequest) {
    restored.pullRequest = await recordPullRequest(
      client,
      info,
      opts.repo,
      issue.id,
      card,
      typeof held.merged_at === "string" ? held.merged_at : undefined,
    );
  }
  for (const name of missing.comments) {
    await client.data("POST", "/api/remote/issue_comments", {
      body: {
        issue_id: issue.id,
        message: fixture(name, { ...vars, key: issue.simple_id }),
        parent_id: null,
      },
    });
  }
  return describeMissing(card, restored);
}

export async function seed(client, opts) {
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
  const issues = await boardIssues(client, board.id);
  // A seeded card left in another column (a film that stopped midway) is
  // filed again in its own, so every run starts from the board as seeded.
  const strays = misfiled(issues, byWord, Object.keys(CARDS));
  await removeSeeded(client, strays, ({ card, home, found }) =>
    console.log(
      `seed: ${home.name}: "${card.title}" was in ${found?.name ?? "another column"}; filed again`,
    ),
  );
  const refiled = new Set(strays.map(({ issue }) => issue.id));
  const existing = new Map(
    issues.filter((i) => !refiled.has(i.id)).map((i) => [i.title, i]),
  );
  const vars = {
    commit: repoCommit(opts.repo),
    now: `${new Date().toISOString().slice(0, 16).replace("T", " ")} UTC`,
  };

  for (const [role, card] of Object.entries(CARDS)) {
    const column = byWord.get(card.column);
    const issue = existing.get(card.title);
    if (issue) {
      // On the board in its column: what the seed wrote the first time can
      // still be gone (the board's database recreated while the cards came
      // back), so it is read back and whatever is missing written again.
      const missing = missingState(card, {
        metadata: issue.extension_metadata,
        comments: card.comments ? await cardComments(client, issue.id) : [],
        pullRequests: card.pullRequest
          ? await cardPullRequests(client, issue.id)
          : [],
      });
      const restored = isComplete(missing)
        ? ""
        : await restoreSeeded(client, info, opts, issue, card, missing, vars);
      console.log(
        restored
          ? `seed: ${column?.name ?? card.column}: "${card.title}" was missing ${restored}; restored`
          : `seed: ${role}: "${card.title}" is already on the board`,
      );
      continue;
    }
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
        metadata: { seeded_by: SEEDED_BY },
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

export async function resolveContext(client, opts) {
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

export function requireBoard(ctx) {
  if (!ctx.board) {
    throw new Skip(`there is no "${BOARD_NAME}" board; run with --seed`);
  }
}

export function requireCard(ctx, role) {
  requireBoard(ctx);
  const card = ctx.cards[role];
  if (!card) {
    throw new Skip(
      `"${CARDS[role].title}" is not on the board; run with --seed`,
    );
  }
  return card;
}

// --- Driving the page ---------------------------------------------------------

export async function open(page, ctx, route) {
  await page.goto(new URL(route, ctx.base).toString(), {
    waitUntil: "domcontentloaded",
  });
}

// Waits for the page to show `target` (a locator, or a function returning
// one), then for the network and animations to quiet down. A target that
// never shows is a warning, not a failure: the capture still says what the
// page looked like.
export async function settle(page, target, { scroll = false } = {}) {
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

// --- Masking --------------------------------------------------------------

export function maskNeedles(extra) {
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
export function applyMasks({ needles, className }) {
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

export function removeMasks(className) {
  for (const cover of document.querySelectorAll(`div.${className}`)) {
    cover.remove();
  }
}

// --- The browser -----------------------------------------------------------

export async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    throw new Fatal(
      "Playwright is not installed: run `pnpm install`, then `npx playwright install chromium`",
      2,
    );
  }
}

export async function launchChromium(executablePath) {
  const { chromium } = await loadPlaywright();
  try {
    return await chromium.launch({ executablePath });
  } catch (error) {
    throw new Fatal(
      `Chromium did not start: ${error.message.split("\n")[0]}. Run \`npx playwright install chromium\`, or pass a Chromium build with --chromium.`,
      2,
    );
  }
}

// A browser context at the board's size in `theme`, with `options` (such as
// recordVideo) on top. The app applies config.theme, and only SYSTEM follows
// the browser, so the page's GET /api/info is answered with the theme set to
// SYSTEM; the saved setting is never changed.
export async function newBoardContext(browser, theme, options = {}) {
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: SCALE,
    colorScheme: theme,
    ...options,
  });
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
  return context;
}

// --- Putting seeded cards back ----------------------------------------------

function seededMarker(issue) {
  const metadata = issue.extension_metadata ?? {};
  return metadata.external?.seeded_by ?? metadata.seeded_by ?? null;
}

// The seeded cards of `roles` that are on the board outside the column they
// were seeded into, each as { role, card, issue, home, found }: `home` is
// that column, `found` the one the card is in (null when it is not one of
// `byWord`). Only seeded titles are looked at; a card whose column the board
// lacks is never misfiled.
export function misfiled(issues, byWord, roles) {
  const out = [];
  for (const role of roles) {
    const card = CARDS[role];
    const issue = issues.find((i) => i.title === card.title);
    if (!issue) continue;
    const home = byWord.get(card.column);
    if (!home || issue.status_id === home.id) continue;
    const found =
      [...byWord.values()].find((s) => s.id === issue.status_id) ?? null;
    out.push({ role, card, issue, home, found });
  }
  return out;
}

// Deletes the misfiled cards so that seed() files them again as new, calling
// `log` for each. A card without the seeding marker is never deleted: the
// script names it and stops.
async function removeSeeded(client, entries, log) {
  for (const entry of entries) {
    const { card, issue, home } = entry;
    if (seededMarker(issue) !== SEEDED_BY) {
      const where = home?.name ?? card.column;
      throw new Fatal(
        issue.status_id === home?.id
          ? `"${card.title}" in ${where} was not filed by the seeding, so it is left alone; delete it and run again`
          : `"${card.title}" has left ${where} and was not filed by the seeding, so it is left alone; move it back to ${where} or delete it, and run again`,
      );
    }
    await client.data("DELETE", `/api/remote/issues/${issue.id}`);
    log(entry);
  }
}

// Deletes the seeded cards of `roles` that are no longer in the column they
// were seeded into, so that seed() files them again as new.
export async function refileMoved(client, roles) {
  return refile(client, roles, misfiled);
}

// Deletes the seeded cards of `roles` wherever they are on the board, in
// their column or not, so that seed() files them again as new; returns them
// as misfiled() does. A row created after an Electric shape exists reaches
// the page through the shape's log; one that only the shape's initial
// snapshot carries can lose state the page derives from it.
export async function refileSeeded(client, roles) {
  return refile(client, roles, (issues, byWord, chosen) =>
    chosen.flatMap((role) => {
      const card = CARDS[role];
      const issue = issues.find((i) => i.title === card.title);
      if (!issue) return [];
      const home = byWord.get(card.column) ?? null;
      const found =
        [...byWord.values()].find((s) => s.id === issue.status_id) ?? null;
      return [{ role, card, issue, home, found }];
    }),
  );
}

async function refile(client, roles, pick) {
  const board = await findBoard(client);
  if (!board) return [];
  const byWord = await columns(client, board.id);
  const entries = pick(await boardIssues(client, board.id), byWord, roles);
  await removeSeeded(client, entries, ({ role, card }) =>
    console.log(`seed: ${role}: removed "${card.title}" to file it again`),
  );
  return entries;
}
