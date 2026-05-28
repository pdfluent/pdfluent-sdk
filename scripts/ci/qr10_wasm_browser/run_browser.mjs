// QR-10 WASM browser hostile-input harness (plain node + Playwright library).
// Serves the wasm-pack `web` package dir over http, loads the glue page in
// real headless Chromium, runs hostile-input cases, captures console errors /
// pageerror / unhandled rejections, asserts typed-error-not-crash, and writes
// a JSON summary. Exits non-zero on any crash / unhandled rejection / silent
// success / control failure.
//
// Usage: node run_browser.mjs <pkgDir> <jsonOut>
//   pkgDir : directory containing xfa_wasm.js + xfa_wasm_bg.wasm + index.html
//   jsonOut: path to write the machine-readable summary
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const pkgDir = process.argv[2];
const jsonOut = process.argv[3] || '/tmp/qr10_result.json';
if (!pkgDir || !fs.existsSync(path.join(pkgDir, 'xfa_wasm.js'))) {
  console.error('SKIP: wasm package not found at', pkgDir);
  process.exit(2);
}

// Resolve the Playwright library from wherever it is installed (npx cache,
// global, or local node_modules) via NODE_PATH-style lookup.
const require = createRequire(import.meta.url);
let chromium;
try {
  ({ chromium } = require('playwright'));
} catch {
  console.error('SKIP: playwright library not resolvable (set NODE_PATH to its node_modules)');
  process.exit(2);
}

const MIME = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html', '.json': 'application/json' };

function serve(dir) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const rel = decodeURIComponent(req.url.split('?')[0]);
      const file = path.join(dir, rel === '/' ? 'index.html' : rel);
      if (!file.startsWith(dir) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
        res.writeHead(404); res.end('nf'); return;
      }
      res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
      fs.createReadStream(file).pipe(res);
    });
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

const CASES = {
  empty: [],
  garbage: [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42],
  truncated_header: Array.from(Buffer.from('%PDF-1.7\n1 0 obj')),
  bogus_xref: Array.from(Buffer.from('%PDF-1.7\nxref\nstartxref\n999999\n%%EOF')),
  nul_block: new Array(4096).fill(0),
};

(async () => {
  const server = await serve(pkgDir);
  const port = server.address().port;
  const url = `http://127.0.0.1:${port}/`;
  const consoleErrors = [];
  const pageErrors = [];
  const browser = await chromium.launch({ headless: true });
  const summary = { url, browser: 'chromium', cases: {}, pageErrors, consoleErrors, ok: false };
  try {
    const page = await browser.newPage();
    page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); });
    page.on('pageerror', (e) => pageErrors.push(String(e)));
    page.on('crash', () => pageErrors.push('PAGE CRASH'));
    await page.goto(url, { waitUntil: 'load' });
    await page.waitForFunction('window.__qr10_ready === true || window.__qr10_error !== null', { timeout: 30000 });
    const initErr = await page.evaluate(() => window.__qr10_error);
    if (initErr) throw new Error('init/runtime error: ' + initErr);

    let fails = [];
    // 0. VALID control -> must succeed (ok:true, pages>=1). Proves the harness
    //    discriminates valid from hostile, i.e. it is not always-false.
    const validPath = process.env.QR10_VALID_PDF || 'tests/corpus-mini/multi-page.pdf';
    if (fs.existsSync(validPath)) {
      const validBytes = Array.from(fs.readFileSync(validPath));
      const cr = await page.evaluate((b) => window.openPdf(b), validBytes);
      summary.cases['valid_control'] = cr;
      if (cr.ok !== true || !(cr.pages >= 1)) fails.push(`valid_control: expected ok:true pages>=1, got ${JSON.stringify(cr)}`);
    } else {
      summary.cases['valid_control'] = { skipped: `fixture missing: ${validPath}` };
    }

    // 1. hostile inputs -> typed error (ok:false + errorCode string), not crash.
    for (const [name, bytes] of Object.entries(CASES)) {
      const r = await page.evaluate((b) => window.openPdf(b), bytes);
      summary.cases[name] = r;
      if (r.ok !== false) fails.push(`${name}: expected ok:false, got ${JSON.stringify(r)}`);
      else if (typeof r.errorCode !== 'string' || r.errorCode.length === 0) fails.push(`${name}: missing typed errorCode`);
    }
    // 2. repeated hostile opens must not poison the runtime.
    for (let i = 0; i < 50; i++) await page.evaluate((b) => window.openPdf(b), CASES.garbage);
    const afterRepeat = await page.evaluate((b) => window.openPdf(b), CASES.garbage);
    summary.cases['after_50_repeats'] = afterRepeat;
    if (afterRepeat.ok !== false) fails.push('runtime poisoned after repeated hostile opens');

    // 3. unhandled rejection / pageerror check.
    const lateErr = await page.evaluate(() => window.__qr10_error);
    if (lateErr) fails.push('unhandled rejection: ' + lateErr);
    if (pageErrors.length) fails.push('pageerror/crash: ' + pageErrors.join('; '));

    summary.ok = fails.length === 0;
    summary.failures = fails;
    fs.writeFileSync(jsonOut, JSON.stringify(summary, null, 2));
    if (!summary.ok) { console.error('QR-10 FAIL:', fails.join(' | ')); process.exitCode = 1; }
    else console.log('QR-10 browser hostile-input: OK —', Object.keys(summary.cases).length, 'cases, 0 crash, typed errors');
  } finally {
    await browser.close();
    server.close();
  }
})().catch((e) => { console.error('QR-10 harness error:', e.message); process.exit(1); });
