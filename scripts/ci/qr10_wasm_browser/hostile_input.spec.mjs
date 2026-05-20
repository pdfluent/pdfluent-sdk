// QR-10 WASM browser hostile-input — @playwright/test variant (for CI runners
// that use the Playwright test runner). The canonical, dependency-light harness
// that was actually EXECUTED for this milestone is `run_browser.mjs` (plain
// node + the Playwright library); both drive the same glue page
// (`test_page.html`) and assert identical behaviour.
//
// Run (CI): serve the wasm-pack `web` package dir (with test_page.html as
// index.html), then:
//   QR10_URL=http://localhost:PORT/ npx playwright test hostile_input.spec.mjs
import { test, expect } from '@playwright/test';

const CASES = {
  empty: [],
  garbage: [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42],
  truncated_header: Array.from(new TextEncoder().encode('%PDF-1.7\n1 0 obj')),
  bogus_xref: Array.from(new TextEncoder().encode('%PDF-1.7\nxref\nstartxref\n999999\n%%EOF')),
  nul_block: new Array(4096).fill(0),
};

test('QR-10 WASM hostile input: typed errors, no crash', async ({ page }) => {
  const pageErrors = [];
  page.on('pageerror', (e) => pageErrors.push(String(e)));
  page.on('crash', () => pageErrors.push('PAGE CRASH'));

  await page.goto(process.env.QR10_URL || 'http://localhost:8080/');
  await page.waitForFunction('window.__qr10_ready === true || window.__qr10_error !== null', { timeout: 30000 });
  expect(await page.evaluate(() => window.__qr10_error)).toBeNull();

  // Hostile inputs -> typed error (ok:false + string errorCode), never a crash.
  for (const [name, bytes] of Object.entries(CASES)) {
    const r = await page.evaluate((b) => window.openPdf(b), bytes);
    expect(r.ok, `${name} must not succeed`).toBe(false);
    expect(typeof r.errorCode, `${name} must surface a typed errorCode`).toBe('string');
  }

  // Repeated hostile opens must not poison the runtime.
  for (let i = 0; i < 50; i++) await page.evaluate((b) => window.openPdf(b), CASES.garbage);
  expect((await page.evaluate((b) => window.openPdf(b), CASES.garbage)).ok).toBe(false);

  // No crash / unhandled rejection occurred.
  expect(await page.evaluate(() => window.__qr10_error)).toBeNull();
  expect(pageErrors).toEqual([]);
});
