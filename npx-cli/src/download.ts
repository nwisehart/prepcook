import https from 'https';
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import os from 'os';
import { execFileSync } from 'child_process';
import type { IncomingMessage } from 'http';

// Binaries come from the GitHub release for this CLI's version:
//   https://github.com/nwisehart/prepcook/releases/download/v<version>/prepcook-<version>-<platform>.tar.gz
// each holding a prepcook-<version>-<platform>/ directory with prepcook, prepcook-mcp,
// LICENSE and NOTICE, plus a SHA256SUMS file covering every tarball.
export const REPO = 'nwisehart/prepcook';
export const CACHE_DIR = path.join(os.homedir(), '.prepcook', 'bin');

// Local development mode: use the tarball local-build.sh writes to
// npx-cli/dist/ instead of downloading a release.
export const LOCAL_DIST_DIR = path.join(__dirname, '..', 'dist');
export const LOCAL_DEV_MODE =
  fs.existsSync(LOCAL_DIST_DIR) || process.env.PREPCOOK_LOCAL === '1';

type ProgressCallback = (downloaded: number, total: number) => void;

export function releaseName(version: string, platform: string): string {
  return `prepcook-${version}-${platform}`;
}

function releaseUrl(version: string, file: string): string {
  return `https://github.com/${REPO}/releases/download/v${version}/${file}`;
}

const REDIRECTS = new Set([301, 302, 303, 307, 308]);

function get(
  url: string,
  onResponse: (res: IncomingMessage) => void,
  onError: (err: Error) => void,
  redirectsLeft = 5
): void {
  https
    .get(url, { headers: { 'User-Agent': 'prepcook-npx-cli' } }, (res) => {
      if (REDIRECTS.has(res.statusCode ?? 0) && res.headers.location) {
        res.resume();
        if (redirectsLeft === 0) {
          onError(new Error(`Too many redirects fetching ${url}`));
          return;
        }
        get(
          new URL(res.headers.location, url).toString(),
          onResponse,
          onError,
          redirectsLeft - 1
        );
        return;
      }
      if (res.statusCode !== 200) {
        res.resume();
        onError(new Error(`HTTP ${res.statusCode} fetching ${url}`));
        return;
      }
      onResponse(res);
    })
    .on('error', onError);
}

function fetchText(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    get(
      url,
      (res) => {
        let data = '';
        res.setEncoding('utf8');
        res.on('data', (chunk: string) => (data += chunk));
        res.on('end', () => resolve(data));
        res.on('error', reject);
      },
      reject
    );
  });
}

function downloadFile(
  url: string,
  destPath: string,
  expectedSha256: string,
  onProgress?: ProgressCallback
): Promise<void> {
  const tempPath = destPath + '.tmp';
  const cleanup = () => {
    try {
      fs.unlinkSync(tempPath);
    } catch {}
  };
  return new Promise((resolve, reject) => {
    get(
      url,
      (res) => {
        const file = fs.createWriteStream(tempPath);
        const hash = crypto.createHash('sha256');
        const totalSize = parseInt(res.headers['content-length'] || '0', 10);
        let downloadedSize = 0;

        res.on('data', (chunk: Buffer) => {
          downloadedSize += chunk.length;
          hash.update(chunk);
          if (onProgress) onProgress(downloadedSize, totalSize);
        });
        res.on('error', (err) => {
          file.close();
          cleanup();
          reject(err);
        });
        res.pipe(file);

        file.on('finish', () => {
          file.close();
          const actualSha256 = hash.digest('hex');
          if (actualSha256 !== expectedSha256) {
            cleanup();
            reject(
              new Error(
                `Checksum mismatch for ${url}: expected ${expectedSha256}, got ${actualSha256}`
              )
            );
            return;
          }
          try {
            fs.renameSync(tempPath, destPath);
            resolve();
          } catch (err) {
            cleanup();
            reject(err);
          }
        });
      },
      (err) => {
        cleanup();
        reject(err);
      }
    );
  });
}

/** Parses `sha256sum` output: `<hex>  <file>` per line. */
export function parseSha256Sums(text: string): Record<string, string> {
  const sums: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const match = /^([0-9a-f]{64})\s+\*?(.+?)\s*$/i.exec(line);
    if (match) sums[match[2]] = match[1].toLowerCase();
  }
  return sums;
}

function extract(tarball: string, destDir: string): void {
  // tar ships with macOS, Linux and Windows 10+.
  execFileSync('tar', ['-xzf', tarball, '-C', destDir], { stdio: 'inherit' });
}

/**
 * Returns the directory holding `prepcook` and `prepcook-mcp` for this version and
 * platform, downloading and unpacking the release tarball on first use.
 */
export async function ensureRelease(
  version: string,
  platform: string,
  onProgress?: ProgressCallback
): Promise<string> {
  const name = releaseName(version, platform);
  const tarballName = `${name}.tar.gz`;

  if (LOCAL_DEV_MODE) {
    const tarball = path.join(LOCAL_DIST_DIR, tarballName);
    if (!fs.existsSync(tarball)) {
      throw new Error(
        `Local build not found: ${tarball}\n` +
          `Run ./local-build.sh first to build the binaries.`
      );
    }
    const dir = path.join(LOCAL_DIST_DIR, name);
    fs.rmSync(dir, { recursive: true, force: true });
    extract(tarball, LOCAL_DIST_DIR);
    return dir;
  }

  const cacheDir = path.join(CACHE_DIR, `v${version}`, platform);
  const dir = path.join(cacheDir, name);
  if (fs.existsSync(path.join(dir, 'prepcook'))) return dir;

  fs.mkdirSync(cacheDir, { recursive: true });
  const sums = parseSha256Sums(
    await fetchText(releaseUrl(version, 'SHA256SUMS'))
  );
  const expected = sums[tarballName];
  if (!expected) {
    throw new Error(
      `Release v${version} has no build for ${platform} (no ${tarballName} in SHA256SUMS)`
    );
  }

  const tarball = path.join(cacheDir, tarballName);
  await downloadFile(
    releaseUrl(version, tarballName),
    tarball,
    expected,
    onProgress
  );
  extract(tarball, cacheDir);
  fs.unlinkSync(tarball);
  return dir;
}

/** The newest published release's version, without the leading `v`. */
export async function getLatestVersion(): Promise<string | undefined> {
  const body = await fetchText(
    `https://api.github.com/repos/${REPO}/releases/latest`
  );
  const tag = (JSON.parse(body) as { tag_name?: string }).tag_name;
  return tag?.replace(/^v/, '');
}
