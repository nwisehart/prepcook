// Generates the Prepcook website into site/dist from the repository's own
// documentation (README.md, CONTRIBUTING.md, docs/*.md) plus three
// hand-written pages (src/home.html, src/tammy.html, src/vibe-kanban.html). The site is served at
// https://prepcook.dev/ (SITE_URL). Every link and asset reference it writes
// is relative, so dist works the same from any host or a local preview; the
// only absolute self-URLs are the canonical and Open Graph URLs in each page's
// head, the JSON-LD on the home and Tammy pages, sitemap.xml and robots.txt,
// which social cards and search engines require. The link check in
// check-links.mjs runs at the end and fails the build on any broken reference.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked } from 'marked';
import sharp from 'sharp';
import { checkLinks } from './check-links.mjs';

const SITE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(SITE, '..');
const SRC = path.join(SITE, 'src');
const DIST = path.join(SITE, 'dist');
const GITHUB = 'https://github.com/nwisehart/prepcook';
const SITE_URL = 'https://prepcook.dev/';

// The sidebar, in reading order. `src` is the markdown file in the repository
// (or a hand-written HTML article under site/src), `out` the page it becomes in
// dist, `label` its sidebar entry. Every docs/*.md must appear here: the build
// fails on one that does not.
const GROUPS = [
  {
    title: 'Get started',
    pages: [
      { src: 'README.md', out: 'get-started.html', label: 'Get started' },
    ],
  },
  {
    title: 'Method',
    pages: [
      { src: 'docs/method.md', label: 'The method' },
      { src: 'docs/reproduce-first.md', label: 'Reproduce first' },
      { src: 'docs/approvals.md', label: 'Plan decisions' },
      { src: 'docs/review.md', label: 'The independent review' },
      {
        src: 'docs/working-alongside-people.md',
        label: 'Working alongside people',
      },
      { src: 'docs/rca-graph.md', label: 'Knowledge and the RCA graph' },
    ],
  },
  {
    title: 'Integrations',
    pages: [
      { src: 'docs/notifications.md', label: 'Notifications' },
      { src: 'docs/evidence.md', label: 'Evidence' },
      { src: 'docs/incidents.md', label: 'Incidents' },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { src: 'docs/configuration.md', label: 'Configuration' },
      { src: 'docs/scripts.md', label: '.prepcook/scripts.toml', mono: true },
      { src: 'docs/api.md', label: 'The HTTP API' },
    ],
  },
  {
    title: 'Developers',
    pages: [
      { src: 'docs/developing.md', label: 'Developing Prepcook' },
      { src: 'docs/playbooks.md', label: 'Playbooks' },
      { src: 'docs/manual-testing.md', label: 'Manual testing' },
      {
        src: 'CONTRIBUTING.md',
        out: 'contributing.html',
        label: 'Contributing',
      },
      {
        src: 'site/src/vibe-kanban.html',
        out: 'vibe-kanban.html',
        label: 'Prepcook and vibe-kanban',
      },
    ],
  },
];

for (const group of GROUPS) {
  for (const page of group.pages) {
    page.out ??= page.src.replace(/\.md$/, '.html');
    page.group = group.title;
  }
}
const PAGES = GROUPS.flatMap((g) => g.pages);
const PAGE_BY_SRC = new Map(PAGES.map((p) => [p.src, p]));

const HOME_TITLE =
  'Prepcook: an orchestrator that reproduces, explains and fixes bugs with coding agents';

// Each page's meta description, written for the search-result snippet: what
// the page tells you, in the words people search for, between
// DESCRIPTION_MIN and DESCRIPTION_MAX characters (the build checks). Keyed by
// source path; a page missing here falls back to its first paragraph. The home
// page's is the README's tagline (homeDescription).
const DESCRIPTION_MIN = 120;
const DESCRIPTION_MAX = 160;
const DESCRIPTIONS = {
  'README.md':
    'Install Prepcook with npx or a release, then take a demo bug from report to pull request with Claude Code: reproduce it, explain it, fix it and review it.',
  'docs/method.md':
    'How Prepcook works a bug with coding agents on a kanban board: Plan, Execute, Review and Shipped, investigation cards, and the Chef and Prep modes.',
  'docs/reproduce-first.md':
    'Why a Prepcook planning agent must reproduce a bug with a failing harness before it writes a root-cause analysis, and what that harness has to assert.',
  'docs/approvals.md':
    'How a person approves a plan, asks for changes or sends it back in Prepcook: in the review page, a Jira or GitHub issue comment, a card comment or the API.',
  'docs/review.md':
    'How Prepcook reviews a fix with a second coding agent that never saw the plan: what it reads, its verdicts, the fix rounds and the check before merging.',
  'docs/working-alongside-people.md':
    'How Prepcook shares tracker tickets with engineers: taking a ticket over by assigning it, when it comments, the daily update and reopened tickets.',
  'docs/rca-graph.md':
    'Prepcook keeps every approved root-cause analysis in a knowledge store and a graph of root-cause classes, and opens a pattern review when a cause recurs.',
  'docs/notifications.md':
    'Send Prepcook events to Slack, a signed webhook, Discord or Microsoft Teams: a plan waiting for a person, a failing pipeline, the daily digest and more.',
  'docs/evidence.md':
    'Give coding agents evidence from Sentry, Datadog, DataSet, CloudWatch Logs and log files while they reproduce a bug, through evidence_search and MCP.',
  'docs/incidents.md':
    'Incidents mode: each open PagerDuty incident becomes a Prepcook investigation, reproduced and explained, and the triage report goes back as a note.',
  'docs/configuration.md':
    'Every Prepcook setting and its default: the orchestrator, the coding agent per stage, Jira and GitHub Issues ticketing, notifications, evidence and secrets.',
  'docs/scripts.md':
    'How a repository tells Prepcook to install, start, seed and test its app in .prepcook/scripts.toml, so a coding agent can reproduce bugs locally or remotely.',
  'docs/api.md':
    "Prepcook's stable HTTP API for scripts and services: file cards with a bearer token, decide on plans, search evidence, and run the app's scripts.",
  'docs/developing.md':
    'How Prepcook is built in Rust and TypeScript: the architecture, the repository layout, the build and test commands, and adding a provider or coding agent.',
  'docs/playbooks.md':
    "The prompts Prepcook's coding agents start from, one per stage and work class: their placeholders, the review gate, and how to change a playbook.",
  'docs/manual-testing.md':
    'The end-to-end check run by hand before each Prepcook release: a demo bug to a merged pull request, investigations, notifications, GitHub Issues and Tammy.',
  'CONTRIBUTING.md':
    'How to contribute to Prepcook, an Apache-2.0 orchestrator for coding agents: opening an issue, pull requests, adding a provider or agent, and playbooks.',
  'site/src/tammy.html':
    'Tammy is a Slack agent that answers customers from your docs, files what looks broken as a Prepcook investigation and explains the root-cause analysis plainly.',
  'site/src/vibe-kanban.html':
    'Prepcook is a fork of vibe-kanban by BloopAI: what it keeps from that kanban board for coding agents, and what it adds to reproduce, explain and fix bugs.',
};

// Files copied into dist at the same relative path they have in the
// repository, so a reference that resolves in the repository resolves on the
// site too.
const COPIED = [
  ...listFiles('docs/images', (f) => f.endsWith('.png')),
  ...listFiles('assets/brand', (f) => f.endsWith('.svg')),
];
const COPIED_SET = new Set(COPIED);

// The screenshots are 2880 pixels wide; the content column is at most 880.
// Each docs/images PNG is also served as WebP, at full size and at
// WEBP_SMALL wide, and every <img> of one the site emits becomes a <picture>
// offering both through srcset, with the PNG as the fallback.
const SCREENSHOT = /^docs\/images\/[^/]+\.png$/;
const WEBP_QUALITY = 82;
const WEBP_SMALL = 880;
const IMAGE_SIZES = '(max-width: 920px) 100vw, 880px';

const problems = [];

function listFiles(dir, keep) {
  return fs
    .readdirSync(path.join(REPO, dir))
    .filter(keep)
    .sort()
    .map((f) => `${dir}/${f}`);
}

function read(rel) {
  return fs.readFileSync(path.join(REPO, rel), 'utf8');
}

function write(rel, content) {
  const file = path.join(DIST, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
}

function copy(fromAbs, rel) {
  const file = path.join(DIST, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.copyFileSync(fromAbs, file);
}

function escapeHtml(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function decodeEntities(s) {
  return s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#(\d+);/g, (_, n) => String.fromCodePoint(Number(n)))
    .replace(/&amp;/g, '&');
}

function stripTags(html) {
  return decodeEntities(html.replace(/<[^>]*>/g, ''));
}

// A relative URL from the page at `fromOut` to `toOut` (both dist paths).
function relativeUrl(fromOut, toOut) {
  const rel = path.posix.relative(path.posix.dirname(fromOut), toOut);
  return rel === '' ? path.posix.basename(toOut) : rel;
}

function rootPrefix(out) {
  const depth = out.split('/').length - 1;
  return depth === 0 ? './' : '../'.repeat(depth);
}

// GitHub's heading ids: lowercase, punctuation and symbols dropped (hyphens
// and underscores kept), each space a hyphen; a repeated id gets -1, -2, ...
function makeSlugger() {
  const seen = new Map();
  return (text) => {
    const base = text
      .toLowerCase()
      .replace(/[^\p{L}\p{M}\p{N}\p{Pc}\- ]/gu, '')
      .replace(/ /g, '-');
    let slug = base;
    while (seen.has(slug)) {
      const n = seen.get(base) + 1;
      seen.set(base, n);
      slug = `${base}-${n}`;
    }
    seen.set(slug, 0);
    return slug;
  };
}

// Rewrites one URL found in the markdown of `srcFile` for the page `outFile`:
// rendered pages and copied images get relative site links, anything else in
// the repository links to the file on GitHub. A link to the site itself
// (SITE_URL...) becomes relative too, so the page body holds no absolute
// self-links.
function rewriteUrl(url, srcFile, outFile) {
  if (url.startsWith(SITE_URL)) {
    const rest = url.slice(SITE_URL.length);
    const hashAt = rest.indexOf('#');
    let target = hashAt === -1 ? rest : rest.slice(0, hashAt);
    if (target === '' || target.endsWith('/')) target += 'index.html';
    return (
      relativeUrl(outFile, target) + (hashAt === -1 ? '' : rest.slice(hashAt))
    );
  }
  if (/^(?:[a-z][a-z0-9+.-]*:|\/\/|#)/i.test(url) || url === '') return url;
  const hashAt = url.indexOf('#');
  const target = hashAt === -1 ? url : url.slice(0, hashAt);
  const anchor = hashAt === -1 ? '' : url.slice(hashAt);
  if (target.startsWith('/')) {
    problems.push(`${srcFile}: root-absolute link ${url}`);
    return url;
  }
  const repoPath = path.posix.normalize(
    path.posix.join(path.posix.dirname(srcFile), decodeURI(target))
  );
  if (repoPath.startsWith('..')) {
    problems.push(`${srcFile}: link outside the repository ${url}`);
    return url;
  }
  const page = PAGE_BY_SRC.get(repoPath);
  if (page) return relativeUrl(outFile, page.out) + anchor;
  if (COPIED_SET.has(repoPath)) return relativeUrl(outFile, repoPath) + anchor;
  const abs = path.join(REPO, repoPath);
  if (!fs.existsSync(abs)) {
    problems.push(`${srcFile}: link to a missing file ${url}`);
    return url;
  }
  const kind = fs.statSync(abs).isDirectory() ? 'tree' : 'blob';
  return `${GITHUB}/${kind}/main/${repoPath.replace(/\/$/, '')}${anchor}`;
}

// Applies rewriteUrl to every href, src and srcset in rendered HTML. Markdown
// links and the raw HTML blocks (<picture>, <img>) both arrive here as
// attributes; code is escaped by then, so its quotes never match.
function rewriteHtml(html, srcFile, outFile) {
  return html
    .replace(
      /(\s(?:href|src))="([^"]*)"/g,
      (_, attr, url) =>
        `${attr}="${rewriteUrl(decodeEntities(url), srcFile, outFile).replace(/&/g, '&amp;')}"`
    )
    .replace(/(\ssrcset)="([^"]*)"/g, (_, attr, value) => {
      const parts = value.split(',').map((candidate) => {
        const [url, ...descriptor] = candidate.trim().split(/\s+/);
        return [rewriteUrl(url, srcFile, outFile), ...descriptor].join(' ');
      });
      return `${attr}="${parts.join(', ')}"`;
    });
}

function codeBlockHtml(text, lang) {
  const label = lang ? escapeHtml(lang) : '';
  return (
    `<div class="code-block">` +
    `<div class="code-head"><span class="code-lang">${label}</span>` +
    `<button class="copy" type="button" hidden>Copy</button></div>` +
    `<pre><code${lang ? ` class="language-${label}"` : ''}>${escapeHtml(text)}</code></pre>` +
    `</div>\n`
  );
}

// Renders one markdown document. Returns the HTML, its first `#` heading as
// plain text and its `##` headings for the table of contents.
function renderMarkdown(markdown, srcFile, outFile) {
  const slug = makeSlugger();
  const toc = [];
  let title = null;
  const marked = new Marked({ gfm: true });
  marked.use({
    renderer: {
      heading({ tokens, depth }) {
        const inner = this.parser.parseInline(tokens);
        const text = stripTags(inner).trim();
        const id = slug(text);
        if (depth === 1) {
          if (title !== null)
            problems.push(`${srcFile}: a second # heading, "${text}"`);
          else title = text;
        }
        if (depth === 2) toc.push({ id, html: inner });
        return (
          `<h${depth} id="${id}">${inner}` +
          `<a class="anchor" href="#${id}" aria-label="Link to this section">#</a>` +
          `</h${depth}>\n`
        );
      },
      code({ text, lang }) {
        return codeBlockHtml(text, (lang || '').split(/\s/)[0]);
      },
      table(token) {
        return `<div class="table-wrap">${this.constructor.prototype.table.call(this, token)}</div>\n`;
      },
    },
  });
  const raw = marked.parse(markdown);
  return { html: rewriteHtml(raw, srcFile, outFile), title, toc };
}

function fill(template, values) {
  return template.replace(/\{\{([a-zA-Z0-9:_-]+)\}\}/g, (match, key) => {
    if (!(key in values)) throw new Error(`template value missing: ${key}`);
    return values[key];
  });
}

const LAYOUT = fs.readFileSync(path.join(SRC, 'layout.html'), 'utf8');

// The README's board screenshot alt text: the home page's hero and every
// page's og:image (the same screenshot) use it.
function boardAlt() {
  const m = read('README.md').match(
    /<img src="docs\/images\/board\.png" alt="([^"]+)"/
  );
  if (!m) throw new Error('README.md: the board screenshot is missing');
  return m[1];
}

// A page's path under SITE_URL, for its canonical and og:url: the dist path,
// except that index.html is the bare root.
function sitePath(out) {
  return out === 'index.html' ? '' : out;
}

// JSON-LD for a <script type="application/ld+json"> in the head. `<` is
// escaped so no string in it can close the script element.
function structuredData(data) {
  if (!data) return '';
  const json = JSON.stringify(data, null, 2).replace(/</g, '\\u003c');
  return `<script type="application/ld+json">\n${json}\n</script>`;
}

const APACHE_LICENSE = 'https://www.apache.org/licenses/LICENSE-2.0';
const AUTHOR = {
  '@type': 'Person',
  name: 'Nick Wisehart',
  url: 'https://github.com/nwisehart',
};

// The home page's structured data: Prepcook as a SoftwareApplication (typed
// SoftwareSourceCode too, which is where codeRepository and
// programmingLanguage are defined) and the site as a WebSite.
function homeStructuredData() {
  const { version } = JSON.parse(read('package.json'));
  return {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': ['SoftwareApplication', 'SoftwareSourceCode'],
        name: 'Prepcook',
        description: readmeTagline(),
        applicationCategory: 'DeveloperApplication',
        operatingSystem: 'macOS, Linux',
        offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
        license: APACHE_LICENSE,
        url: SITE_URL,
        downloadUrl: `${GITHUB}/releases`,
        softwareVersion: version,
        author: AUTHOR,
        codeRepository: GITHUB,
        programmingLanguage: ['Rust', 'TypeScript'],
        screenshot: `${SITE_URL}docs/images/board.png`,
      },
      { '@type': 'WebSite', name: 'Prepcook', url: SITE_URL },
    ],
  };
}

function tammyStructuredData() {
  return {
    '@context': 'https://schema.org',
    '@type': ['SoftwareApplication', 'SoftwareSourceCode'],
    name: 'Tammy',
    description: DESCRIPTIONS['site/src/tammy.html'],
    applicationCategory: 'BusinessApplication',
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    license: APACHE_LICENSE,
    url: `${SITE_URL}tammy.html`,
    author: AUTHOR,
    codeRepository: 'https://github.com/nwisehart/tammy',
    programmingLanguage: 'TypeScript',
  };
}

function page({
  out,
  title,
  description,
  main,
  section,
  docs = false,
  jsonLd = null,
}) {
  const root = rootPrefix(out);
  const html = fill(LAYOUT, {
    title: escapeHtml(title),
    description: escapeHtml(description),
    structuredData: structuredData(jsonLd),
    path: sitePath(out),
    boardAlt: escapeHtml(boardAlt()),
    root,
    bodyClass: docs ? 'has-sidebar' : 'plain',
    menuButton: docs
      ? `<button class="icon-button menu-button" type="button" aria-label="Open the docs menu" aria-expanded="false" aria-controls="sidebar"><svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true"><path d="M3 5.5h14M3 10h14M3 14.5h14" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg></button>`
      : '',
    main,
  });
  return responsiveImages(
    html.replace(
      `data-section="${section}"`,
      `data-section="${section}" aria-current="page"`
    ),
    out
  );
}

// A PNG's dimensions, from its IHDR chunk.
function pngSize(rel) {
  const head = Buffer.alloc(24);
  const fd = fs.openSync(path.join(REPO, rel), 'r');
  try {
    fs.readSync(fd, head, 0, 24, 0);
  } finally {
    fs.closeSync(fd);
  }
  if (head.toString('ascii', 12, 16) !== 'IHDR')
    throw new Error(`${rel}: not a PNG`);
  return { width: head.readUInt32BE(16), height: head.readUInt32BE(20) };
}

// Writes the WebP versions of every screenshot: <name>.webp at full size and
// <name>-880.webp, for the srcset responsiveImages writes.
async function writeWebp() {
  await Promise.all(
    COPIED.filter((rel) => SCREENSHOT.test(rel)).map(async (rel) => {
      const base = path.join(DIST, rel.replace(/\.png$/, ''));
      const input = path.join(REPO, rel);
      await sharp(input).webp({ quality: WEBP_QUALITY }).toFile(`${base}.webp`);
      if (pngSize(rel).width > WEBP_SMALL) {
        await sharp(input)
          .resize({ width: WEBP_SMALL })
          .webp({ quality: WEBP_QUALITY })
          .toFile(`${base}-${WEBP_SMALL}.webp`);
      }
    })
  );
}

// An element's attributes, values left as written (still HTML-escaped).
function tagAttributes(tag) {
  const attrs = new Map();
  for (const m of tag.matchAll(/\s([a-z][a-z-]*)(?:="([^"]*)")?/g)) {
    attrs.set(m[1], m[2] ?? '');
  }
  return attrs;
}

// The repository path of the screenshot `url` points at from the page at
// `out`, or null when it is not one.
function screenshotAt(url, out) {
  if (!url || /^(?:[a-z][a-z0-9+.-]*:|\/|#)/i.test(url)) return null;
  const rel = path.posix.normalize(
    path.posix.join(path.posix.dirname(out), url)
  );
  return SCREENSHOT.test(rel) && COPIED_SET.has(rel) ? rel : null;
}

function webpSource(rel, out, media, sizes) {
  const base = rel.replace(/\.png$/, '');
  const { width } = pngSize(rel);
  const candidates = [`${relativeUrl(out, `${base}.webp`)} ${width}w`];
  if (width > WEBP_SMALL)
    candidates.unshift(
      `${relativeUrl(out, `${base}-${WEBP_SMALL}.webp`)} ${WEBP_SMALL}w`
    );
  const mediaAttr = media === undefined ? '' : ` media="${media}"`;
  return `<source${mediaAttr} type="image/webp" srcset="${candidates.join(', ')}" sizes="${sizes}" />`;
}

// Rewrites every screenshot <img> on a page, alone or in a <picture>, to a
// <picture> that offers the WebP versions first: per theme, the -dark WebP
// and the -dark PNG under the original dark media query (which site.js
// switches with the theme toggle), then the light WebP, then the PNG <img>.
// The <img> gets the PNG's real width and height, so the layout does not
// shift as it loads, and loading="lazy" unless it is marked
// fetchpriority="high". The sources' sizes is IMAGE_SIZES, the docs column,
// unless the <img> carries its own `sizes` for a wider slot.
function responsiveImages(html, out) {
  return html.replace(
    /<picture>([\s\S]*?)<\/picture>|<img\b[^>]*>/g,
    (match, inner) => {
      const imgTag =
        inner === undefined ? match : inner.match(/<img\b[^>]*>/)?.[0];
      if (!imgTag) return match;
      const img = tagAttributes(imgTag);
      const light = screenshotAt(img.get('src'), out);
      if (!light) return match;
      const sizes = img.get('sizes') ?? IMAGE_SIZES;
      const sources = [];
      for (const tag of inner?.match(/<source\b[^>]*>/g) ?? []) {
        const source = tagAttributes(tag);
        const rel = screenshotAt(source.get('srcset'), out);
        if (!rel) {
          sources.push(tag);
          continue;
        }
        const media = source.get('media');
        sources.push(
          webpSource(rel, out, media, sizes),
          `<source${media === undefined ? '' : ` media="${media}"`} srcset="${source.get('srcset')}" />`
        );
      }
      sources.push(webpSource(light, out, undefined, sizes));
      const { width, height } = pngSize(light);
      const loading =
        img.get('fetchpriority') === 'high'
          ? ' fetchpriority="high"'
          : ' loading="lazy"';
      const others = [...img]
        .filter(
          ([name]) =>
            ![
              'src',
              'alt',
              'width',
              'height',
              'loading',
              'fetchpriority',
              'sizes',
            ].includes(name)
        )
        .map(([name, value]) => ` ${name}="${value}"`)
        .join('');
      return (
        `<picture>${sources.join('')}` +
        `<img src="${img.get('src')}" alt="${img.get('alt') ?? ''}" width="${width}" height="${height}"${loading}${others} />` +
        `</picture>`
      );
    }
  );
}

function sidebarHtml(current) {
  const groups = GROUPS.map((group) => {
    const items = group.pages
      .map((p) => {
        const label = p.mono
          ? `<code>${escapeHtml(p.label)}</code>`
          : escapeHtml(p.label);
        const active = p === current ? ' aria-current="page"' : '';
        return `<li><a href="${relativeUrl(current.out, p.out)}"${active}>${label}</a></li>`;
      })
      .join('');
    return `<div class="nav-group"><p class="nav-title">${escapeHtml(group.title)}</p><ul>${items}</ul></div>`;
  }).join('');
  return `<nav class="sidebar" id="sidebar" aria-label="Documentation">${groups}</nav><div class="scrim" hidden></div>`;
}

function tocHtml(toc) {
  if (toc.length < 2) return { aside: '', inline: '' };
  const list = `<ul>${toc
    .map(
      (t) =>
        `<li><a href="#${t.id}">${t.html.replace(/<a [^>]*>|<\/a>/g, '')}</a></li>`
    )
    .join('')}</ul>`;
  return {
    aside: `<aside class="toc" aria-label="On this page"><p class="nav-title">On this page</p>${list}</aside>`,
    inline: `<details class="toc-inline"><summary>On this page</summary>${list}</details>`,
  };
}

// A hand-written HTML article in the docs layout: its <h1> is the title and
// its <h2 id>s the table of contents. Its links are written relative to its
// place in dist, so they are used as they are.
function renderArticle(src) {
  const html = read(src);
  const h1 = html.match(/<h1[^>]*>([\s\S]*?)<\/h1>/);
  const toc = [...html.matchAll(/<h2 id="([^"]+)">([\s\S]*?)<\/h2>/g)].map(
    (m) => ({ id: m[1], html: m[2] })
  );
  return { html, title: h1 ? stripTags(h1[1]).trim() : null, toc };
}

function docPage(p, index) {
  const rendered = p.src.endsWith('.html')
    ? renderArticle(p.src)
    : renderMarkdown(read(p.src), p.src, p.out);
  const { toc } = rendered;
  let { html, title } = rendered;
  // The README opens with the wordmark rather than a # heading; the page is
  // named for where it sits in the docs.
  if (p.src === 'README.md') {
    title = 'Get started';
    html = `<h1 class="visually-hidden">${title}</h1>\n${html}`;
  }
  const pageTitle = title;
  if (!pageTitle) problems.push(`${p.src}: no # heading for the page title`);
  const prev = PAGES[index - 1];
  const next = PAGES[index + 1];
  const pager = [
    prev
      ? `<a class="pager-prev" href="${relativeUrl(p.out, prev.out)}"><span>Previous</span>${escapeHtml(prev.label)}</a>`
      : '<span></span>',
    next
      ? `<a class="pager-next" href="${relativeUrl(p.out, next.out)}"><span>Next</span>${escapeHtml(next.label)}</a>`
      : '<span></span>',
  ].join('');
  const { aside, inline } = tocHtml(toc);
  const main =
    `<div class="doc-layout">${sidebarHtml(p)}` +
    `<main class="doc-main" id="main"><p class="crumb">${escapeHtml(p.group)}</p>${inline}` +
    `<article class="prose">${html}</article>` +
    `<p class="edit"><a href="${GITHUB}/edit/main/${p.src}">Edit this page on GitHub</a></p>` +
    `<nav class="pager" aria-label="Previous and next">${pager}</nav></main>${aside}</div>`;
  write(
    p.out,
    page({
      out: p.out,
      title: `${pageTitle} · Prepcook`,
      description: description(p.src),
      main,
      section: 'docs',
      docs: true,
    })
  );
}

function description(src) {
  const text = DESCRIPTIONS[src];
  if (text === undefined) return firstParagraph(src);
  if (text.length < DESCRIPTION_MIN || text.length > DESCRIPTION_MAX)
    problems.push(
      `${src}: its description is ${text.length} characters; keep it within ${DESCRIPTION_MIN}–${DESCRIPTION_MAX}`
    );
  return text;
}

// The first prose paragraph of a markdown file, as plain text, for the
// meta description of a page DESCRIPTIONS does not cover.
function firstParagraph(src) {
  const tokens = new Marked({ gfm: true }).lexer(read(src));
  const para = tokens.find(
    (t) => t.type === 'paragraph' && !t.text.startsWith('<')
  );
  return para
    ? stripTags(new Marked().parseInline(para.text)).replace(/\s+/g, ' ').trim()
    : '';
}

// Where the README's problem paragraph turns from the problem to the method;
// the home page shows the two halves as separate paragraphs.
const PROBLEM_TURN = 'Prepcook puts those three steps';

// The README's tagline: its first paragraph, entirely in bold. It is the
// home page's lead and the start of its meta description.
function readmeTagline() {
  const first = new Marked({ gfm: true })
    .lexer(read('README.md'))
    .find((t) => t.type === 'paragraph' && !t.text.startsWith('<'));
  const m = first && first.text.match(/^\*\*([^*]+)\*\*$/);
  if (!m)
    throw new Error('README.md: the first paragraph must be the bold tagline');
  return m[1].replace(/\s+/g, ' ').trim();
}

// Pieces of README.md and docs/images/README.md the home page shows, read at
// build time so the home page cannot drift from them.
function homeValues() {
  const readme = read('README.md');
  const tokens = new Marked({ gfm: true }).lexer(readme);
  const inline = (md, src = 'README.md') =>
    rewriteHtml(new Marked({ gfm: true }).parseInline(md), src, 'index.html');

  // The opening: the one-line tagline in bold, the problem paragraph (split
  // in two on the home page where it turns from the problem to the method),
  // and the paragraph that explains the name.
  const paragraphs = tokens.filter(
    (t) => t.type === 'paragraph' && !t.text.startsWith('<')
  );
  const problem = paragraphs[1].text.replace(/\s+/g, ' ');
  const turn = problem.indexOf(PROBLEM_TURN);
  if (turn === -1)
    throw new Error(
      `README.md: the second paragraph must contain "${PROBLEM_TURN}"`
    );
  const kitchen = paragraphs.find((t) => t.text.startsWith('A prep cook '));
  if (!kitchen)
    throw new Error('README.md: no paragraph starting "A prep cook"');
  const kitchenSentences = kitchen.text
    .replace(/\s+/g, ' ')
    .split(/(?<=\.) (?=[A-Z])/);

  const alt = boardAlt();

  // "What it does": one paragraph per stage, each opening with **Stage.**
  const start = tokens.findIndex(
    (t) => t.type === 'heading' && t.text === 'What it does'
  );
  const stages = {};
  for (const t of tokens.slice(start + 1)) {
    if (t.type === 'heading') break;
    const m =
      t.type === 'paragraph' &&
      t.text.match(/^\*\*([A-Za-z ]+)\.\*\*\s+([\s\S]*)$/);
    if (m) stages[m[1]] = inline(m[2].replace(/\s+/g, ' '));
  }
  const helpers = {
    Plan: 'Reproduce it, then explain it',
    Execute: 'The smallest fix, with tests',
    Review: 'Fresh eyes on the fix',
    Shipped: 'PR open or merged; you clean up',
    'Clean up': 'Worktrees archived',
  };
  const stageHtml = Object.entries(helpers)
    .map(([name, helper], i) => {
      if (!stages[name])
        throw new Error(`README.md: no "What it does" paragraph for ${name}`);
      return (
        `<li class="stage"><div class="stage-name"><span class="stage-step">${i + 1}</span>` +
        `<h3>${name}</h3><p class="stage-helper">${helper}</p></div>` +
        `<p>${stages[name]}</p></li>`
      );
    })
    .join('\n');

  // The install commands from Quickstart step 1: the npx block (it starts
  // with `npx`) and the release-tarball block (the one that runs `curl`).
  const installAt = tokens.findIndex(
    (t) => t.type === 'heading' && t.text.startsWith('1. Install and start')
  );
  const installCode = [];
  for (const t of tokens.slice(installAt + 1)) {
    if (t.type === 'heading') break;
    if (t.type === 'code') installCode.push(t);
  }
  const npx = installCode.find((t) => t.text.startsWith('npx '));
  const tarball = installCode.find((t) => /^curl /m.test(t.text));
  if (installAt === -1 || !npx || !tarball)
    throw new Error(
      'README.md: Quickstart step 1 needs an npx code block and a curl code block'
    );

  // Captions: the first sentence of each row's "What it must show".
  const shots = {};
  for (const line of read('docs/images/README.md').split('\n')) {
    const m = line.match(/^\| `([a-z-]+)\.png` \| (.+?) \|/);
    if (m) shots[m[1]] = m[2].split(/(?<=\.) (?=[A-Z])/)[0];
  }
  const values = {
    pitchLead: escapeHtml(readmeTagline()),
    pitchProblem: inline(problem.slice(0, turn).trim()),
    pitchMethod: inline(problem.slice(turn)),
    whyTheName: inline(kitchenSentences.slice(0, 2).join(' ')),
    boardAlt: escapeHtml(alt),
    boardCaption: escapeHtml(alt),
    stages: stageHtml,
    installTarball: codeBlockHtml(tarball.text, tarball.lang),
    installNpx: codeBlockHtml(npx.text, npx.lang),
  };
  for (const name of [
    'card-rca',
    'card-triaged',
    'settings-integrations',
    'settings-agents',
    'quickstart-stand-up',
  ]) {
    if (!shots[name])
      throw new Error(`docs/images/README.md: no row for ${name}.png`);
    const html = inline(shots[name], 'docs/images/README.md');
    values[`caption:${name}`] = html;
    values[`alt:${name}`] = escapeHtml(stripTags(html));
  }
  return values;
}

function homeDescription() {
  const text = `${readmeTagline()} Open-source, Apache-2.0, Rust and TypeScript.`;
  if (text.length < DESCRIPTION_MIN || text.length > DESCRIPTION_MAX)
    problems.push(
      `the home page's description is ${text.length} characters; keep it within ${DESCRIPTION_MIN}–${DESCRIPTION_MAX}`
    );
  return text;
}

// The files each page is built from, for its lastmod in the sitemap.
function pageSources() {
  return [
    {
      out: 'index.html',
      sources: ['site/src/home.html', 'README.md', 'docs/images/README.md'],
    },
    ...PAGES.map((p) => ({ out: p.out, sources: [p.src] })),
    { out: 'tammy.html', sources: ['site/src/tammy.html'] },
  ];
}

// The date of the last commit that touched any of `sources`, or null when git
// has none (an uncommitted file, or no repository).
function lastCommitDate(sources) {
  try {
    const date = execFileSync(
      'git',
      ['log', '-1', '--format=%cI', '--', ...sources],
      { cwd: REPO, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }
    ).trim();
    return date || null;
  } catch {
    return null;
  }
}

// sitemap.xml: every page's canonical URL, with the date its sources last
// changed (the build time when git cannot say).
function writeSitemap() {
  const buildTime = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const urls = pageSources().map(({ out, sources }) => {
    const lastmod = lastCommitDate(sources) ?? buildTime;
    return (
      `  <url>\n    <loc>${escapeHtml(SITE_URL + sitePath(out))}</loc>\n` +
      `    <lastmod>${lastmod}</lastmod>\n  </url>\n`
    );
  });
  write(
    'sitemap.xml',
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
      '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
      urls.join('') +
      '</urlset>\n'
  );
}

// Every HTML page in dist, as a dist-relative path with / separators.
function distPages(dir = DIST) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return distPages(full);
    return entry.name.endsWith('.html')
      ? [path.relative(DIST, full).split(path.sep).join('/')]
      : [];
  });
}

async function build() {
  fs.rmSync(DIST, { recursive: true, force: true });
  fs.mkdirSync(DIST, { recursive: true });

  const docsOnDisk = listFiles('docs', (f) => f.endsWith('.md'));
  for (const doc of docsOnDisk) {
    if (!PAGE_BY_SRC.has(doc))
      problems.push(`${doc} is not in the sidebar (GROUPS in build.mjs)`);
  }

  for (const rel of COPIED) copy(path.join(REPO, rel), rel);
  await writeWebp();
  for (const f of fs.readdirSync(path.join(SITE, 'assets'))) {
    copy(path.join(SITE, 'assets', f), `assets/${f}`);
  }
  // The app's own tab and home-screen icons: the white mark on a blue tile
  // reads on light and dark tab strips alike.
  for (const f of ['favicon.svg', 'apple-touch-icon.png']) {
    copy(path.join(REPO, 'packages/public', f), f);
  }
  copy(path.join(SRC, 'site.css'), 'static/site.css');
  copy(path.join(SRC, 'site.js'), 'static/site.js');

  PAGES.forEach(docPage);

  const home = fill(
    fs.readFileSync(path.join(SRC, 'home.html'), 'utf8'),
    homeValues()
  );
  write(
    'index.html',
    page({
      out: 'index.html',
      title: HOME_TITLE,
      description: homeDescription(),
      main: home,
      section: 'home',
      jsonLd: homeStructuredData(),
    })
  );

  const tammy = fs.readFileSync(path.join(SRC, 'tammy.html'), 'utf8');
  write(
    'tammy.html',
    page({
      out: 'tammy.html',
      title: 'Tammy · Prepcook',
      description: description('site/src/tammy.html'),
      main: tammy,
      section: 'tammy',
      jsonLd: tammyStructuredData(),
    })
  );

  writeSitemap();
  write(
    'robots.txt',
    `User-agent: *\nAllow: /\n\nSitemap: ${SITE_URL}sitemap.xml\n`
  );

  for (const file of distPages()) {
    const count = (
      fs.readFileSync(path.join(DIST, file), 'utf8').match(/<h1[\s>]/g) || []
    ).length;
    if (count !== 1)
      problems.push(`${file}: ${count} <h1> elements; a page has one`);
  }

  if (problems.length) {
    for (const p of problems) console.error(`build: ${p}`);
    process.exit(1);
  }
  const failures = checkLinks(DIST);
  if (failures.length) {
    for (const f of failures) console.error(`link check: ${f}`);
    process.exit(1);
  }
  console.log(
    `Built ${distPages().length} pages into ${path.relative(REPO, DIST)}/`
  );
}

await build();
