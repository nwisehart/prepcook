#!/usr/bin/env node
// A webhook target for testing Prepcook's notifications by hand
// (docs/manual-testing.md). Prints each delivery with its event, delivery id
// and whether X-PrepCook-Signature matches WEBHOOK_SECRET, then answers 200.
// --fail answers 500 instead, so the retries (after 0.5, 1 and 2 seconds)
// arrive as the same X-PrepCook-Delivery four times.
//
//   WEBHOOK_SECRET=<the target's secret> node scripts/webhook-echo.mjs [--port 4999] [--fail]
import { createHmac, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import { parseArgs } from 'node:util';

const { values: flags } = parseArgs({
  options: {
    port: { type: 'string', default: '4999' },
    fail: { type: 'boolean', default: false },
  },
});
const secret = process.env.WEBHOOK_SECRET ?? '';

// Computed over the raw bytes, as a real receiver must: re-serialised JSON
// would not match.
function signatureVerdict(body, header) {
  if (!header)
    return secret ? 'UNSIGNED, but WEBHOOK_SECRET is set' : 'unsigned';
  if (!secret) return 'signed; set WEBHOOK_SECRET to check it';
  const expected = Buffer.from(
    'sha256=' + createHmac('sha256', secret).update(body).digest('hex')
  );
  const received = Buffer.from(header);
  return expected.length === received.length &&
    timingSafeEqual(expected, received)
    ? 'signature ok'
    : 'SIGNATURE MISMATCH';
}

createServer((req, res) => {
  const chunks = [];
  req.on('data', (chunk) => chunks.push(chunk));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    const event = req.headers['x-prepcook-event'] ?? '-';
    const delivery = req.headers['x-prepcook-delivery'] ?? '-';
    const verdict = signatureVerdict(body, req.headers['x-prepcook-signature']);
    console.log(`\n${new Date().toISOString()} ${req.method} ${req.url}`);
    console.log(`event=${event} delivery=${delivery} ${verdict}`);
    try {
      console.log(JSON.stringify(JSON.parse(body.toString('utf8')), null, 2));
    } catch {
      console.log(body.toString('utf8'));
    }
    const status = flags.fail ? 500 : 200;
    res.writeHead(status, { 'content-type': 'text/plain' });
    res.end(status === 200 ? 'ok\n' : 'failing on purpose (--fail)\n');
  });
}).listen(Number(flags.port), '127.0.0.1', () => {
  const answer = flags.fail ? 'answering 500' : 'answering 200';
  const check = secret ? 'checking signatures' : 'WEBHOOK_SECRET unset';
  console.log(
    `webhook-echo on http://127.0.0.1:${flags.port}/ (${answer}; ${check})`
  );
});
