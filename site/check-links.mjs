// Checks every internal reference in site/dist: each href, src, poster and
// srcset candidate (every one of them, so each WebP size of a screenshot) in
// an HTML page that is not an absolute URL (http:, https:, mailto:, data:)
// must be relative, must not point at a .md file, must resolve to a file in
// dist, and its #anchor must be an id on the target page.
//
// The site is served at https://prepcook.dev/ (SITE_URL). Absolute URLs under
// it are allowed only where an absolute URL is required: <link rel="canonical">
// and the og:url and og:image metas. For those, SITE_URL stands for the root
// of dist: the path after it must resolve to a file there, and the canonical
// and og:url must both name the page they sit on (index.html as the bare
// root). An absolute self-URL anywhere else fails the check.
//
// sitemap.xml must list every page in dist, and every <loc> in it must be a
// page in dist.
//
// blog/feed.xml, the RSS feed, is read away from the site, so every URL in
// it (each <link>, <guid> and atom:link, and each href, src and srcset
// candidate in a post's content:encoded) must be absolute; one under
// SITE_URL must resolve to a file in dist, its #anchor to an id there, and
// each item's <link> to a page.
//
// Run on its own (`node check-links.mjs`) or as the last step of build.mjs.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SITE_URL = 'https://prepcook.dev/';
const SELF = /^https?:\/\/(?:www\.)?prepcook\.dev(?:[/?#]|$)/i;

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

// The <link rel="canonical"> href and the og:url / og:image contents, each
// with the tag it came from, and the HTML with those tags removed so the
// general scan below does not see them again.
function selfUrls(html) {
  const found = [];
  const rest = html.replace(/<(link|meta)\b[^>]*>/gi, (tag) => {
    const attr = (name) => {
      const m = tag.match(new RegExp(`\\s${name}="([^"]*)"`));
      return m ? decode(m[1]) : null;
    };
    const rel = attr('rel');
    const property = attr('property');
    if (rel === 'canonical') {
      found.push({ kind: 'canonical', url: attr('href') ?? '' });
      return '';
    }
    if (property === 'og:url' || property === 'og:image') {
      found.push({ kind: property, url: attr('content') ?? '' });
      return '';
    }
    return tag;
  });
  return { found, rest };
}

function references(html) {
  const refs = [];
  for (const m of html.matchAll(/\s(href|src|poster)="([^"]*)"/g)) {
    refs.push(decode(m[2]));
  }
  for (const m of html.matchAll(/\ssrcset="([^"]*)"/g)) {
    for (const candidate of m[1].split(',')) {
      refs.push(decode(candidate.trim().split(/\s+/)[0]));
    }
  }
  return refs;
}

// The pages sitemap.xml lists that are not in dist, and the pages in dist it
// does not list.
function checkSitemap(dist) {
  const failures = [];
  const file = path.join(dist, 'sitemap.xml');
  if (!fs.existsSync(file)) return ['sitemap.xml is missing'];
  const listed = new Set();
  for (const m of fs
    .readFileSync(file, 'utf8')
    .matchAll(/<loc>([^<]*)<\/loc>/g)) {
    const url = decode(m[1]);
    if (!url.startsWith(SITE_URL)) {
      failures.push(`sitemap.xml: <loc> ${url} is not under ${SITE_URL}`);
      continue;
    }
    const page = decodeURI(url.slice(SITE_URL.length)) || 'index.html';
    const resolved = path.join(dist, page);
    if (
      !page.endsWith('.html') ||
      !fs.existsSync(resolved) ||
      !fs.statSync(resolved).isFile()
    ) {
      failures.push(`sitemap.xml: <loc> ${url} is not a page in the site`);
      continue;
    }
    listed.add(page);
  }
  for (const file of htmlFiles(dist)) {
    const page = path.relative(dist, file).split(path.sep).join('/');
    if (!listed.has(page)) failures.push(`sitemap.xml does not list ${page}`);
  }
  return failures;
}

function idsReader() {
  const ids = new Map();
  return (file) => {
    if (!ids.has(file)) {
      const html = fs.readFileSync(file, 'utf8');
      ids.set(
        file,
        new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => decode(m[1])))
      );
    }
    return ids.get(file);
  };
}

// The feed's URLs that are not absolute, or that point into the site at
// something dist does not have.
function checkFeed(dist, idsOf) {
  const file = path.join(dist, 'blog', 'feed.xml');
  if (!fs.existsSync(file)) return ['blog/feed.xml is missing'];
  const xml = fs.readFileSync(file, 'utf8');
  const failures = [];
  const urls = [];
  for (const m of xml.matchAll(/<(link|guid)\b[^>]*>([^<]*)<\/\1>/g)) {
    urls.push({ url: decode(m[2].trim()), item: false });
  }
  for (const m of xml.matchAll(/<item>[\s\S]*?<link>([^<]*)<\/link>/g)) {
    urls.push({ url: decode(m[1].trim()), item: true });
  }
  for (const m of xml.matchAll(/<atom:link\b[^>]*\shref="([^"]*)"/g)) {
    urls.push({ url: decode(m[1]), item: false });
  }
  for (const m of xml.matchAll(/<!\[CDATA\[([\s\S]*?)\]\]>/g)) {
    for (const url of references(m[1])) urls.push({ url, item: false });
  }
  for (const { url, item } of urls) {
    if (!/^(?:https?:|mailto:)/i.test(url)) {
      failures.push(`blog/feed.xml: ${url || 'an empty URL'} is not absolute`);
      continue;
    }
    if (!url.startsWith(SITE_URL)) {
      if (SELF.test(url))
        failures.push(`blog/feed.xml: ${url} is not under ${SITE_URL}`);
      continue;
    }
    const rest = url.slice(SITE_URL.length);
    const hashAt = rest.indexOf('#');
    const target = decodeURI(
      (hashAt === -1 ? rest : rest.slice(0, hashAt)).split('?')[0]
    );
    const anchor =
      hashAt === -1 ? '' : decodeURIComponent(rest.slice(hashAt + 1));
    let resolved = path.join(dist, target);
    if (fs.existsSync(resolved) && fs.statSync(resolved).isDirectory()) {
      resolved = path.join(resolved, 'index.html');
    }
    if (!fs.existsSync(resolved)) {
      failures.push(`blog/feed.xml: ${url} is missing from the site`);
      continue;
    }
    if (item && !resolved.endsWith('.html')) {
      failures.push(`blog/feed.xml: item link ${url} is not a page`);
    }
    if (anchor && resolved.endsWith('.html') && !idsOf(resolved).has(anchor)) {
      failures.push(
        `blog/feed.xml: no #${anchor} on ${path.relative(dist, resolved)}`
      );
    }
  }
  return failures;
}

export function checkLinks(dist) {
  const idsOf = idsReader();
  const failures = [...checkSitemap(dist), ...checkFeed(dist, idsOf)];

  for (const file of htmlFiles(dist)) {
    const page = path.relative(dist, file);
    const { found, rest: html } = selfUrls(fs.readFileSync(file, 'utf8'));
    const ownPath = page === 'index.html' ? '' : page.split(path.sep).join('/');
    for (const { kind, url } of found) {
      if (!url.startsWith(SITE_URL)) {
        failures.push(`${page}: ${kind} is not under ${SITE_URL}: ${url}`);
        continue;
      }
      const rest = url.slice(SITE_URL.length);
      if ((kind === 'canonical' || kind === 'og:url') && rest !== ownPath) {
        failures.push(`${page}: ${kind} ${url} does not name this page`);
        continue;
      }
      let resolved = path.join(dist, decodeURI(rest.split(/[?#]/)[0]));
      if (fs.existsSync(resolved) && fs.statSync(resolved).isDirectory()) {
        resolved = path.join(resolved, 'index.html');
      }
      if (!fs.existsSync(resolved)) {
        failures.push(`${page}: ${kind} ${url} is missing from the site`);
      }
    }
    for (const ref of references(html)) {
      if (SELF.test(ref)) {
        failures.push(`${page}: absolute self-link ${ref}; use a relative one`);
        continue;
      }
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
