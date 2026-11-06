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
//     A card whose title is already on the board in its column is not filed
//     again but checked: its metadata keys the seed sets (the work class and
//     the planning, triage or `merged_at` record) are read back from GET
//     /api/remote/issues, its comments from GET /api/remote/issue_comments
//     (matched by first line) and its pull request from GET
//     /api/remote/pull-requests (matched by number), and only what is
//     missing is written again, with a "was missing …; restored" line, so
//     seeding twice is safe and repairs a board whose state was partly lost.
//     A card found in another column (a film that stopped midway) is deleted
//     and filed again in its own, when it carries the seeding's marker
//     (otherwise seeding stops and names it). Only the six seeded titles are
//     ever deleted;
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

import { mkdirSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import {
  api,
  applyMasks,
  CARDS,
  checkServer,
  Fatal,
  launchChromium,
  MASK_CLASS,
  maskNeedles,
  newBoardContext,
  open,
  removeMasks,
  requireBoard,
  requireCard,
  resolveContext,
  ROOT,
  SCALE,
  seed,
  serverFromStatus,
  settle,
  Skip,
  VIEWPORT,
} from "./lib/pantry-board.mjs";

const THEMES = ["light", "dark"];

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

// --- Capturing -------------------------------------------------------------

async function capture(opts, ctx) {
  const browser = await launchChromium(opts.chromium);
  mkdirSync(opts.out, { recursive: true });
  const needles = maskNeedles(opts.mask);
  const results = [];
  try {
    for (const theme of THEMES) {
      const context = await newBoardContext(browser, theme, {
        // Dialogs fade in; a capture must not catch one halfway.
        reducedMotion: "reduce",
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
