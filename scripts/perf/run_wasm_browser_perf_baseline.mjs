// run_wasm_browser_perf_baseline.mjs — non-XFA WASM browser perf baseline.
//
// Serves the wasm-pack `web` package dir over http, loads the perf glue page
// in real Chromium (Playwright), and measures:
//   - WASM init time (module instantiation)
//   - valid PDF open+pageCount loop (p50/p95/p99 ns, in-browser timed)
//   - hostile/invalid open loop timing (must fail fast, no hang)
//   - repeated-open JS heap signal (Chromium performance.memory)
//   - no page errors / no unhandled rejection
//
// Usage: node run_wasm_browser_perf_baseline.mjs <pkgDir> <validPdf> <outJson> [iters]
// Exit:  0 ok, 1 failure (init/page error/poisoned), 2 SKIP (pkg/playwright missing).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const pkgDir = process.argv[2];
const validPdf = process.argv[3];
const outJson = process.argv[4];
const ITERS = parseInt(process.argv[5] || '300', 10);

if (!pkgDir || !fs.existsSync(path.join(pkgDir, 'xfa_wasm.js'))) {
  console.error('SKIP: wasm package not found at', pkgDir);
  process.exit(2);
}
const require = createRequire(import.meta.url);
let chromium;
try { ({ chromium } = require('playwright')); }
catch { console.error('SKIP: playwright library unresolved'); process.exit(2); }

const MIME = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html', '.json': 'application/json' };

function serve(dir) {
  const server = http.createServer((req, res) => {
    const rel = req.url === '/' ? '/index.html' : req.url.split('?')[0];
    const fp = path.join(dir, rel);
    if (!fp.startsWith(dir) || !fs.existsSync(fp)) { res.statusCode = 404; res.end('nf'); return; }
    res.setHeader('Content-Type', MIME[path.extname(fp)] || 'application/octet-stream');
    fs.createReadStream(fp).pipe(res);
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

(async () => {
  const server = await serve(pkgDir);
  const port = server.address().port;
  const url = `http://127.0.0.1:${port}/index.html`;
  const validBytes = Array.from(fs.readFileSync(validPdf));
  const garbage = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42];

  const summary = { kind: 'wasm_browser', browser: 'chromium', url, iters: ITERS, ok: false };
  const browser = await chromium.launch({ headless: true });
  const pageErrors = [], consoleErrors = [];
  try {
    const page = await browser.newPage();
    page.on('pageerror', (e) => pageErrors.push(String(e)));
    page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); });
    await page.goto(url, { waitUntil: 'load' });
    await page.waitForFunction(() => window.__perf_ready === true || window.__perf_error, null, { timeout: 30000 });

    const initErr = await page.evaluate(() => window.__perf_error);
    if (initErr) throw new Error(initErr);

    summary.init_ms = await page.evaluate(() => window.__init_ms);
    summary.valid = await page.evaluate(([b, n]) => window.perfLoop(b, n, true), [validBytes, ITERS]);
    summary.hostile = await page.evaluate(([b, n]) => window.perfLoop(b, n, false), [garbage, ITERS]);

    // Runtime must remain healthy after the hostile loop.
    const after = await page.evaluate((b) => window.perfLoop(b, 1, true), validBytes);
    summary.runtime_healthy_after_hostile = after.oks === 1;

    summary.pageErrors = pageErrors;
    summary.consoleErrors = consoleErrors;
    const healthy = summary.valid.oks === ITERS && summary.hostile.errs === ITERS
      && summary.runtime_healthy_after_hostile && pageErrors.length === 0;
    summary.ok = healthy;
  } catch (e) {
    summary.error = String(e && e.message ? e.message : e);
    summary.pageErrors = pageErrors;
  } finally {
    await browser.close();
    server.close();
  }
  fs.mkdirSync(path.dirname(outJson), { recursive: true });
  fs.writeFileSync(outJson, JSON.stringify(summary, null, 2));
  const v = summary.valid || {};
  console.log(`wasm perf -> ${outJson}`);
  console.log(`  init=${summary.init_ms?.toFixed?.(1)}ms  valid p50=${v.p50_ns?.toFixed?.(0)}ns p95=${v.p95_ns?.toFixed?.(0)}ns oks=${v.oks}/${ITERS}  ok=${summary.ok}`);
  process.exit(summary.ok ? 0 : 1);
})();
