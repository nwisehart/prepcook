// Checks the JSON-LD in the built site: it parses, and each entity carries the
// fields search engines read. Run after the build (`pnpm run build` runs it).

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const SITE = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(SITE, 'dist');
const { version } = JSON.parse(
  fs.readFileSync(path.join(SITE, '..', 'package.json'), 'utf8')
);

// Every JSON-LD block on a built page, parsed, with @graph entries flattened
// into one list of entities.
function entities(page) {
  const html = fs.readFileSync(path.join(DIST, page), 'utf8');
  const blocks = [
    ...html.matchAll(
      /<script type="application\/ld\+json">([\s\S]*?)<\/script>/g
    ),
  ].map((m) => JSON.parse(m[1]));
  assert.ok(blocks.length > 0, `${page} has no JSON-LD`);
  for (const block of blocks) {
    assert.equal(block['@context'], 'https://schema.org');
  }
  return blocks.flatMap((b) => b['@graph'] ?? [b]);
}

function ofType(list, type) {
  return list.filter((e) => [e['@type']].flat().includes(type));
}

function assertFields(entity, fields) {
  for (const field of fields) {
    const value = entity[field];
    assert.ok(
      value !== undefined && value !== '' && value !== null,
      `${entity.name ?? entity['@type']}: ${field} is missing`
    );
  }
}

test('the home page describes Prepcook and the site', () => {
  const list = entities('index.html');
  const [app, ...otherApps] = ofType(list, 'SoftwareApplication');
  assert.ok(app, 'no SoftwareApplication');
  assert.equal(otherApps.length, 0, 'more than one SoftwareApplication');
  assertFields(app, [
    'name',
    'description',
    'applicationCategory',
    'operatingSystem',
    'offers',
    'license',
    'url',
    'downloadUrl',
    'softwareVersion',
    'author',
    'codeRepository',
    'programmingLanguage',
    'screenshot',
  ]);
  assert.equal(app.name, 'Prepcook');
  assert.equal(app.applicationCategory, 'DeveloperApplication');
  assert.equal(app.operatingSystem, 'macOS, Linux');
  assert.deepEqual(app.offers, {
    '@type': 'Offer',
    price: '0',
    priceCurrency: 'USD',
  });
  assert.equal(app.license, 'https://www.apache.org/licenses/LICENSE-2.0');
  assert.equal(app.url, 'https://prepcook.dev/');
  assert.equal(
    app.downloadUrl,
    'https://github.com/nwisehart/prepcook/releases'
  );
  assert.equal(app.softwareVersion, version);
  assert.deepEqual(app.author, {
    '@type': 'Person',
    name: 'Nick Wisehart',
    url: 'https://github.com/nwisehart',
  });
  assert.equal(app.codeRepository, 'https://github.com/nwisehart/prepcook');
  assert.deepEqual(app.programmingLanguage, ['Rust', 'TypeScript']);
  assert.equal(app.screenshot, 'https://prepcook.dev/docs/images/board.png');
  assert.ok(fs.existsSync(path.join(DIST, 'docs/images/board.png')));

  const [site] = ofType(list, 'WebSite');
  assert.ok(site, 'no WebSite');
  assert.equal(site.name, 'Prepcook');
  assert.equal(site.url, 'https://prepcook.dev/');
});

test('the Tammy page describes Tammy', () => {
  const [tammy] = ofType(entities('tammy.html'), 'SoftwareApplication');
  assert.ok(tammy, 'no SoftwareApplication');
  assertFields(tammy, [
    'name',
    'description',
    'applicationCategory',
    'offers',
    'license',
    'url',
    'author',
    'codeRepository',
  ]);
  assert.equal(tammy.name, 'Tammy');
  assert.equal(tammy.url, 'https://prepcook.dev/tammy.html');
  assert.equal(tammy.codeRepository, 'https://github.com/nwisehart/tammy');
});

test('no other page carries JSON-LD', () => {
  for (const page of ['get-started.html', 'docs/method.html']) {
    const html = fs.readFileSync(path.join(DIST, page), 'utf8');
    assert.ok(!html.includes('application/ld+json'), page);
  }
});
