// The parts of scripts/board-video.mjs that need neither a browser nor a
// server: the storyboard and its pace, the ffmpeg command lines (as argument
// arrays, never a shell string), the GIF's size fallback, and writing the
// three files from Playwright's recording.

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import path from "node:path";

// How long each kind of beat takes on screen, in milliseconds. Nothing is
// faster than a person would do it: a cursor move is a glide, a scroll is
// slow, and anything to read is held between 1.5 and 2.5 seconds.
export const PACE = {
  // A cursor glide to the next target.
  glide: 550,
  // The press, and the beat after it.
  click: 200,
  // A panel opening or closing, or a card changing column.
  settle: 350,
  // A slow scroll to a comment in the open card.
  scroll: 900,
  // A hold on something to read.
  read: 1800,
  // A short look at a card that is not read in full.
  glance: 2000,
  // A short hold on the board between steps.
  rest: 1500,
  // A hold on the full board.
  board: 3000,
};

// The holds that are for reading, which the storyboard keeps between 1.5 and
// 2.5 seconds.
export const READING_BEATS = ["read", "glance", "rest"];

// The beats that open a card, and close its panel again.
const OPEN = ["glide", "click", "settle"];
const CLOSE = ["glide", "click", "settle"];

// The film, step by step, in order. `beats` are what each step spends its
// time on (PACE keys); the driver in board-video.mjs performs each step with
// the same PACE values, so the planned length is the film's length less the
// time the server takes to answer.
export const STORYBOARD = [
  {
    id: "board",
    shows: "The Pantry board, full",
    beats: ["board"],
  },
  {
    id: "open-plan",
    shows: 'Open the Plan bug, "Sharing a list fails for a user who exists"',
    beats: [...OPEN],
  },
  {
    id: "reproduction",
    shows:
      "Move into the card and scroll slowly to its REPRODUCTION: comment, and hold",
    beats: ["glide", "scroll", "read"],
  },
  {
    id: "rca",
    shows: "Scroll on to the RCA comment, and hold",
    beats: ["scroll", "read"],
  },
  {
    id: "approve",
    shows:
      "Approve on the card; it moves to Execute (read back from the API); close the panel and hold on the board",
    beats: ["glide", "click", "settle", ...CLOSE, "rest"],
  },
  {
    id: "review",
    shows: "Open the Review card for a glance, and close it",
    beats: [...OPEN, "glance", ...CLOSE],
  },
  {
    id: "shipped",
    shows:
      'Rest on the Shipped card, its Merged chip and "Merged — close card" header; open it and press Clean up; it leaves Shipped (read back from the API); close the panel and hold on the board',
    beats: [
      "glide",
      "rest",
      "click",
      "settle",
      "glide",
      "click",
      "settle",
      ...CLOSE,
      "board",
    ],
  },
  {
    id: "end",
    shows: 'End on the board, the cursor resting by the "need you" pill',
    beats: ["glide", "rest"],
  },
];

// The film's length, in seconds, is kept within these bounds.
export const FILM_SECONDS = { min: 20, max: 30 };

export function plannedMs(storyboard = STORYBOARD, pace = PACE) {
  return storyboard
    .flatMap((step) => step.beats)
    .reduce((total, beat) => {
      if (!(beat in pace)) throw new Error(`no pace for the beat "${beat}"`);
      return total + pace[beat];
    }, 0);
}

// The files written into --out, next to each other.
// The film's files per theme: board.* for the light film, board-dark.* for
// the dark one, so the site can play whichever matches the reader's theme.
export function outputsFor(theme) {
  const suffix = theme === "dark" ? "-dark" : "";
  return {
    webm: `board${suffix}.webm`,
    mp4: `board${suffix}.mp4`,
    gif: `board${suffix}.gif`,
  };
}

export const OUTPUTS = outputsFor("light");

// The MP4 and the GIF widths, and the GIF's limit: GitHub renders a README
// image of any size, but one over a few megabytes makes the page slow.
export const MP4_WIDTH = 1440;
export const GIF_LIMIT_BYTES = 6_000_000;
// The GIF is made at the first of these that comes in under the limit; when
// none does, the last is kept and its size reported.
export const GIF_ATTEMPTS = [
  { width: 960, fps: 12 },
  { width: 960, fps: 10 },
  { width: 800, fps: 10 },
];

// What to do after GIF attempt `index` came out at `bytes`: keep it, or make
// attempt `next`. `over` says the kept one is still over the limit.
export function afterGifAttempt(
  index,
  bytes,
  { limit = GIF_LIMIT_BYTES, attempts = GIF_ATTEMPTS } = {},
) {
  if (bytes < limit) return { keep: true, over: false };
  if (index + 1 < attempts.length) return { keep: false, next: index + 1 };
  return { keep: true, over: true };
}

const QUIET = ["-hide_banner", "-loglevel", "error", "-y"];

function seconds(ms) {
  return (Math.max(0, ms) / 1000).toFixed(3);
}

// Playwright's recording, cut to the storyboard (`startMs` after the
// recording began, `durationMs` long) and re-encoded as VP9.
export function trimArgs(raw, out, startMs, durationMs) {
  return [
    ...QUIET,
    "-ss",
    seconds(startMs),
    "-i",
    raw,
    "-t",
    seconds(durationMs),
    "-an",
    "-c:v",
    "libvpx-vp9",
    "-b:v",
    "0",
    "-crf",
    "32",
    "-row-mt",
    "1",
    "-deadline",
    "good",
    "-cpu-used",
    "2",
    out,
  ];
}

// H.264 in yuv420p (what every browser and QuickTime plays), MP4_WIDTH wide,
// with the index at the front so it starts playing before it has loaded.
export function mp4Args(webm, out) {
  return [
    ...QUIET,
    "-i",
    webm,
    "-an",
    "-vf",
    `scale=${MP4_WIDTH}:-2:flags=lanczos`,
    "-c:v",
    "libx264",
    "-preset",
    "slow",
    "-crf",
    "20",
    "-pix_fmt",
    "yuv420p",
    "-movflags",
    "+faststart",
    out,
  ];
}

function gifFilter({ width, fps }) {
  return `fps=${fps},scale=${width}:-1:flags=lanczos`;
}

// The GIF in two passes: a palette made from the frames that change, then the
// frames mapped onto it.
export function gifPaletteArgs(webm, palette, attempt) {
  return [
    ...QUIET,
    "-i",
    webm,
    "-vf",
    `${gifFilter(attempt)},palettegen=stats_mode=diff`,
    palette,
  ];
}

export function gifArgs(webm, palette, out, attempt) {
  return [
    ...QUIET,
    "-i",
    webm,
    "-i",
    palette,
    "-lavfi",
    `${gifFilter(attempt)}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle`,
    "-loop",
    "0",
    out,
  ];
}

export function formatBytes(bytes) {
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} kB`;
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

// `ffmpeg` when a system ffmpeg is on the PATH, else null. Playwright's own
// ffmpeg, which records the video, is not on the PATH and cannot make the
// MP4 or the GIF.
export function findFfmpeg(spawn = spawnSync) {
  const result = spawn("ffmpeg", ["-version"], { stdio: "ignore" });
  return result.status === 0 ? "ffmpeg" : null;
}

function run(ffmpeg, args) {
  const result = spawnSync(ffmpeg, args, {
    stdio: ["ignore", "ignore", "pipe"],
    encoding: "utf8",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const why = (result.stderr || "").trim().split("\n").at(-1);
    throw new Error(`ffmpeg failed making ${args.at(-1)}: ${why}`);
  }
}

// Writes `outputs` (OUTPUTS unless a theme's names are given) into `outDir`
// from the recording at `raw`. With ffmpeg the
// WebM is cut to the storyboard and the MP4 and GIF are made from it;
// without, the recording is copied as it is, and an MP4 or GIF left from an
// earlier run is removed so the three never disagree. `scratch` holds the
// GIF's palette. Returns what was written, with sizes.
export function writeOutputs({
  raw,
  outDir,
  startMs,
  durationMs,
  ffmpeg,
  scratch,
  outputs = OUTPUTS,
  log = console.log,
}) {
  mkdirSync(outDir, { recursive: true });
  const at = (name) => path.join(outDir, name);
  const size = (file) => statSync(file).size;
  if (!ffmpeg) {
    copyFileSync(raw, at(outputs.webm));
    for (const name of [outputs.mp4, outputs.gif]) {
      if (existsSync(at(name))) {
        rmSync(at(name));
        log(
          `removed ${name}, which an earlier run made from another recording`,
        );
      }
    }
    return {
      trimmed: false,
      files: [{ name: outputs.webm, bytes: size(at(outputs.webm)) }],
    };
  }
  run(ffmpeg, trimArgs(raw, at(outputs.webm), startMs, durationMs));
  run(ffmpeg, mp4Args(at(outputs.webm), at(outputs.mp4)));
  const palette = path.join(scratch, "palette.png");
  let index = 0;
  let outcome;
  for (;;) {
    const attempt = GIF_ATTEMPTS[index];
    run(ffmpeg, gifPaletteArgs(at(outputs.webm), palette, attempt));
    run(ffmpeg, gifArgs(at(outputs.webm), palette, at(outputs.gif), attempt));
    const bytes = size(at(outputs.gif));
    log(
      `${outputs.gif}: ${attempt.width} wide at ${attempt.fps} fps is ${formatBytes(bytes)}`,
    );
    outcome = afterGifAttempt(index, bytes);
    if (outcome.keep) break;
    index = outcome.next;
  }
  return {
    trimmed: true,
    gif: { ...GIF_ATTEMPTS[index], over: outcome.over },
    files: Object.values(outputs).map((name) => ({
      name,
      bytes: size(at(name)),
    })),
  };
}
