// Reproduction harness template — copy to .repro/<TICKET>/repro.mjs and fill in CASES.
//
// Contract (see SKILL.md): logs in once and reuses the session; waits for the page to
// settle before looking; prints exactly one JSON line per case on stdout; screenshots
// every case; exits non-zero when any case fails. Diagnostics go to stderr only.
// The app's address and the test login come from the environment Prep Cook sets when it
// stands the app up (PREPCOOK_APP_URL, PREPCOOK_TEST_USER_EMAIL, PREPCOOK_TEST_USER_PASSWORD) and
// are never written anywhere.

import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const TICKET = process.env.TICKET ?? path.basename(path.dirname(new URL(import.meta.url).pathname));
const OUT = path.join(process.cwd(), '.repro', TICKET);
const STATE = path.join(OUT, 'state.json');
const SETTLE_MS = 750;
const STEP_TIMEOUT_MS = 30_000;

// ---- cases -----------------------------------------------------------------
// Every case names what a customer would do, what they should see, and how to read
// what they actually see. Keep a control that must pass on the target branch.
const CASES = [
  {
    name: 'control',
    path: '/', // e.g. '/templates/4/create?input=value'
    expected: '', // the value a correct application shows
    // Return the observed value from the settled page. Choose selectors from the
    // rendered DOM you captured during reconnaissance, never from memory.
    observe: async (page) => (await page.locator('body').innerText()).slice(0, 40),
  },
];

// ---- plumbing --------------------------------------------------------------
const log = (...args) => console.error(`[${TICKET}]`, ...args);

function readEnvironment() {
  const missing = ['PREPCOOK_APP_URL', 'PREPCOOK_TEST_USER_EMAIL', 'PREPCOOK_TEST_USER_PASSWORD'].filter(
    (name) => !process.env[name],
  );
  if (missing.length > 0) throw new Error(`not set: ${missing.join(', ')}`);
  return {
    baseUrl: process.env.PREPCOOK_APP_URL.replace(/\/$/, ''),
    account: { email: process.env.PREPCOOK_TEST_USER_EMAIL, password: process.env.PREPCOOK_TEST_USER_PASSWORD },
  };
}

async function settle(page) {
  await page.waitForLoadState('networkidle', { timeout: STEP_TIMEOUT_MS });
  await page.waitForTimeout(SETTLE_MS);
}

async function login(context, baseUrl, account) {
  const page = await context.newPage();
  await page.goto(`${baseUrl}/login`, { waitUntil: 'networkidle', timeout: STEP_TIMEOUT_MS });
  // Selectors below come from reconnaissance of the login page; adjust if it changes.
  await page.getByLabel(/email/i).fill(account.email);
  await page.getByLabel(/password/i).fill(account.password);
  await page.getByRole('button', { name: /log in|sign in/i }).click();
  await settle(page);
  if (/\/login/.test(page.url())) throw new Error('login did not leave the login page');
  await context.storageState({ path: STATE });
  fs.chmodSync(STATE, 0o600);
  await page.close();
}

async function main() {
  fs.mkdirSync(OUT, { recursive: true });
  const { baseUrl, account } = readEnvironment();
  const browser = await chromium.launch({
    // Playwright's bundled Chromium unless the machine names its own.
    executablePath: process.env.PREPCOOK_CHROMIUM_PATH || undefined,
    args: ['--no-sandbox'],
  });
  // Certificates are verified like a browser would. A certificate error is a finding
  // to post on the card, not a flag to set here.
  const contextOptions = { viewport: { width: 1440, height: 900 } };
  let context;
  if (fs.existsSync(STATE)) {
    context = await browser.newContext({ ...contextOptions, storageState: STATE });
  } else {
    context = await browser.newContext(contextOptions);
    await login(context, baseUrl, account);
  }

  let failed = 0;
  for (const c of CASES) {
    const page = await context.newPage();
    let observed;
    try {
      await page.goto(`${baseUrl}${c.path}`, { waitUntil: 'networkidle', timeout: STEP_TIMEOUT_MS });
      if (/\/login/.test(page.url())) {
        await login(context, baseUrl, account);
        await page.goto(`${baseUrl}${c.path}`, { waitUntil: 'networkidle', timeout: STEP_TIMEOUT_MS });
      }
      await settle(page);
      observed = await c.observe(page);
    } catch (error) {
      observed = `ERROR: ${error.message}`;
    }
    await page.screenshot({ path: path.join(OUT, `case-${c.name}.png`), fullPage: false });
    const pass = observed === c.expected;
    if (!pass) failed += 1;
    process.stdout.write(`${JSON.stringify({ case: c.name, expected: c.expected, observed, pass })}\n`);
    await page.close();
  }

  await browser.close();
  log(`${CASES.length - failed}/${CASES.length} passed`);
  process.exit(failed === 0 ? 0 : 1);
}

main().catch((error) => {
  log('harness crashed:', error.message);
  process.exit(2);
});
