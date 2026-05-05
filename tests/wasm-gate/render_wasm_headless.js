#!/usr/bin/env node
'use strict';
/**
 * Render PDFs via WASM in headless Chromium and write page-1 PNGs.
 *
 * Uses Playwright to run the `--target web` wasm-pack bundle in a real
 * browser context so ES-module imports and fetch() work correctly.
 *
 * Output filename per PDF: `{pdf-basename}.png`
 *
 * Usage:
 *   node render_wasm_headless.js \
 *     --wasm-pkg /tmp/wasm-pkg \
 *     --corpus   corpus/wasm-sample-100 \
 *     --out      /tmp/wasm-renders \
 *     [--dpi     96]
 */

const { chromium } = require('playwright');
const http = require('http');
const fs   = require('fs');
const path = require('path');
const url  = require('url');

// ── CLI args ──────────────────────────────────────────────────────────────
function getArg(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : null;
}

const wasmPkgDir = getArg('--wasm-pkg');
const corpusDir  = getArg('--corpus');
const outDir     = getArg('--out');
const dpi        = parseFloat(getArg('--dpi') || '96');

if (!wasmPkgDir || !corpusDir || !outDir) {
  process.stderr.write(
    'Usage: render_wasm_headless.js --wasm-pkg <dir> --corpus <dir>' +
    ' --out <dir> [--dpi <n>]\n'
  );
  process.exit(1);
}

fs.mkdirSync(outDir, { recursive: true });

// PDF points are 1/72 inch; scale = dpi/72 converts to device pixels per point.
const scale = dpi / 72.0;

// ── HTTP server ────────────────────────────────────────────────────────────
// Serves:
//   /          → bootstrap HTML that initialises the WASM module
//   /wasm/*    → wasmPkgDir  (xfa_wasm.js + xfa_wasm_bg.wasm)
//   /pdf/*     → corpusDir   (PDF files)

const MIME_TYPES = {
  '.js':   'application/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.pdf':  'application/pdf',
  '.html': 'text/html; charset=utf-8',
};

const BOOTSTRAP_HTML = `<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body>
<canvas id="c"></canvas>
<script type="module">
  import init, { PdfDoc } from '/wasm/xfa_wasm.js';
  try {
    await init('/wasm/xfa_wasm_bg.wasm');
    window.__PdfDoc = PdfDoc;
  } catch (err) {
    window.__wasmError = String(err);
  }
  window.__wasmReady = true;
</script>
</body>
</html>`;

function startServer() {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const pathname = url.parse(req.url).pathname;

      // Bootstrap page
      if (pathname === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
        res.end(BOOTSTRAP_HTML);
        return;
      }

      // Resolve file path
      let filePath;
      if (pathname.startsWith('/wasm/')) {
        filePath = path.join(wasmPkgDir, pathname.slice('/wasm/'.length));
      } else if (pathname.startsWith('/pdf/')) {
        // URL-decode the filename segment
        filePath = path.join(
          corpusDir,
          decodeURIComponent(pathname.slice('/pdf/'.length))
        );
      } else {
        res.writeHead(404);
        res.end();
        return;
      }

      // Prevent directory traversal
      const resolvedBase  = path.resolve(pathname.startsWith('/wasm/') ? wasmPkgDir : corpusDir);
      const resolvedFile  = path.resolve(filePath);
      if (!resolvedFile.startsWith(resolvedBase + path.sep) &&
          resolvedFile !== resolvedBase) {
        res.writeHead(403);
        res.end();
        return;
      }

      fs.stat(resolvedFile, (err, stat) => {
        if (err || !stat.isFile()) {
          res.writeHead(404);
          res.end();
          return;
        }
        const ext         = path.extname(resolvedFile).toLowerCase();
        const contentType = MIME_TYPES[ext] || 'application/octet-stream';
        res.writeHead(200, {
          'Content-Type':                contentType,
          'Content-Length':              String(stat.size),
          // Allow SharedArrayBuffer in case the WASM module uses threads.
          'Cross-Origin-Opener-Policy':  'same-origin',
          'Cross-Origin-Embedder-Policy':'require-corp',
        });
        fs.createReadStream(resolvedFile).pipe(res);
      });
    });

    server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port }));
    server.on('error', reject);
  });
}

// ── Main ──────────────────────────────────────────────────────────────────
async function main() {
  const pdfFiles = fs.readdirSync(corpusDir)
    .filter(f => f.toLowerCase().endsWith('.pdf'))
    .sort();

  if (pdfFiles.length === 0) {
    process.stderr.write(`ERROR: no PDF files found in ${corpusDir}\n`);
    process.exit(1);
  }

  process.stdout.write(
    `Rendering ${pdfFiles.length} PDFs at ${dpi} DPI (scale=${scale.toFixed(4)})...\n`
  );

  const { server, port } = await startServer();
  const baseUrl = `http://127.0.0.1:${port}`;

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page    = await context.newPage();

  // Forward browser errors to stderr so CI logs capture them.
  page.on('console', msg => {
    if (msg.type() === 'error') {
      process.stderr.write(`[browser-console] ${msg.text()}\n`);
    }
  });
  page.on('pageerror', err => {
    process.stderr.write(`[browser-error] ${err.message}\n`);
  });

  // Load the bootstrap page and wait for WASM initialisation.
  await page.goto(baseUrl, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => window.__wasmReady === true, { timeout: 60_000 });

  const wasmError = await page.evaluate(() => window.__wasmError);
  if (wasmError) {
    process.stderr.write(`ERROR: WASM init failed: ${wasmError}\n`);
    await browser.close();
    server.close();
    process.exit(1);
  }

  let rendered = 0;
  let failed   = 0;

  for (const pdfFile of pdfFiles) {
    const pdfUrl  = `${baseUrl}/pdf/${encodeURIComponent(pdfFile)}`;
    const outName = path.basename(pdfFile, path.extname(pdfFile)) + '.png';
    const outPath = path.join(outDir, outName);

    try {
      const pngBase64 = await page.evaluate(
        async ({ pdfUrl, scale }) => {
          const PdfDoc = window.__PdfDoc;
          if (!PdfDoc) throw new Error('PdfDoc not available on window');

          // Fetch PDF bytes from the local server.
          const resp = await fetch(pdfUrl);
          if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${pdfUrl}`);
          const bytes = new Uint8Array(await resp.arrayBuffer());

          // Open and render page 0 (renderPage returns [w:4LE][h:4LE][RGBA...]).
          const doc = PdfDoc.open(bytes);
          let raw;
          try {
            raw = doc.renderPage(0, scale);
          } finally {
            doc.free();
          }

          const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
          const w    = view.getUint32(0, true);
          const h    = view.getUint32(4, true);
          const rgba = new Uint8ClampedArray(raw.buffer, raw.byteOffset + 8, w * h * 4);

          // Draw to off-screen canvas and export as PNG.
          const canvas  = document.getElementById('c');
          canvas.width  = w;
          canvas.height = h;
          canvas.getContext('2d').putImageData(new ImageData(rgba, w, h), 0, 0);

          // Strip the data-URL prefix.
          return canvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, '');
        },
        { pdfUrl, scale }
      );

      fs.writeFileSync(outPath, Buffer.from(pngBase64, 'base64'));
      rendered++;
      process.stdout.write(`  ${pdfFile} → ${outName}\n`);
    } catch (err) {
      failed++;
      process.stderr.write(`  WARN: ${pdfFile}: ${err.message}\n`);
    }
  }

  process.stdout.write(
    `\nWASM renders: ${rendered} ok, ${failed} failed (total ${pdfFiles.length})\n`
  );

  await browser.close();
  server.close();

  if (rendered === 0) {
    process.stderr.write('ERROR: no PDFs were rendered successfully\n');
    process.exit(1);
  }
}

main().catch(err => {
  process.stderr.write(`Fatal: ${err.stack || String(err)}\n`);
  process.exit(1);
});
