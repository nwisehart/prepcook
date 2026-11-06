#!/usr/bin/env node
// Records the Pantry board as a short film, 20 to 30 seconds in the light
// theme (or the dark one, with --theme dark) at 1440×900 and device scale 2,
// with Playwright's recordVideo, from a
// running Prepcook. It is a sibling of scripts/screenshots.mjs and is run the
// same way: on a Mac with Prepcook running (`prepcook up`), the orchestrator
// switched off (Settings → Orchestrator → Enable automatic agent launches),
// a clone of prepcook-demo beside this repository and PREPCOOK_API_TOKEN
// exported.
//
//   PREPCOOK_API_TOKEN=… node scripts/board-video.mjs [--seed] [--url http://127.0.0.1:4100]
//     [--theme light|dark] [--repo ../prepcook-demo] [--out docs/media] [--chromium <path>]
//     [--mask <text>]… [--list] [--doctor]
//
// The server (--url, else the one `prepcook status` reports), the token and
// `--seed` work as in screenshots.mjs, whose seeding this script shares
// (scripts/lib/pantry-board.mjs); that script's header says what the seed files
// and what it cannot. The page reads the board through Electric shapes, and
// a row that reaches it only in a shape's initial snapshot (one created
// before the shape was, such as a card seeded before the board restarted)
// renders without the state the page derives from it: no Triaged header, no
// Merged chip. So with --seed the board page is opened first, which creates
// its shapes when they do not exist yet, and while it is open the three
// cards whose state the film shows (the Plan bug, the triaged investigation,
// and the Shipped card with its merged pull request) are deleted and filed
// again, wherever they are; the cards, their metadata and the pull request
// then arrive through the shapes' live log, which renders them in full. (A
// first run with no Pantry board seeds once before the page opens, so there
// is a board to open.) The film starts only when the page itself shows the
// three states, within 30 s: an Approve button on the Plan bug's card,
// "Triaged — report ready" on the investigation's and a Clean up button on
// the Shipped card's; otherwise it stops, naming the ones missing, with the
// failure screenshot and button list described under Outputs. The film
// changes two of them (the Plan bug is approved into Execute, the merged card
// is cleaned up), so after it those two are filed again when they have left
// their columns, and the board is left as seeded. The seed itself files again
// any seeded card it finds outside its column, so a film that stopped midway
// is put right by the next --seed, and it writes again whatever a seeded
// card in its column has lost (metadata, comments, the pull request). Without
// --seed the board must already hold the seeded cards in their columns.
// Either way, before the film the script reads back through the API the
// states the film shows, and refuses, naming the card, when one is missing:
// the Plan bug's plan waiting at the gate (its metadata), the investigation
// triaged (GET /api/cards/{id} status `triaged`) and the Shipped card's pull
// request merged (GET /api/remote/pull-requests). The script refuses to run
// while the orchestrator is on: an approved card in Execute would start an
// agent.
//
// The storyboard (scripts/lib/board-video.mjs, STORYBOARD and PACE; --list
// prints it), with every cursor move a glide and every hold long enough to
// read:
//   1. the board, full, 3 s;
//   2. open the Plan bug ("Sharing a list fails for a user who exists"),
//      scroll slowly to its REPRODUCTION: comment and hold, then to the RCA
//      comment and hold;
//   3. press Approve on the card. Approving is a card state change that the
//      approvals endpoint makes without the orchestrator, so the card moves
//      to Execute; the script reads the card's column back from the API and
//      stops if it did not move. Close the panel and hold on the board;
//   4. open the Review card for 2 s, and close it;
//   5. rest on the Shipped card, its Merged chip and amber "Merged — close
//      card" header in view; open it, press Clean up in its panel, and read
//      back that the card has left Shipped for the Clean up rail. With the
//      orchestrator off the repository's clean-up script does not run (the
//      seeded card has no worktree to archive); the film shows the card
//      leaving, which is what the button does on the board. Close the panel
//      and hold on the board 3 s;
//   6. end on the board, the cursor resting by the "need you" pill.
//
// Nothing on the board is staged: every state the film shows is one the
// seed or a click put there through the app and its API. Two things are
// drawn over the page, neither of them app state: a cursor (a recording
// shows no pointer, so a small arrow follows the real mouse events), and the
// masks screenshots.mjs uses, kept over the home directory, the owner's
// email and any --mask text on every frame.
//
// Browser: Playwright's Chromium and the ffmpeg build it records video with
// (`npx playwright install chromium ffmpeg`), or any Chromium build named
// with --chromium. That ffmpeg only records; the MP4 and the GIF need a
// system one (below).
//
// --doctor records nothing and changes nothing (--seed is ignored): for the
// three cards the film depends on (the Plan bug, the triaged investigation,
// the Shipped card) it prints what the server's API holds (the card's column,
// GET /api/cards/{id}, its metadata and GET /api/remote/pull-requests), what
// the browser receives (the board opened as the film opens it, and from inside
// the page, with the page's own token, a full snapshot (offset=-1) of the
// Electric shapes ProjectProvider syncs for issues, pull_requests and
// pull_request_issues) and the text of each card on the board, so a header or
// chip the film lacks can be traced to the shape that lacks its data. It also
// lists every shape request the page itself made in its first seconds (shape,
// offset, handle, live, status, size, the rows and control messages each 200
// carried, whether each response had the electric-schema, electric-handle,
// electric-offset and electric-cursor headers and its content type, and for
// pull_requests and pull_request_issues the first message's headers) and the
// text of any sync error the page shows. Anything that
// looks like a token is masked in the report.
//
// Outputs, next to each other in --out (default docs/media/), named board-dark.*
// instead of board.* with --theme dark: board.webm,
// Playwright's recording cut to the storyboard; and, when a system ffmpeg is
// on the PATH, board.mp4 (H.264, yuv420p, faststart, 1440 wide) and board.gif
// (960 wide at 12 fps from a two-pass palette, dropping to 10 fps and then
// 800 wide to stay under 6 MB; its size is printed). Without ffmpeg only
// board.webm is written, uncut, with the page load (and, with --seed, the
// refiling) at its start, and the script prints how to install ffmpeg. When
// a step fails once the page is open, the script writes failure.png there
// (the page at that moment) and prints the buttons the page offered.

import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseArgs } from "node:util";
import {
  FILM_SECONDS,
  findFfmpeg,
  formatBytes,
  outputsFor,
  PACE,
  plannedMs,
  STORYBOARD,
  writeOutputs,
} from "./lib/board-video.mjs";
import {
  api,
  applyMasks,
  boardIssues,
  cardPullRequests,
  CARDS,
  checkServer,
  columns,
  Fatal,
  findBoard,
  launchChromium,
  MASK_CLASS,
  maskNeedles,
  newBoardContext,
  open,
  planGatePending,
  pullRequestState,
  refileMoved,
  refileSeeded,
  removeMasks,
  resolveContext,
  ROOT,
  SCALE,
  seed,
  serverFromStatus,
  settle,
  VIEWPORT,
} from "./lib/pantry-board.mjs";

// The cards the film opens, and the ones it moves (which --seed refiles).
const FILMED = ["planning", "reviewing", "done"];
const MOVED = ["planning", "done"];
// The cards whose state the film shows (the plan at the gate, the triage, the
// merged pull request), which --seed files fresh before the film.
const FRESH = ["planning", "triaged", "done"];
// How long a state change may take to show before the film stops.
const STATE_TIMEOUT = 15_000;
// How long the board page may take to show the filmed states once the board
// is ready, before the film starts.
const PAGE_STATE_TIMEOUT = 30_000;
// The cards --doctor reports on, and the shapes it reads (shared/remote-types.ts
// PROJECT_*_SHAPE, which ProjectProvider subscribes to with { project_id }).
const DOCTORED = ["planning", "triaged", "done"];
const DOCTOR_SHAPES = ["issues", "pull_requests", "pull_request_issues"];

function usage() {
  return `usage: node scripts/board-video.mjs [--seed] [--url <server>] [--theme light|dark] [--repo <path>]
       [--out <dir>] [--chromium <path>] [--mask <text>]… [--list] [--doctor]`;
}

function parseOptions() {
  let parsed;
  try {
    // `pnpm board-video -- --seed` hands the script the `--` too.
    const args = process.argv.slice(2);
    if (args[0] === "--") args.shift();
    parsed = parseArgs({
      args,
      options: {
        url: { type: "string" },
        seed: { type: "boolean", default: false },
        theme: { type: "string", default: "light" },
        repo: { type: "string", default: path.join("..", "prepcook-demo") },
        out: { type: "string", default: path.join("docs", "media") },
        chromium: { type: "string" },
        mask: { type: "string", multiple: true, default: [] },
        list: { type: "boolean", default: false },
        doctor: { type: "boolean", default: false },
        help: { type: "boolean", short: "h", default: false },
      },
    }).values;
  } catch (error) {
    throw new Fatal(`${error.message}\n${usage()}`, 2);
  }
  if (parsed.theme !== "light" && parsed.theme !== "dark")
    throw new Fatal(
      `--theme is light or dark, not ${parsed.theme}\n${usage()}`,
      2,
    );
  return {
    ...parsed,
    outputs: outputsFor(parsed.theme),
    // Relative paths are taken from the repository root, as screenshots.mjs
    // takes them.
    repo: path.resolve(ROOT, parsed.repo),
    out: path.resolve(ROOT, parsed.out),
  };
}

function printList({ out, theme, outputs }) {
  const rel = path.relative(process.cwd(), out) || ".";
  console.log(
    `A ${(plannedMs() / 1000).toFixed(1)} s film of the Pantry board, ${theme} theme, ${VIEWPORT.width}×${VIEWPORT.height} at ${SCALE}x, into ${rel}/:\n`,
  );
  STORYBOARD.forEach((step, i) => {
    const ms = step.beats.reduce((total, beat) => total + PACE[beat], 0);
    console.log(`${i + 1}. ${step.id} (${(ms / 1000).toFixed(1)} s)`);
    console.log(`   ${step.shows}`);
  });
  console.log(
    `\nfiles: ${outputs.webm}; with ffmpeg on the PATH also ${outputs.mp4} and ${outputs.gif}`,
  );
}

// --- The board before the film ------------------------------------------------

async function requireOrchestratorOff(client) {
  const info = await client.data("GET", "/api/info");
  const mode = info.config?.orchestrator?.mode;
  if (mode && mode !== "off") {
    throw new Fatal(
      `the orchestrator is on (orchestrator.mode = "${mode}"), and the card the film approves would start an agent in Execute. Switch off Settings → Orchestrator → Enable automatic agent launches, save, and run again.`,
    );
  }
}

// Each filmed card, in the column it was seeded into.
async function requireSeededBoard(client, ctx) {
  if (!ctx.board)
    throw new Fatal('there is no "Pantry" board; run with --seed');
  const byWord = await columns(client, ctx.board.id);
  for (const word of ["executing", "cleanup"]) {
    if (!byWord.has(word)) {
      throw new Fatal(
        `the Pantry board has no ${word === "cleanup" ? "Clean up" : "Execute"} column for the film to move a card into`,
      );
    }
  }
  for (const role of FILMED) {
    const card = ctx.cards[role];
    const home = byWord.get(CARDS[role].column);
    if (!card) {
      throw new Fatal(
        `"${CARDS[role].title}" is not on the board; run with --seed`,
      );
    }
    if (home && card.status_id !== home.id) {
      throw new Fatal(
        `"${CARDS[role].title}" is not in ${home.name}; run with --seed to file it again`,
      );
    }
  }
  return byWord;
}

// The card states the film shows, read back through the API before anything
// is recorded: the Plan bug's plan waiting at the gate (so Approve is
// offered), the investigation triaged (its "Triaged — report ready" header),
// and the Shipped card's pull request merged (its Merged chip and the
// "Merged — close card" header with Clean up). A board without one of them
// is refused rather than filmed.
async function requireFilmedStates(client, ctx) {
  const refuse = (role, what) => {
    throw new Fatal(
      `"${CARDS[role].title}" ${what}, so the film would show the board wrong; run with --seed to restore it`,
    );
  };
  if (!planGatePending(ctx.cards.planning.extension_metadata)) {
    refuse(
      "planning",
      "has no plan waiting at the gate (no planning verdict in its metadata, or one already decided): Approve is not offered",
    );
  }
  const investigation = ctx.cards.triaged;
  if (!investigation) refuse("triaged", "is not on the board");
  const view = await client.data("GET", `/api/cards/${investigation.id}`, {
    auth: true,
  });
  if (view?.status !== "triaged") {
    refuse(
      "triaged",
      `is not triaged (GET /api/cards answers status "${view?.status}"): it has no triage record or is not an investigation`,
    );
  }
  const state = pullRequestState(
    await cardPullRequests(client, ctx.cards.done.id),
  );
  if (state !== "merged") {
    refuse(
      "done",
      state === null
        ? "has no pull request on the board service: no Merged chip and no Clean up"
        : `has its pull request ${state}, not merged: no "Merged — close card" header and no Clean up`,
    );
  }
}

// Waits until the card's column, read from the API, is `word`'s.
async function waitForColumn(client, ctx, byWord, role, word) {
  const want = byWord.get(word);
  const deadline = Date.now() + STATE_TIMEOUT;
  let issue;
  while (Date.now() < deadline) {
    issue = (await boardIssues(client, ctx.board.id)).find(
      (i) => i.id === ctx.cards[role].id,
    );
    if (want && issue?.status_id === want.id) return;
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  const now = [...byWord.values()].find((s) => s.id === issue?.status_id);
  throw new Fatal(
    `"${CARDS[role].title}" did not reach ${want?.name ?? word} (it is in ${now?.name ?? "no column"}); the film stops rather than show a state the board did not reach`,
  );
}

// --- --doctor -------------------------------------------------------------

const UUID = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i;

// `text` with JWTs, long opaque strings (ids excepted) and the mask needles
// replaced, so the report can be pasted anywhere.
function redact(text, needles) {
  let out = String(text).replace(
    /eyJ[\w-]+\.[\w-]+\.[\w-]*|[\w-]{32,}/g,
    (m) => (UUID.test(m) ? m : "[masked]"),
  );
  for (const needle of needles) out = out.split(needle).join("[masked]");
  return out;
}

const brief = (value) => {
  const json = JSON.stringify(value);
  return json === undefined ? "absent" : json.slice(0, 200);
};

// The card's metadata keys and the values its headers read.
function describeMetadata(meta) {
  let parsed = meta;
  try {
    if (typeof meta === "string") parsed = JSON.parse(meta);
  } catch {
    return [`type string, not JSON: ${brief(meta)}`];
  }
  const lines = [
    `type ${typeof meta}${typeof meta === "string" ? " (JSON text)" : ""}, keys ${brief(Object.keys(parsed ?? {}))}`,
  ];
  for (const key of ["work_class", "planning", "triage", "merged_at"]) {
    lines.push(`${key}: ${brief(parsed?.[key])}`);
  }
  return lines;
}

const describePr = (pr) =>
  `#${pr.number} ${pr.status} merged_at ${pr.merged_at ?? "null"} (id ${pr.id}, issue_id ${pr.issue_id ?? "none"})`;

// Runs in the page: a full snapshot of each shape, fetched as the app's
// collections fetch it (remote base from /api/info, bearer from
// /api/auth/token), as { table: { status, rows } | { error } }.
async function fetchShapes({ projectId, tables }) {
  const unwrap = async (response) => {
    const json = await response.json();
    return json?.data ?? json;
  };
  const info = await unwrap(await fetch("/api/info"));
  const { access_token: token } = await unwrap(await fetch("/api/auth/token"));
  const base = info?.shared_api_base || "";
  const out = { base };
  for (const table of tables) {
    const id = encodeURIComponent(projectId);
    const url = `${base}/v1/shape/project/${id}/${table}?project_id=${id}&offset=-1`;
    try {
      const response = await fetch(url, {
        headers: { Authorization: `Bearer ${token}` },
      });
      const body = await response.text();
      let messages = [];
      try {
        messages = JSON.parse(body);
      } catch {
        out[table] = { status: response.status, error: body.slice(0, 200) };
        continue;
      }
      const rows = new Map();
      for (const message of Array.isArray(messages) ? messages : []) {
        const op = message.headers?.operation;
        if (op === "delete") rows.delete(message.key);
        else if (message.value) {
          rows.set(message.key, { ...rows.get(message.key), ...message.value });
        }
      }
      out[table] = { status: response.status, rows: [...rows.values()] };
    } catch (error) {
      out[table] = { error: String(error) };
    }
  }
  return out;
}

// Records every Electric shape request the page makes, in order, with what
// came back. `entries()` is the log so far; `drain()` waits (briefly) for the
// response bodies still being read.
function recordShapeRequests(page) {
  const log = [];
  const byRequest = new Map();
  const pending = new Set();
  page.on("request", (request) => {
    if (!request.url().includes("/v1/shape/")) return;
    const entry = { url: request.url(), state: "pending" };
    byRequest.set(request, entry);
    log.push(entry);
  });
  page.on("response", (response) => {
    const entry = byRequest.get(response.request());
    if (!entry) return;
    entry.state = "done";
    entry.status = response.status();
    const read = response
      .allHeaders()
      .then((headers) => {
        entry.headers = headers;
        return response.body();
      })
      .then((body) => {
        entry.size = body.length;
        if (entry.status === 200) Object.assign(entry, countMessages(body));
      })
      .catch((error) => {
        entry.bodyError = String(error.message ?? error).split("\n")[0];
      })
      .finally(() => pending.delete(read));
    pending.add(read);
  });
  page.on("requestfailed", (request) => {
    const entry = byRequest.get(request);
    if (!entry) return;
    entry.state = "failed";
    entry.failure = request.failure()?.errorText ?? "unknown";
  });
  return {
    entries: () => [...log],
    drain: (ms = 3_000) =>
      Promise.race([
        Promise.allSettled([...pending]),
        new Promise((resolve) => setTimeout(resolve, ms)),
      ]),
  };
}

// The rows (entries whose `value` is an object) and the control messages
// (`headers.control`, by kind) in a shape response body, and the first
// message's `headers` object.
function countMessages(body) {
  let messages;
  try {
    messages = JSON.parse(body.toString("utf8"));
  } catch {
    return { parseError: true };
  }
  const list = Array.isArray(messages) ? messages : [];
  let rows = 0;
  const control = {};
  for (const message of list) {
    const kind = message?.headers?.control;
    if (kind) control[kind] = (control[kind] ?? 0) + 1;
    else if (message?.value && typeof message.value === "object") rows += 1;
  }
  return { rows, control, firstHeaders: list[0]?.headers };
}

// The shapes whose responses --doctor prints the first message's headers for.
const FIRST_HEADERS_SHAPES = new Set(["pull_requests", "pull_request_issues"]);

// The shape a recorded request is for: the path after the project id.
function shapeOf(entry) {
  const parts = new URL(entry.url).pathname.split("/").filter(Boolean);
  const after = parts.slice(parts.indexOf("shape") + 1);
  return (after[0] === "project" ? after.slice(2) : after).join("/");
}

// Which Electric response headers a shape response carried (whether each is
// there, not its value, except the offset, which carries no secret) and its
// content type. The client types a row's values (JSON text, timestamps) by
// electric-schema, so a response without it shows here.
function describeResponseHeaders(headers) {
  const has = (name) => (headers[name] === undefined ? "no" : "yes");
  return `hdr: schema=${has("electric-schema")} handle=${has("electric-handle")} offset=${headers["electric-offset"] ?? "no"} cursor=${has("electric-cursor")} content-type=${headers["content-type"] ?? "none"}`;
}

// The lines for a recorded shape request: the shape, offset, handle and live,
// and what came back; then its response headers and, for the pull request
// shapes, the first message's headers.
function describeShapeRequest(entry, index) {
  const url = new URL(entry.url);
  const shape = shapeOf(entry);
  const param = (name) => url.searchParams.get(name) ?? "-";
  let result;
  if (entry.state === "failed") result = `failed: ${entry.failure}`;
  else if (entry.state === "pending") result = "no response yet";
  else {
    result = `${entry.status}, ${entry.size ?? "?"} B`;
    if (entry.bodyError) result += `, body unread (${entry.bodyError})`;
    else if (entry.parseError) result += ", body not JSON";
    else if (entry.rows !== undefined) {
      const control = Object.entries(entry.control)
        .map(([kind, n]) => `control: ${kind}${n > 1 ? ` x${n}` : ""}`)
        .join(", ");
      result += `, ${entry.rows} rows${control ? `, ${control}` : ""}`;
    }
  }
  const lines = [
    `   ${index + 1}. ${shape} offset=${param("offset")} handle=${param("handle")} live=${param("live")}: ${result}`,
  ];
  if (entry.headers) {
    lines.push(`      ${describeResponseHeaders(entry.headers)}`);
  }
  if (FIRST_HEADERS_SHAPES.has(shape) && entry.rows !== undefined) {
    lines.push(
      `      first message headers: ${JSON.stringify(entry.firstHeaders) ?? "none (no messages)"}`,
    );
  }
  return lines.join("\n");
}

async function doctor(opts, client, ctx) {
  const needles = maskNeedles(opts.mask);
  const say = (line) => console.log(redact(line, needles));
  if (!ctx.board) {
    say('doctor: there is no "Pantry" board');
    return 0;
  }
  const byWord = await columns(client, ctx.board.id);
  const columnName = (id) =>
    [...byWord.values()].find((s) => s.id === id)?.name ?? "no column";

  say("== 1. the server's API");
  for (const role of DOCTORED) {
    const issue = ctx.cards[role];
    say(`-- ${role}: "${CARDS[role].title}"`);
    if (!issue) {
      say("   not on the board");
      continue;
    }
    say(`   id ${issue.id}, column ${columnName(issue.status_id)}`);
    const view = await client.call("GET", `/api/cards/${issue.id}`, {
      auth: true,
    });
    const card = view.json?.data ?? view.json;
    say(
      `   GET /api/cards: ${view.status}, status ${card?.status}, work_class ${card?.work_class}`,
    );
    for (const line of describeMetadata(issue.extension_metadata)) {
      say(`   metadata ${line}`);
    }
    const prs = await cardPullRequests(client, issue.id);
    say(`   pull requests: ${prs.map(describePr).join("; ") || "none"}`);
  }

  const browser = await launchChromium(opts.chromium);
  try {
    const context = await newBoardContext(browser, opts.theme);
    const page = await context.newPage();
    page.prepcookWarnings = [];
    const recorder = recordShapeRequests(page);
    await open(page, ctx, `/projects/${ctx.board.id}`);
    await settle(page, () => cardOnBoard(page, ctx.cards.done ?? { id: "" }));
    await page.waitForTimeout(5_000);
    // Taken before fetchShapes, whose own requests are not the page's.
    const subscribed = recorder.entries();
    await recorder.drain();

    say("== 2. the page's own shape subscriptions, in order");
    if (!subscribed.length) say("   none");
    // Printed unredacted: offset, handle and live carry no secrets, and the
    // masking would hide a long handle.
    subscribed.forEach((entry, i) =>
      console.log(describeShapeRequest(entry, i)),
    );
    const syncError = page.getByRole("button", { name: /sync error/ });
    if (await syncError.count()) {
      await syncError.first().click();
      const popover = page
        .locator("[data-radix-popper-content-wrapper]")
        .last();
      await popover
        .waitFor({ state: "visible", timeout: 3_000 })
        .catch(() => {});
      const text = (await popover.count())
        ? (await popover.innerText()).replace(/\s+/g, " ").trim()
        : "(no popover)";
      say(`   sync error: ${text}`);
      await page.keyboard.press("Escape");
    } else {
      say("   sync error: none shown");
    }

    const shapes = await page.evaluate(fetchShapes, {
      projectId: ctx.board.id,
      tables: DOCTOR_SHAPES,
    });

    say(
      `== 3. the Electric shapes, from the page (remote base ${shapes.base || "the page's origin"})`,
    );
    for (const table of DOCTOR_SHAPES) {
      const shape = shapes[table];
      say(
        `   ${table}: ${shape.error ? `failed (${shape.status ?? ""} ${shape.error})` : `${shape.status}, ${shape.rows.length} rows`}`,
      );
    }
    const rows = (table) => shapes[table]?.rows ?? [];
    for (const role of DOCTORED) {
      const issue = ctx.cards[role];
      if (!issue) continue;
      say(`-- ${role}`);
      const row = rows("issues").find((r) => r.id === issue.id);
      if (!row) say("   no row in the issues shape");
      else {
        for (const line of describeMetadata(row.extension_metadata)) {
          say(`   metadata ${line}`);
        }
      }
      const links = rows("pull_request_issues").filter(
        (l) => l.issue_id === issue.id,
      );
      say(
        `   pull_request_issues: ${links.map((l) => `pr ${l.pull_request_id}`).join("; ") || "none"}`,
      );
      const linked = new Set(links.map((l) => l.pull_request_id));
      const prs = rows("pull_requests").filter(
        (pr) => pr.issue_id === issue.id || linked.has(pr.id),
      );
      say(`   pull_requests: ${prs.map(describePr).join("; ") || "none"}`);
    }

    say("== 4. the cards on the page");
    for (const role of DOCTORED) {
      const issue = ctx.cards[role];
      if (!issue) continue;
      const locator = cardOnBoard(page, issue);
      const text = (await locator.count())
        ? (await locator.first().innerText()).replace(/\s+/g, " ").trim()
        : "(not on the page)";
      say(`-- ${role}: ${text}`);
    }
    for (const warning of page.prepcookWarnings) say(`warning: ${warning}`);
    await context.close();
  } finally {
    await browser.close();
  }
  return 0;
}

// --- Drawn over the page --------------------------------------------------

// Runs in the page from its first script: a small arrow that follows the
// mouse events Playwright sends (a recording has no pointer of its own) and
// dips while a button is held.
function cursorOverlay() {
  const install = () => {
    const cursor = document.createElement("div");
    cursor.setAttribute("aria-hidden", "true");
    cursor.innerHTML =
      '<svg width="22" height="22" viewBox="0 0 22 22"><path d="M3 2 L3 18 L7.5 14 L10.5 20.5 L13 19.4 L10.1 13 L16 13 Z" fill="#1f2328" stroke="#ffffff" stroke-width="1.4" stroke-linejoin="round"/></svg>';
    Object.assign(cursor.style, {
      position: "fixed",
      left: "0",
      top: "0",
      zIndex: "2147483647",
      pointerEvents: "none",
      transformOrigin: "3px 2px",
      transform: "translate(-40px, -40px)",
    });
    let x = -40;
    let y = -40;
    let scale = 1;
    // Re-attached whenever the page replaces what it was attached to.
    const place = () => {
      if (!cursor.isConnected) document.documentElement.appendChild(cursor);
      cursor.style.transform = `translate(${x - 3}px, ${y - 2}px) scale(${scale})`;
    };
    place();
    window.addEventListener(
      "mousemove",
      (event) => {
        x = event.clientX;
        y = event.clientY;
        place();
      },
      true,
    );
    window.addEventListener(
      "mousedown",
      () => {
        scale = 0.85;
        place();
      },
      true,
    );
    window.addEventListener(
      "mouseup",
      () => {
        scale = 1;
        place();
      },
      true,
    );
  };
  if (document.documentElement) install();
  else document.addEventListener("DOMContentLoaded", install);
}

// The init script that keeps the masks over the needles on every frame,
// built from the same applyMasks and removeMasks screenshots.mjs runs once
// per capture.
function maskingScript(needles) {
  const args = JSON.stringify({ needles, className: MASK_CLASS });
  return `(() => {
    const applyMasks = ${applyMasks.toString()};
    const removeMasks = ${removeMasks.toString()};
    const args = ${args};
    const frame = () => {
      if (document.body) {
        removeMasks(args.className);
        applyMasks(args);
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  })();`;
}

// --- Moving like a person -----------------------------------------------------

const wait = (page, ms) => page.waitForTimeout(Math.max(0, ms));

function ease(t) {
  return t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
}

// A beat that lasts at least `ms`: `until` (when given) is awaited, and the
// rest of the beat is waited out, so a quick answer does not shorten it.
async function beat(page, ms, until) {
  const started = Date.now();
  if (until) await until();
  await wait(page, ms - (Date.now() - started));
}

function makeCursor(page) {
  const at = { x: VIEWPORT.width * 0.55, y: VIEWPORT.height * 0.7 };
  return {
    // Puts the cursor down without a glide, before the film starts.
    async park() {
      await page.mouse.move(at.x, at.y);
    },
    // Glides to (x, y) in PACE.glide, one page.mouse.move per frame on an
    // ease-in-out curve.
    async glideTo(x, y, ms = PACE.glide) {
      const from = { ...at };
      const steps = Math.max(2, Math.round(ms / 16));
      const started = Date.now();
      for (let i = 1; i <= steps; i += 1) {
        const t = ease(i / steps);
        await page.mouse.move(
          from.x + (x - from.x) * t,
          from.y + (y - from.y) * t,
        );
        await wait(page, started + (ms * i) / steps - Date.now());
      }
      at.x = x;
      at.y = y;
    },
    async click() {
      const started = Date.now();
      await page.mouse.down();
      await wait(page, 90);
      await page.mouse.up();
      await wait(page, PACE.click - (Date.now() - started));
    },
    position: () => ({ ...at }),
  };
}

// The point a glide aims at on `locator`: its centre, kept inside the
// viewport when the element runs off the edge (the Shipped column does at
// 1440 wide).
async function aim(locator, { dx = 0.5, dy = 0.5 } = {}) {
  await locator.waitFor({ state: "visible", timeout: STATE_TIMEOUT });
  const box = await locator.boundingBox();
  if (!box) throw new Fatal("an element the film needs has no box on screen");
  const right = Math.min(box.x + box.width, VIEWPORT.width - 24);
  const bottom = Math.min(box.y + box.height, VIEWPORT.height - 24);
  return {
    x: box.x + (right - box.x) * dx,
    y: box.y + (bottom - box.y) * dy,
  };
}

async function glideAndClick(cursor, locator, options) {
  const { x, y } = await aim(locator, options);
  await cursor.glideTo(x, y);
  await cursor.click();
}

// Scrolls whatever is under the cursor, over PACE.scroll, until the top of
// `locator` sits a quarter of the way down the viewport. Content that is
// not laid out yet is reached in small wheel steps first.
async function slowScrollTo(page, locator, ms = PACE.scroll) {
  const started = Date.now();
  for (let i = 0; i < 60 && !(await locator.boundingBox()); i += 1) {
    await page.mouse.wheel(0, 120);
    await wait(page, 30);
  }
  const box = await locator.boundingBox();
  if (!box) {
    throw new Fatal("a comment the film scrolls to never appeared in the card");
  }
  const distance = box.y - VIEWPORT.height * 0.25;
  const steps = Math.max(2, Math.round((ms - (Date.now() - started)) / 16));
  let done = 0;
  for (let i = 1; i <= steps; i += 1) {
    const next = Math.round(distance * ease(i / steps));
    await page.mouse.wheel(0, next - done);
    done = next;
    await wait(page, 16);
  }
  await wait(page, ms - (Date.now() - started));
}

// --- The film -------------------------------------------------------------

function cardOnBoard(page, issue) {
  return page.locator(`[data-rfd-draggable-id="${issue.id}"]`);
}

function closePanelButton(page) {
  return page.getByRole("button", { name: "Close panel", exact: true });
}

async function openCard(page, cursor, issue) {
  await glideAndClick(cursor, cardOnBoard(page, issue), { dx: 0.45, dy: 0.4 });
  await beat(page, PACE.settle, () =>
    closePanelButton(page)
      .first()
      .waitFor({ state: "visible", timeout: STATE_TIMEOUT }),
  );
}

async function closePanel(page, cursor) {
  const button = closePanelButton(page).first();
  if (!(await button.isVisible())) {
    // The panel closed itself; the beat is still spent.
    await wait(page, PACE.glide + PACE.click + PACE.settle);
    return;
  }
  await glideAndClick(cursor, button);
  await beat(page, PACE.settle, () =>
    button.waitFor({ state: "hidden", timeout: STATE_TIMEOUT }),
  );
}

// Waits until the open board page shows each state the film needs, on the
// card itself, and stops naming every one that has not appeared in
// PAGE_STATE_TIMEOUT.
async function waitForFilmedStatesOnPage(page, cards) {
  const wanted = [
    {
      what: `the Approve button on "${CARDS.planning.title}"`,
      locator: cardOnBoard(page, cards.planning).getByRole("button", {
        name: "Approve",
        exact: true,
      }),
    },
    {
      what: `"Triaged — report ready" on "${CARDS.triaged.title}"`,
      locator: cardOnBoard(page, cards.triaged).getByText(
        "Triaged — report ready",
      ),
    },
    {
      what: `the Clean up button on "${CARDS.done.title}"`,
      locator: cardOnBoard(page, cards.done).getByRole("button", {
        name: "Clean up",
        exact: true,
      }),
    },
  ];
  // "attached": the Shipped column can run past the right edge at 1440 wide.
  const results = await Promise.allSettled(
    wanted.map(({ locator }) =>
      locator
        .first()
        .waitFor({ state: "attached", timeout: PAGE_STATE_TIMEOUT }),
    ),
  );
  const missing = wanted
    .filter((_, i) => results[i].status === "rejected")
    .map(({ what }) => what);
  if (missing.length) {
    throw new Fatal(
      `the board page never showed ${missing.join(", ")} within ${PAGE_STATE_TIMEOUT / 1000} s, though the API holds those states; run with --doctor to see what the page's shapes carried`,
    );
  }
}

// Opens the board, has `prepare` make it ready while the page is open (it
// returns { ctx, byWord }), waits for the page to show the filmed states, and
// films.
async function film(page, opening, prepare, marks) {
  await open(page, opening, `/projects/${opening.board.id}`);
  // The board's shapes are subscribed once its cards are drawn.
  await settle(page, () => page.locator("[data-rfd-draggable-id]").first());
  const { ctx, byWord } = await prepare();
  await waitForFilmedStatesOnPage(page, ctx.cards);
  await settle(page, () => cardOnBoard(page, ctx.cards.done));
  await shoot(page, ctx, byWord, marks);
}

async function shoot(page, ctx, byWord, marks) {
  const { client, cards } = ctx;
  const cursor = makeCursor(page);

  await page.evaluate(() => document.fonts.ready);
  await cursor.park();
  await wait(page, 400);
  marks.start = Date.now();

  // 1. The board, full.
  await wait(page, PACE.board);

  // 2. The Plan bug: its reproduction, then its RCA.
  await openCard(page, cursor, cards.planning);
  // The wheel scrolls what is under the cursor: move into the panel first.
  const closeAt = await aim(closePanelButton(page).first());
  await cursor.glideTo(closeAt.x - 220, VIEWPORT.height * 0.6);
  await slowScrollTo(page, page.getByText("REPRODUCTION:").first());
  await wait(page, PACE.read);
  await slowScrollTo(
    page,
    page.getByText("RCA: sharing looks the email up").first(),
  );
  await wait(page, PACE.read);

  // 3. Approve, on the card itself; the panel's own Approve when the card's
  //    button is out of reach.
  const onCard = cardOnBoard(page, cards.planning).getByRole("button", {
    name: "Approve",
    exact: true,
  });
  const approve = (await onCard.isVisible())
    ? onCard
    : page
        .getByRole("region", { name: /^Needs you/ })
        .getByRole("button", { name: "Approve", exact: true });
  await glideAndClick(cursor, approve.first());
  await beat(page, PACE.settle, () =>
    waitForColumn(client, ctx, byWord, "planning", "executing"),
  );
  await closePanel(page, cursor);
  await wait(page, PACE.rest);

  // 4. A glance at the Review card.
  await openCard(page, cursor, cards.reviewing);
  await wait(page, PACE.glance);
  await closePanel(page, cursor);

  // 5. The merged card: rest on it so its Merged chip and amber "Merged —
  //    close card" header are in view, then press the header's Clean up,
  //    which is on the board card itself, the same place Approve lives. The
  //    panel is not opened: it slides over the Shipped column and would hide
  //    the button.
  const shipped = cardOnBoard(page, cards.done);
  const restAt = await aim(shipped, { dx: 0.45, dy: 0.4 });
  await cursor.glideTo(restAt.x, restAt.y);
  await wait(page, PACE.rest);
  const cleanUp = shipped.getByRole("button", {
    name: "Clean up",
    exact: true,
  });
  await cleanUp.first().scrollIntoViewIfNeeded();
  await glideAndClick(cursor, cleanUp.first());
  await beat(page, PACE.settle, () =>
    waitForColumn(client, ctx, byWord, "done", "cleanup"),
  );
  await wait(page, PACE.board);

  // 6. The "need you" pill.
  const pill = page.getByRole("button", { name: /^\d+ needs? you$/ }).first();
  const { x, y } = await aim(pill);
  await cursor.glideTo(x + 40, y + 26);
  await wait(page, PACE.rest);
  marks.end = Date.now();
}

async function record(opts, opening, prepare, scratch) {
  const browser = await launchChromium(opts.chromium);
  const marks = {};
  let raw;
  try {
    const context = await newBoardContext(browser, opts.theme, {
      recordVideo: { dir: scratch, size: VIEWPORT },
    });
    await context.addInitScript(cursorOverlay);
    await context.addInitScript({
      content: maskingScript(maskNeedles(opts.mask)),
    });
    let page;
    try {
      page = await context.newPage();
    } catch (error) {
      if (!/ffmpeg/i.test(error.message)) throw error;
      throw new Fatal(
        "Playwright records video with its own ffmpeg, which is not installed: run `npx playwright install chromium ffmpeg`",
        2,
      );
    }
    marks.recording = Date.now();
    // settle() records content that did not appear here.
    page.prepcookWarnings = [];
    try {
      await film(page, opening, prepare, marks);
    } catch (error) {
      // What the page looked like when a step failed, and which buttons it
      // offered, so a selector that no longer matches the UI can be fixed
      // without re-running the whole film blind.
      const shot = path.join(opts.out, "failure.png");
      mkdirSync(opts.out, { recursive: true });
      await page.screenshot({ path: shot, fullPage: false }).catch(() => {});
      const buttons = await page
        .getByRole("button")
        .evaluateAll((nodes) =>
          nodes
            .map((n) =>
              (n.getAttribute("aria-label") || n.textContent || "").trim(),
            )
            .filter(Boolean),
        )
        .catch(() => []);
      console.error(`board-video: the page at the failure is in ${shot}`);
      console.error(`board-video: buttons on it: ${JSON.stringify(buttons)}`);
      throw error;
    } finally {
      await context.close();
    }
    raw = await page.video().path();
  } finally {
    await browser.close();
  }
  return { raw, marks };
}

async function main() {
  const opts = parseOptions();
  if (opts.help) {
    console.log(usage());
    return 0;
  }
  if (opts.list) {
    printList(opts);
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
  if (opts.doctor) {
    return doctor(opts, client, {
      ...(await resolveContext(client, opts)),
      base,
    });
  }
  await requireOrchestratorOff(client);
  // The board as the film needs it, read back through the API.
  const checked = async () => {
    const ctx = { ...(await resolveContext(client, opts)), base };
    const byWord = await requireSeededBoard(client, ctx);
    await requireFilmedStates(client, ctx);
    return { ctx, byWord };
  };
  let opening;
  let prepare;
  if (opts.seed) {
    // There must be a board for the page to open before the refile.
    if (!(await findBoard(client))) await seed(client, opts);
    opening = { ...(await resolveContext(client, opts)), base };
    // Run with the board page open, so the shapes exist and the refiled
    // state-bearing cards reach the page through their live log (see the
    // header).
    prepare = async () => {
      const fresh = await refileSeeded(client, FRESH);
      await seed(client, opts);
      for (const { card, home } of fresh) {
        console.log(
          `seed: ${home?.name ?? card.column}: refiled "${card.title}" so the film sees it fresh`,
        );
      }
      return checked();
    };
  } else {
    // Without --seed the board is checked before a browser is launched.
    const ready = await checked();
    opening = ready.ctx;
    prepare = async () => ready;
  }

  const ffmpeg = findFfmpeg();
  const scratch = mkdtempSync(path.join(os.tmpdir(), "prepcook-board-video-"));
  try {
    console.log("recording the board…");
    const { raw, marks } = await record(opts, opening, prepare, scratch);
    const durationMs = marks.end - marks.start;
    const result = writeOutputs({
      raw,
      outDir: opts.out,
      // Half a second either side, so the cut never clips a move.
      startMs: marks.start - marks.recording - 500,
      durationMs: durationMs + 1000,
      ffmpeg,
      scratch,
      outputs: opts.outputs,
    });
    for (const file of result.files) {
      console.log(
        `wrote ${path.relative(process.cwd(), path.join(opts.out, file.name))} (${formatBytes(file.bytes)})`,
      );
    }
    const seconds = durationMs / 1000;
    console.log(`the film runs ${seconds.toFixed(1)} s`);
    if (seconds < FILM_SECONDS.min || seconds > FILM_SECONDS.max) {
      console.warn(
        `warning: that is outside ${FILM_SECONDS.min}–${FILM_SECONDS.max} s; the server answered slowly, or PACE needs adjusting`,
      );
    }
    if (result.gif?.over) {
      console.warn(
        `warning: ${opts.outputs.gif} is still over ${formatBytes(6_000_000)} at ${result.gif.width} wide and ${result.gif.fps} fps`,
      );
    }
    if (!result.trimmed) {
      console.log(
        `\nffmpeg is not on the PATH, so ${opts.outputs.webm} is the recording as it is (the page load included) and there is no ${opts.outputs.mp4} or ${opts.outputs.gif}. Install it and run again:\n\n  brew install ffmpeg\n`,
      );
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
    if (opts.seed) {
      console.log("putting the filmed cards back as seeded");
      await refileMoved(client, MOVED);
      await seed(client, opts);
    }
  }
  return 0;
}

main().then(
  (code) => process.exit(code),
  (error) => {
    console.error(
      `board-video: ${error instanceof Fatal ? error.message : error.stack}`,
    );
    process.exit(error instanceof Fatal ? error.code : 1);
  },
);
