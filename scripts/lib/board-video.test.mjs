// Unit tests for the browser-free parts of scripts/board-video.mjs:
// `node --test scripts/lib/board-video.test.mjs`.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  afterGifAttempt,
  FILM_SECONDS,
  findFfmpeg,
  formatBytes,
  GIF_ATTEMPTS,
  GIF_LIMIT_BYTES,
  gifArgs,
  gifPaletteArgs,
  mp4Args,
  PACE,
  plannedMs,
  READING_BEATS,
  STORYBOARD,
  trimArgs,
  writeOutputs,
} from "./board-video.mjs";

function scratchDir(t) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "board-video-test-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test("the storyboard runs in order and lasts 20 to 30 seconds", () => {
  assert.deepEqual(
    STORYBOARD.map((step) => step.id),
    [
      "board",
      "open-plan",
      "reproduction",
      "rca",
      "approve",
      "review",
      "shipped",
      "end",
    ],
  );
  const seconds = plannedMs() / 1000;
  assert.ok(
    seconds >= FILM_SECONDS.min && seconds <= FILM_SECONDS.max,
    `${seconds} s`,
  );
  for (const step of STORYBOARD) {
    assert.ok(step.shows, `${step.id} says what it shows`);
    assert.ok(step.beats.length > 0, `${step.id} has beats`);
    for (const beat of step.beats)
      assert.ok(beat in PACE, `${step.id}: ${beat}`);
  }
});

test("the board opens and closes on a 3 second hold", () => {
  assert.deepEqual(STORYBOARD[0].beats, ["board"]);
  assert.equal(PACE.board, 3000);
  const shipped = STORYBOARD.find((step) => step.id === "shipped");
  assert.equal(shipped.beats.at(-1), "board");
});

test("nothing to read is held for less than 1.5 or more than 2.5 seconds", () => {
  for (const beat of READING_BEATS) {
    assert.ok(
      PACE[beat] >= 1500 && PACE[beat] <= 2500,
      `${beat}: ${PACE[beat]}`,
    );
  }
  assert.equal(PACE.glance, 2000, "the Review card is open for 2 s");
});

test("no beat is faster than a person", () => {
  assert.ok(PACE.glide >= 400, "a glide, not a jump");
  assert.ok(PACE.scroll >= 800, "a slow scroll");
});

test("a beat without a pace is refused", () => {
  assert.throws(
    () => plannedMs([{ id: "x", shows: "x", beats: ["teleport"] }]),
    /no pace for the beat "teleport"/,
  );
});

test("the trim cuts the recording to the storyboard and re-encodes VP9", () => {
  const args = trimArgs("raw.webm", "out/board.webm", 1234, 28600);
  assert.ok(args.every((arg) => typeof arg === "string"));
  assert.deepEqual(args.slice(args.indexOf("-ss"), args.indexOf("-ss") + 4), [
    "-ss",
    "1.234",
    "-i",
    "raw.webm",
  ]);
  assert.equal(args[args.indexOf("-t") + 1], "28.600");
  assert.equal(args[args.indexOf("-c:v") + 1], "libvpx-vp9");
  assert.ok(args.includes("-an"));
  assert.equal(args.at(-1), "out/board.webm");
  assert.equal(
    trimArgs("a", "b", -5, 10)[trimArgs("a", "b", -5, 10).indexOf("-ss") + 1],
    "0.000",
  );
});

test("the MP4 is H.264, yuv420p, faststart and 1440 wide", () => {
  const args = mp4Args("board.webm", "board.mp4");
  const after = (flag) => args[args.indexOf(flag) + 1];
  assert.equal(after("-i"), "board.webm");
  assert.equal(after("-c:v"), "libx264");
  assert.equal(after("-pix_fmt"), "yuv420p");
  assert.equal(after("-movflags"), "+faststart");
  assert.equal(after("-vf"), "scale=1440:-2:flags=lanczos");
  assert.ok(args.includes("-y"));
  assert.equal(args.at(-1), "board.mp4");
});

test("the GIF is two passes over one palette", () => {
  const attempt = { width: 960, fps: 12 };
  const palette = gifPaletteArgs("board.webm", "palette.png", attempt);
  assert.equal(
    palette[palette.indexOf("-vf") + 1],
    "fps=12,scale=960:-1:flags=lanczos,palettegen=stats_mode=diff",
  );
  assert.equal(palette.at(-1), "palette.png");

  const gif = gifArgs("board.webm", "palette.png", "board.gif", attempt);
  assert.deepEqual(
    gif.filter((_, i) => gif[i - 1] === "-i"),
    ["board.webm", "palette.png"],
  );
  assert.equal(
    gif[gif.indexOf("-lavfi") + 1],
    "fps=12,scale=960:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
  );
  assert.equal(gif.at(-1), "board.gif");
});

test("the GIF drops to 10 fps, then to 800 wide, to come in under 6 MB", () => {
  assert.deepEqual(GIF_ATTEMPTS, [
    { width: 960, fps: 12 },
    { width: 960, fps: 10 },
    { width: 800, fps: 10 },
  ]);
  assert.equal(GIF_LIMIT_BYTES, 6_000_000);
  assert.deepEqual(afterGifAttempt(0, 5_999_999), { keep: true, over: false });
  assert.deepEqual(afterGifAttempt(0, 6_000_000), { keep: false, next: 1 });
  assert.deepEqual(afterGifAttempt(1, 7_000_000), { keep: false, next: 2 });
  assert.deepEqual(afterGifAttempt(2, 4_000_000), { keep: true, over: false });
  assert.deepEqual(afterGifAttempt(2, 6_500_000), { keep: true, over: true });
  assert.deepEqual(afterGifAttempt(0, 10, { limit: 5 }), {
    keep: false,
    next: 1,
  });
});

test("sizes print in kB under a megabyte, MB above", () => {
  assert.equal(formatBytes(512_000), "512 kB");
  assert.equal(formatBytes(5_432_100), "5.4 MB");
});

test("ffmpeg is found only when it runs", () => {
  assert.equal(
    findFfmpeg(() => ({ status: 0 })),
    "ffmpeg",
  );
  assert.equal(
    findFfmpeg(() => ({ status: null, error: new Error("ENOENT") })),
    null,
  );
  assert.equal(
    findFfmpeg(() => ({ status: 1 })),
    null,
  );
});

test("without ffmpeg the recording is copied and stale files go", (t) => {
  const dir = scratchDir(t);
  const raw = path.join(dir, "raw.webm");
  const out = path.join(dir, "out");
  writeFileSync(raw, "recording");
  writeOutputs({ raw, outDir: out, ffmpeg: null, scratch: dir, log: () => {} });
  writeFileSync(path.join(out, "board.mp4"), "old");
  writeFileSync(path.join(out, "board.gif"), "old");
  const logged = [];
  const result = writeOutputs({
    raw,
    outDir: out,
    ffmpeg: null,
    scratch: dir,
    log: (line) => logged.push(line),
  });
  assert.equal(result.trimmed, false);
  assert.deepEqual(result.files, [{ name: "board.webm", bytes: 9 }]);
  assert.equal(readFileSync(path.join(out, "board.webm"), "utf8"), "recording");
  assert.ok(!existsSync(path.join(out, "board.mp4")));
  assert.ok(!existsSync(path.join(out, "board.gif")));
  assert.equal(logged.length, 2);
});

test(
  "with ffmpeg the three files are made from a recording",
  { skip: findFfmpeg() ? false : "no ffmpeg on the PATH" },
  (t) => {
    const dir = scratchDir(t);
    const raw = path.join(dir, "raw.webm");
    // A stand-in for Playwright's recording: VP8 at the board's size.
    execFileSync("ffmpeg", [
      "-hide_banner",
      "-loglevel",
      "error",
      "-f",
      "lavfi",
      "-i",
      "testsrc2=size=1440x900:rate=25:duration=3",
      "-c:v",
      "libvpx",
      "-b:v",
      "1M",
      raw,
    ]);
    const out = path.join(dir, "out");
    const result = writeOutputs({
      raw,
      outDir: out,
      startMs: 500,
      durationMs: 2000,
      ffmpeg: "ffmpeg",
      scratch: dir,
      log: () => {},
    });
    assert.equal(result.trimmed, true);
    assert.deepEqual(result.gif, { width: 960, fps: 12, over: false });
    const probe = (file) =>
      JSON.parse(
        execFileSync(
          "ffprobe",
          [
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,pix_fmt:format=duration",
            "-of",
            "json",
            path.join(out, file),
          ],
          { encoding: "utf8" },
        ),
      );
    const webm = probe("board.webm");
    assert.equal(webm.streams[0].codec_name, "vp9");
    assert.ok(
      Math.abs(Number(webm.format.duration) - 2) < 0.2,
      webm.format.duration,
    );
    const mp4 = probe("board.mp4");
    assert.equal(mp4.streams[0].codec_name, "h264");
    assert.equal(mp4.streams[0].pix_fmt, "yuv420p");
    assert.equal(mp4.streams[0].width, 1440);
    const gif = probe("board.gif");
    assert.equal(gif.streams[0].codec_name, "gif");
    assert.equal(gif.streams[0].width, 960);
    // faststart: the index (moov) comes before the media (mdat).
    const bytes = readFileSync(path.join(out, "board.mp4")).toString("latin1");
    assert.ok(bytes.indexOf("moov") < bytes.indexOf("mdat"));
  },
);
