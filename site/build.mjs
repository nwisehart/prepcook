// Generates the Prepcook website into site/dist from the repository's own
// documentation (README.md, CONTRIBUTING.md, docs/*.md) plus two hand-written
// pages (src/home.html, src/tammy.html). Every link and asset reference it
// writes is relative, because the site is served under a sub-path
// (https://nwisehart.github.io/prepcook/). The link check in check-links.mjs
// runs at the end and fails the build on any broken reference.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked } from 'marked';
import { checkLinks } from './check-links.mjs';

const SITE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(SITE, '..');
const SRC = path.join(SITE, 'src');
const DIST = path.join(SITE, 'dist');
const GITHUB = 'https://github.com/nwisehart/prepcook';

// The sidebar, in reading order. `src` is the markdown file in the repository,
// `out` the page it becomes in dist, `label` its sidebar entry. Every
// docs/*.md must appear here: the build fails on one that does not.
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

// Files copied into dist at the same relative path they have in the
// repository, so a reference that resolves in the repository resolves on the
// site too.
const COPIED = [
  ...listFiles('docs/images', (f) => f.endsWith('.png')),
  ...listFiles('assets/brand', (f) => f.endsWith('.svg')),
];
const COPIED_SET = new Set(COPIED);

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
// the repository links to the file on GitHub.
function rewriteUrl(url, srcFile, outFile) {
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
        if (depth === 1 && title === null) title = text;
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

function page({ out, title, description, main, section, docs = false }) {
  const root = rootPrefix(out);
  const html = fill(LAYOUT, {
    title: escapeHtml(title),
    description: escapeHtml(description),
    root,
    bodyClass: docs ? 'has-sidebar' : 'plain',
    menuButton: docs
      ? `<button class="icon-button menu-button" type="button" aria-label="Open the docs menu" aria-expanded="false" aria-controls="sidebar"><svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true"><path d="M3 5.5h14M3 10h14M3 14.5h14" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg></button>`
      : '',
    main,
  });
  return html.replace(
    `data-section="${section}"`,
    `data-section="${section}" aria-current="page"`
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

function docPage(p, index) {
  const { html, title, toc } = renderMarkdown(read(p.src), p.src, p.out);
  const pageTitle = p.src === 'README.md' ? 'Get started' : title;
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
      description: firstParagraph(p.src),
      main,
      section: 'docs',
      docs: true,
    })
  );
}

// The first prose paragraph of a markdown file, as plain text, for the
// page's meta description.
function firstParagraph(src) {
  const tokens = new Marked({ gfm: true }).lexer(read(src));
  const para = tokens.find(
    (t) => t.type === 'paragraph' && !t.text.startsWith('<')
  );
  return para
    ? stripTags(new Marked().parseInline(para.text)).replace(/\s+/g, ' ').trim()
    : '';
}

// Pieces of README.md and docs/images/README.md the home page shows, read at
// build time so the home page cannot drift from them.
function homeValues() {
  const readme = read('README.md');
  const tokens = new Marked({ gfm: true }).lexer(readme);
  const inline = (md, src = 'README.md') =>
    rewriteHtml(new Marked({ gfm: true }).parseInline(md), src, 'index.html');

  const pitch = tokens.find(
    (t) => t.type === 'paragraph' && !t.text.startsWith('<')
  );
  const sentences = pitch.text.replace(/\s+/g, ' ').split(/(?<=\.) (?=[A-Z])/);

  const boardAlt = readme.match(
    /<img src="docs\/images\/board\.png" alt="([^"]+)"/
  );
  if (!boardAlt) throw new Error('README.md: the board screenshot is missing');

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
    pitchLead: inline(sentences[0]),
    pitchRest: inline(sentences.slice(1).join(' ')),
    boardAlt: escapeHtml(boardAlt[1]),
    boardCaption: escapeHtml(boardAlt[1]),
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

function build() {
  fs.rmSync(DIST, { recursive: true, force: true });
  fs.mkdirSync(DIST, { recursive: true });

  const docsOnDisk = listFiles('docs', (f) => f.endsWith('.md'));
  for (const doc of docsOnDisk) {
    if (!PAGE_BY_SRC.has(doc))
      problems.push(`${doc} is not in the sidebar (GROUPS in build.mjs)`);
  }

  for (const rel of COPIED) copy(path.join(REPO, rel), rel);
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
      title: 'Prepcook',
      description: firstParagraph('README.md'),
      main: home,
      section: 'home',
    })
  );

  const tammy = fs.readFileSync(path.join(SRC, 'tammy.html'), 'utf8');
  write(
    'tammy.html',
    page({
      out: 'tammy.html',
      title: 'Tammy · Prepcook',
      description:
        'Tammy is the Slack agent in front of Prepcook: she answers customers from your docs and takes problem reports to the kitchen.',
      main: tammy,
      section: 'tammy',
    })
  );

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
    `Built ${PAGES.length + 2} pages into ${path.relative(REPO, DIST)}/`
  );
}

build();
