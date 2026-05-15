#!/usr/bin/env node
// WASM performance bench harness for @pdfluent/sdk-wasm.
//
// Usage:
//   node scripts/bench/wasm-bench.mjs [--iters N] [--warmup K]
//                                     [--ops <list>] [--out <path>]
//                                     [--fixtures <path>]
//                                     [--quiet]
//
// Fixtures are read from a JSON manifest (default
// benchmarks/fixtures/wasm_perf/manifest.json). Each entry has a
// `path` (file:// or absolute), a `tag`, a category, and optional
// `private: true` flag. Private fixtures are reported with size +
// page count but not committed.
//
// The harness loads the local pkg build at crates/xfa-wasm/pkg/.

import { performance } from 'node:perf_hooks';
import { readFile } from 'node:fs/promises';
import { argv, exit, memoryUsage } from 'node:process';
import { resolve, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync } from 'node:fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '..', '..');
const PKG_DIR = resolve(REPO_ROOT, 'crates', 'xfa-wasm', 'pkg');
const DEFAULT_FIXTURES = resolve(
  REPO_ROOT,
  'benchmarks',
  'fixtures',
  'wasm_perf',
  'manifest.json',
);

// CLI parse.
function parseArgs() {
  const a = argv.slice(2);
  const opts = {
    iters: 10,
    warmup: 2,
    ops: null,
    out: null,
    fixtures: DEFAULT_FIXTURES,
    quiet: false,
  };
  for (let i = 0; i < a.length; i++) {
    const k = a[i];
    if (k === '--iters') opts.iters = parseInt(a[++i], 10);
    else if (k === '--warmup') opts.warmup = parseInt(a[++i], 10);
    else if (k === '--ops') opts.ops = a[++i].split(',');
    else if (k === '--out') opts.out = a[++i];
    else if (k === '--fixtures') opts.fixtures = resolve(a[++i]);
    else if (k === '--quiet') opts.quiet = true;
    else if (k === '-h' || k === '--help') { usage(); exit(0); }
    else { console.error(`Unknown flag: ${k}`); usage(); exit(2); }
  }
  return opts;
}

function usage() {
  console.log(`wasm-bench — repeatable WASM perf harness

  --iters   N    iterations per op (default 10)
  --warmup  K    warmup iterations discarded (default 2)
  --ops     csv  ops to run (default: all). E.g.
                 open,pageCount,metadata,render,xfa_flatten,
                 pdfa_validate,pdfa_convert,
                 edits_stateless,edits_stateful,save
  --fixtures path manifest.json with fixtures (default benchmarks/fixtures/wasm_perf/manifest.json)
  --out     path optional JSON output path
  --quiet        suppress per-iteration logs`);
}

// Stats.
function summarize(samples) {
  if (samples.length === 0) return null;
  const sorted = [...samples].sort((x, y) => x - y);
  const n = sorted.length;
  const median = sorted[Math.floor(n / 2)];
  const p90 = sorted[Math.min(n - 1, Math.floor(n * 0.9))];
  const min = sorted[0];
  const max = sorted[n - 1];
  const mean = sorted.reduce((a, b) => a + b, 0) / n;
  return { n, median_ms: median, p90_ms: p90, min_ms: min, max_ms: max, mean_ms: mean };
}

function memNow() {
  const m = memoryUsage();
  return { rss: m.rss, heapUsed: m.heapUsed, external: m.external };
}

// Run an op with warmup + iters, return summary + memory delta.
async function bench(name, fn, { iters, warmup, quiet, fixture }) {
  for (let i = 0; i < warmup; i++) {
    try { await fn(); } catch (e) {
      return { name, fixture, error: `warmup failed: ${e.message || e}` };
    }
  }
  const memBefore = memNow();
  const samples = [];
  let lastErr = null;
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    try {
      await fn();
    } catch (e) {
      lastErr = e;
      break;
    }
    const t1 = performance.now();
    samples.push(t1 - t0);
    if (!quiet) process.stdout.write('.');
  }
  if (!quiet) process.stdout.write('\n');
  if (global.gc) global.gc();
  const memAfter = memNow();
  const out = { name, fixture, ...summarize(samples) };
  out.mem_delta_rss = memAfter.rss - memBefore.rss;
  out.mem_delta_heap = memAfter.heapUsed - memBefore.heapUsed;
  if (lastErr) out.error = lastErr.message || String(lastErr);
  return out;
}

async function loadModule() {
  // Use a per-call fresh import for cold-init measurement.
  const wasmPath = resolve(PKG_DIR, 'xfa_wasm_bg.wasm');
  const jsPath = resolve(PKG_DIR, 'xfa_wasm.js');
  if (!existsSync(wasmPath) || !existsSync(jsPath)) {
    throw new Error(`pkg missing — expected ${PKG_DIR}/xfa_wasm.js`);
  }
  const wasmBytes = await readFile(wasmPath);
  const mod = await import(`file://${jsPath}?bust=${Date.now()}_${Math.random()}`);
  await mod.default({ module_or_path: wasmBytes });
  return mod;
}

// Cold init: measure repeatedly with fresh import per iter.
async function benchColdInit({ iters, warmup }) {
  const wasmPath = resolve(PKG_DIR, 'xfa_wasm_bg.wasm');
  const jsPath = resolve(PKG_DIR, 'xfa_wasm.js');
  const wasmBytes = await readFile(wasmPath);
  const run = async () => {
    const mod = await import(`file://${jsPath}?bust=${Date.now()}_${Math.random()}`);
    await mod.default({ module_or_path: wasmBytes });
  };
  for (let i = 0; i < warmup; i++) await run();
  const samples = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    await run();
    samples.push(performance.now() - t0);
  }
  return { name: 'wasm_cold_init', ...summarize(samples) };
}

function expandEnv(s) {
  return s.replace(/\$\{([A-Z0-9_]+)\}/g, (_, name) => process.env[name] ?? '');
}

async function loadFixtures(path) {
  if (!existsSync(path)) return [];
  const raw = JSON.parse(await readFile(path, 'utf-8'));
  const out = [];
  for (const e of raw.fixtures || []) {
    const expanded = expandEnv(e.path);
    if (expanded.includes('${') || expanded === '') {
      console.error(`  [skip] unresolved env var in path: ${e.tag} → ${e.path}`);
      continue;
    }
    const p = expanded.startsWith('/') ? expanded : resolve(dirname(path), expanded);
    if (!existsSync(p)) {
      console.error(`  [skip] missing fixture: ${e.tag} → ${p}`);
      continue;
    }
    const bytes = await readFile(p);
    out.push({ ...e, bytes, size: bytes.byteLength, name: basename(p) });
  }
  return out;
}

async function main() {
  const opts = parseArgs();
  const results = {
    runner: 'wasm-bench.mjs',
    package: '@pdfluent/sdk-wasm',
    iterations: opts.iters,
    warmup: opts.warmup,
    runtime: `node ${process.version}`,
    platform: `${process.platform}/${process.arch}`,
    timestamp_utc: new Date().toISOString(),
    cold_init: null,
    fixtures: [],
    ops: [],
  };

  console.log('== wasm-bench ==');
  console.log(`pkg:       ${PKG_DIR}`);
  console.log(`iters/op:  ${opts.iters} (+${opts.warmup} warmup)`);

  // 1) Cold init.
  console.log('\n[cold init]');
  results.cold_init = await benchColdInit(opts);
  console.log(`  median=${results.cold_init.median_ms.toFixed(2)}ms  p90=${results.cold_init.p90_ms.toFixed(2)}ms`);

  // 2) Warm module for everything else.
  const wasm = await loadModule();
  const PdfDoc = wasm.PdfDoc;
  const PdfDocMut = wasm.PdfDocMut;

  // 3) Fixtures.
  const fixtures = await loadFixtures(opts.fixtures);
  if (fixtures.length === 0) {
    console.log('\n[warn] no fixtures found — exiting (set --fixtures or create manifest)');
    if (opts.out) await persist(results, opts.out);
    return;
  }
  results.fixtures = fixtures.map((f) => ({
    tag: f.tag,
    category: f.category,
    name: f.private ? '<private>' : f.name,
    bytes: f.size,
    pageCount: null,
    private: !!f.private,
  }));

  // Run ops per fixture.
  for (let fi = 0; fi < fixtures.length; fi++) {
    const f = fixtures[fi];
    console.log(`\n[fixture ${fi + 1}/${fixtures.length}] ${f.tag}  (${(f.size / 1024).toFixed(0)} KiB)`);

    // open + pageCount + metadata.
    const want = (name) => !opts.ops || opts.ops.includes(name);

    let docForReads = null;
    try {
      docForReads = PdfDoc.open ? PdfDoc.open(f.bytes) : new PdfDoc(f.bytes);
    } catch (e) {
      // Try ctor variant.
      try { docForReads = new PdfDoc(f.bytes); }
      catch (e2) { console.error(`  open failed: ${e2.message}`); continue; }
    }
    try {
      const pc = docForReads.pageCount();
      results.fixtures[fi].pageCount = pc;
      console.log(`  pageCount=${pc}`);
    } catch {}
    try { docForReads.free(); } catch {}

    const ops = [];

    if (want('open')) ops.push(['open', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.free();
    }]);
    if (want('pageCount')) ops.push(['pageCount', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.pageCount(); d.free();
    }]);
    if (want('metadata')) ops.push(['metadata', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.metadata(); d.free();
    }]);
    if (want('text_page0')) ops.push(['text_page0', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.text(0); d.free();
    }]);
    if (want('render_thumb_p0')) ops.push(['render_thumb_p0', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.renderThumbnail(0, 256); d.free();
    }]);
    if (want('render_page_p0_x1') && f.size < 10 * 1024 * 1024) ops.push(['render_page_p0_x1', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.renderPage(0, 1.0); d.free();
    }]);
    if (want('xfa_flatten') && f.category === 'xfa') ops.push(['xfa_flatten', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.flattenXfa(); d.free();
    }]);
    if (want('pdfa_validate')) ops.push(['pdfa_validate', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.validatePdfA('2b'); d.free();
    }]);
    if (want('pdfa_convert') && f.size < 5 * 1024 * 1024) ops.push(['pdfa_convert', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.convertToPdfa('2b'); d.free();
    }]);
    if (want('compress')) ops.push(['compress', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.compress(); d.free();
    }]);
    if (want('add_watermark_stateless')) ops.push(['add_watermark_stateless', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.addTextWatermark('CONFIDENTIAL', 0.3); d.free();
    }]);
    if (want('rotate_p0_stateless')) ops.push(['rotate_p0_stateless', () => {
      const d = openPdfDoc(PdfDoc, f.bytes); d.rotatePage(0, 90); d.free();
    }]);

    // PdfDocMut session.
    if (PdfDocMut && want('mut_open')) ops.push(['mut_open', () => {
      const m = PdfDocMut.open(f.bytes); m.free();
    }]);
    if (PdfDocMut && want('mut_1edit_save')) ops.push(['mut_1edit_save', () => {
      const m = PdfDocMut.open(f.bytes); m.rotatePage(0, 90); m.save(); m.free();
    }]);
    if (PdfDocMut && want('mut_4edit_save')) ops.push(['mut_4edit_save', () => {
      const m = PdfDocMut.open(f.bytes);
      m.rotatePage(0, 90);
      m.addTextWatermark('DRAFT', 0.2);
      try { m.compress(); } catch {}
      try { m.addStickyNote(0, 50, 50, 'note'); } catch {}
      m.save(); m.free();
    }]);
    if (PdfDocMut && want('mut_10edit_save')) ops.push(['mut_10edit_save', () => {
      const m = PdfDocMut.open(f.bytes);
      for (let i = 0; i < 10; i++) {
        try { m.rotatePage(0, 90); } catch { break; }
      }
      m.save(); m.free();
    }]);

    // Stateless equivalents to compare against PdfDocMut.
    // Stateless chains are O(N×fullParse). Skip them on >1 MiB files
    // because the speedup vs PdfDocMut is already demonstrated; the
    // absolute numbers blow out CI time without adding signal.
    const allowSlowChain = f.size < 1024 * 1024;
    if (allowSlowChain && want('stateless_4edit_chain')) ops.push(['stateless_4edit_chain', () => {
      let bytes = f.bytes;
      let d = openPdfDoc(PdfDoc, bytes); bytes = d.rotatePage(0, 90); d.free();
      d = openPdfDoc(PdfDoc, bytes); bytes = d.addTextWatermark('DRAFT', 0.2); d.free();
      try { d = openPdfDoc(PdfDoc, bytes); bytes = d.compress(); d.free(); } catch {}
      try { d = openPdfDoc(PdfDoc, bytes); bytes = d.addStickyNote(0, 50, 50, 'note'); d.free(); } catch {}
    }]);
    if (allowSlowChain && want('stateless_10edit_chain')) ops.push(['stateless_10edit_chain', () => {
      let bytes = f.bytes;
      for (let i = 0; i < 10; i++) {
        try {
          const d = openPdfDoc(PdfDoc, bytes);
          bytes = d.rotatePage(0, 90);
          d.free();
        } catch { break; }
      }
    }]);

    for (const [name, fn] of ops) {
      console.log(`  ${name}`);
      const r = await bench(name, fn, { ...opts, fixture: f.tag });
      r.fixture = f.tag;
      r.category = f.category;
      results.ops.push(r);
      if (r.error) {
        console.log(`    error: ${r.error}`);
      } else {
        console.log(`    median=${r.median_ms.toFixed(2)}ms  p90=${r.p90_ms.toFixed(2)}ms  rss_delta=${(r.mem_delta_rss / 1024 / 1024).toFixed(1)}MiB`);
      }
    }
  }

  if (opts.out) await persist(results, opts.out);
  printSummary(results);
}

function openPdfDoc(PdfDoc, bytes) {
  if (typeof PdfDoc.open === 'function') return PdfDoc.open(bytes);
  return new PdfDoc(bytes);
}

async function persist(results, out) {
  const { writeFile, mkdir } = await import('node:fs/promises');
  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, JSON.stringify(results, null, 2));
  console.log(`\nWrote ${out}`);
}

function printSummary(results) {
  console.log('\n== summary ==');
  console.log(`cold init: ${results.cold_init.median_ms.toFixed(2)}ms (p90 ${results.cold_init.p90_ms.toFixed(2)}ms)`);
  for (const f of results.fixtures) {
    console.log(`\n${f.tag} (${f.category}, ${(f.bytes / 1024).toFixed(0)} KiB, ${f.pageCount}p${f.private ? ', private' : ''})`);
    for (const op of results.ops.filter((o) => o.fixture === f.tag)) {
      if (op.error) {
        console.log(`  ${op.name.padEnd(28)} ERROR: ${op.error}`);
      } else {
        console.log(`  ${op.name.padEnd(28)} ${op.median_ms.toFixed(2)}ms (p90 ${op.p90_ms.toFixed(2)}ms)`);
      }
    }
  }
}

main().catch((e) => {
  console.error(e);
  exit(1);
});
