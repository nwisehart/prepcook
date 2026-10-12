import { spawn } from "child_process";
import path from "path";
import fs from "fs";
import {
  CACHE_DIR,
  LOCAL_DEV_MODE,
  ensureRelease,
  getLatestVersion,
} from "./download";

const CLI_VERSION: string = require("../package.json").version;

// The platforms a release is built for (see .github/workflows/release.yml).
function getPlatform(): string {
  const { platform, arch } = process;
  if (platform === "linux" && arch === "x64") return "linux-x64";
  if (platform === "darwin" && arch === "x64") return "macos-x64";
  if (platform === "darwin" && arch === "arm64") return "macos-arm64";

  console.error(`No prebuilt Prep Cook for ${platform}-${arch}.`);
  console.error("Releases cover Linux x64, macOS x64 and macOS arm64;");
  console.error(
    "elsewhere, build from source: https://github.com/nwisehart/prepcook#install",
  );
  process.exit(1);
}

// Remove other versions from the binary cache once this one is ready.
function cleanOldVersions(): void {
  try {
    for (const entry of fs.readdirSync(CACHE_DIR, { withFileTypes: true })) {
      if (entry.isDirectory() && entry.name !== `v${CLI_VERSION}`) {
        fs.rmSync(path.join(CACHE_DIR, entry.name), {
          recursive: true,
          force: true,
        });
      }
    }
  } catch {
    // Not critical.
  }
}

function showProgress(downloaded: number, total: number): void {
  const percent = total ? Math.round((downloaded / total) * 100) : 0;
  const mb = (downloaded / (1024 * 1024)).toFixed(1);
  const totalMb = total ? (total / (1024 * 1024)).toFixed(1) : "?";
  process.stderr.write(
    `\r   Downloading: ${mb}MB / ${totalMb}MB (${percent}%)`,
  );
}

function checkForUpdates(): void {
  if (LOCAL_DEV_MODE) return;
  getLatestVersion()
    .then((latest) => {
      if (latest && latest !== CLI_VERSION) {
        setTimeout(() => {
          console.error(`\nUpdate available: ${CLI_VERSION} -> ${latest}`);
        }, 2000);
      }
    })
    .catch(() => {});
}

async function run(binary: "prepcook" | "prepcook-mcp", args: string[]): Promise<void> {
  const platform = getPlatform();
  let dir: string;
  try {
    dir = await ensureRelease(CLI_VERSION, platform, showProgress);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`\nCould not get Prep Cook ${CLI_VERSION}: ${msg}`);
    process.exit(1);
  }
  if (!LOCAL_DEV_MODE) cleanOldVersions();

  const bin = path.join(dir, binary);
  if (!fs.existsSync(bin)) {
    console.error(`${bin} is missing; delete ${dir} and try again.`);
    process.exit(1);
  }

  const proc = spawn(bin, args, { stdio: "inherit" });
  proc.on("exit", (code, signal) => process.exit(signal ? 1 : (code ?? 0)));
  proc.on("error", (e) => {
    console.error(`${binary} failed to start: ${e.message}`);
    process.exit(1);
  });
  for (const sig of ["SIGINT", "SIGTERM"] as const) {
    process.on(sig, () => proc.kill(sig));
  }
}

// `prepcook mcp [args]` (or `--mcp`) runs the MCP server; `--version` prints
// this CLI's version; anything else goes to the prepcook binary as it is, so
// `npx prepcook up` is `prepcook up`.
function main(): void {
  const args = process.argv.slice(2);
  if (args[0] === "--version" || args[0] === "-v") {
    console.log(CLI_VERSION);
    return;
  }
  const mcpIndex =
    args[0] === "mcp" ? 0 : args.findIndex((a) => a === "--mcp");
  if (mcpIndex !== -1) {
    const rest = args.filter((_, i) => i !== mcpIndex);
    void run("prepcook-mcp", rest.length > 0 ? rest : ["--mode", "global"]);
    return;
  }
  checkForUpdates();
  void run("prepcook", args);
}

main();
