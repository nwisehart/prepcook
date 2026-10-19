// Checks every internal reference in site/dist: each href, src and srcset in
// an HTML page that is not an absolute URL (http:, https:, mailto:, data:)
// must be relative, must not point at a .md file, must resolve to a file in
// dist, and its #anchor must be an id on the target page. Run on its own
// (`node check-links.mjs`) or as the last step of build.mjs.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function htmlFiles(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return htmlFiles(full);
    return entry.name.endsWith('.html') ? [full] : [];
  });
}

function decode(s) {
  return s
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function references(html) {
  const refs = [];
  for (const m of html.matchAll(/\s(href|src)="([^"]*)"/g)) {
    refs.push(decode(m[2]));
  }
  for (const m of html.matchAll(/\ssrcset="([^"]*)"/g)) {
    for (const candidate of m[1].split(',')) {
      refs.push(decode(candidate.trim().split(/\s+/)[0]));
    }
  }
  return refs;
}

export function checkLinks(dist) {
  const failures = [];
  const ids = new Map();
  const idsOf = (file) => {
    if (!ids.has(file)) {
      const html = fs.readFileSync(file, 'utf8');
      ids.set(
        file,
        new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => decode(m[1])))
      );
    }
    return ids.get(file);
  };

  for (const file of htmlFiles(dist)) {
    const page = path.relative(dist, file);
    const html = fs.readFileSync(file, 'utf8');
    for (const ref of references(html)) {
      if (/^(?:https?:|mailto:|data:)/i.test(ref)) continue;
      if (ref === '') {
        failures.push(`${page}: empty reference`);
        continue;
      }
      if (ref.startsWith('/')) {
        failures.push(`${page}: root-absolute reference ${ref}`);
        continue;
      }
      if (/^[a-z][a-z0-9+.-]*:/i.test(ref)) {
        failures.push(`${page}: unexpected scheme ${ref}`);
        continue;
      }
      const hashAt = ref.indexOf('#');
      const target = (hashAt === -1 ? ref : ref.slice(0, hashAt)).split('?')[0];
      const anchor =
        hashAt === -1 ? '' : decodeURIComponent(ref.slice(hashAt + 1));
      if (/\.md$/i.test(target)) {
        failures.push(`${page}: link to markdown ${ref}`);
        continue;
      }
      let resolved =
        target === ''
          ? file
          : path.resolve(path.dirname(file), decodeURI(target));
      if (!resolved.startsWith(dist)) {
        failures.push(`${page}: reference outside the site ${ref}`);
        continue;
      }
      if (fs.existsSync(resolved) && fs.statSync(resolved).isDirectory()) {
        resolved = path.join(resolved, 'index.html');
      }
      if (!fs.existsSync(resolved)) {
        failures.push(`${page}: missing ${ref}`);
        continue;
      }
      if (
        anchor &&
        resolved.endsWith('.html') &&
        !idsOf(resolved).has(anchor)
      ) {
        failures.push(
          `${page}: no #${anchor} on ${path.relative(dist, resolved)}`
        );
      }
    }
  }
  return failures;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dist = path.join(path.dirname(fileURLToPath(import.meta.url)), 'dist');
  if (!fs.existsSync(dist)) {
    console.error('link check: site/dist does not exist; run the build first');
    process.exit(1);
  }
  const failures = checkLinks(dist);
  for (const f of failures) console.error(`link check: ${f}`);
  if (failures.length) process.exit(1);
  console.log('link check: every internal reference resolves');
}
