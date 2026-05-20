// QR-10 release gate — WASM browser hostile-input harness (Playwright).
// Runs in a real headless browser against the built WASM package. Requires:
//   npm i -D @playwright/test && npx playwright install chromium
//   a served page that imports the built `pdfluent` wasm and exposes
//   window.openPdf(bytes) -> {ok, errorCode}.
// This file is the executable gate definition; CI runs `npx playwright test`.
import { test, expect } from '@playwright/test';

const cases = {
  empty: new Uint8Array([]),
  garbage: new Uint8Array([0xDE, 0xAD, 0xBE, 0xEF]),
  truncated: new TextEncoder().encode('%PDF-1.7\n1 0 obj'),
};

test.beforeEach(async ({ page }) => {
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('crash', () => errors.push('PAGE CRASH'));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  page.__errors = errors;
  await page.goto(process.env.QR10_URL || 'http://localhost:8080/');
});

for (const [name, bytes] of Object.entries(cases)) {
  test(`hostile ${name}: typed error, no crash, no unhandled rejection`, async ({ page }) => {
    const res = await page.evaluate((b) => window.openPdf(new Uint8Array(b)), Array.from(bytes));
    expect(res.ok).toBe(false);            // hostile input must not "succeed"
    expect(typeof res.errorCode).toBe('string'); // typed error surfaced to JS
    expect(page.__errors).toEqual([]);     // no crash / unhandled rejection
  });
}

test('repeated opens do not leak (heap stays bounded)', async ({ page }) => {
  const grow = await page.evaluate(async () => {
    const before = performance.memory?.usedJSHeapSize ?? 0;
    for (let i = 0; i < 200; i++) window.openPdf(new Uint8Array([0xDE,0xAD]));
    const after = performance.memory?.usedJSHeapSize ?? 0;
    return after - before;
  });
  // Allow churn but flag gross growth (>64MB) across 200 hostile opens.
  expect(grow).toBeLessThan(64 * 1024 * 1024);
});
