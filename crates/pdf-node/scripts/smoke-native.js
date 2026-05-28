/**
 * Native binary smoke test — C4 cross-platform matrix.
 *
 * Run from repo root:
 *   node crates/pdf-node/scripts/smoke-native.js
 *
 * The script must be run after `npm run build` inside crates/pdf-node.
 * It verifies that the native .node binary loads and that the core API
 * surface works end-to-end on the current platform.
 *
 * Platforms tested in CI (dormant until GitLab pipeline is wired):
 *   darwin-arm64, darwin-x64, linux-arm64-gnu, linux-x64-gnu,
 *   linux-x64-musl, win32-x64-msvc
 */

'use strict'

const fs = require('fs')
const path = require('path')
const os = require('os')

// ── Resolve fixture path ──────────────────────────────────────────────────────

const FIXTURES = path.resolve(__dirname, '..', '..', '..', 'fixtures')
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf')

if (!fs.existsSync(SAMPLE_PDF)) {
  console.error(`[smoke] fixture not found: ${SAMPLE_PDF}`)
  process.exit(1)
}

// ── Load binding ──────────────────────────────────────────────────────────────

let PdfDocument
try {
  ;({ PdfDocument } = require('../index.js'))
} catch (err) {
  console.error(`[smoke] failed to load native binding: ${err.message}`)
  console.error('  → run: cd crates/pdf-node && npm run build')
  process.exit(1)
}

const platform = `${process.platform}-${process.arch}`
console.log(`[smoke] platform: ${platform}  node: ${process.version}`)

// ── Core 5-line smoke ─────────────────────────────────────────────────────────
// This exact pattern is the C4 spec smoke:
//   PdfDocument.open(...).pageCount  must return a positive integer.

const buf = fs.readFileSync(SAMPLE_PDF)
const doc = PdfDocument.open(buf)
const pages = doc.pageCount
if (typeof pages !== 'number' || pages < 1) {
  console.error(`[smoke] FAIL — pageCount returned: ${pages}`)
  process.exit(1)
}
console.log(`[smoke] pageCount: ${pages}  ✓`)

// ── Extended surface check ────────────────────────────────────────────────────

const info = doc.info()
console.log(`[smoke] info.title: ${info.title ?? '(none)'}  ✓`)

const text = doc.extractText(0)
if (typeof text !== 'string') {
  console.error('[smoke] FAIL — extractText returned non-string')
  process.exit(1)
}
console.log(`[smoke] extractText(0): ${text.slice(0, 40).replace(/\n/g, '↵')}…  ✓`)

const geo = doc.pageGeometry(0)
if (!(geo.width > 0 && geo.height > 0)) {
  console.error('[smoke] FAIL — pageGeometry returned zero dimensions')
  process.exit(1)
}
console.log(`[smoke] geometry: ${geo.width.toFixed(1)} × ${geo.height.toFixed(1)} pt  ✓`)

const annots = doc.annotations(0)
if (!Array.isArray(annots)) {
  console.error('[smoke] FAIL — annotations returned non-array')
  process.exit(1)
}
console.log(`[smoke] annotations(0): ${annots.length} items  ✓`)

// ── Error class smoke ─────────────────────────────────────────────────────────

const {
  PdfluentError,
  PdfluentIoError,
  PdfluentParseError,
  openPdf,
} = require('../index.js')

let caught = false
try {
  openPdf('/nonexistent/path/that/does/not/exist.pdf')
} catch (err) {
  caught = true
  if (!(err instanceof PdfluentError)) {
    console.error(`[smoke] FAIL — error is not PdfluentError: ${err.constructor.name}`)
    process.exit(1)
  }
  console.log(`[smoke] openPdf error → ${err.constructor.name} ("${err.message.slice(0, 60)}")  ✓`)
}
if (!caught) {
  console.error('[smoke] FAIL — expected openPdf to throw on missing file')
  process.exit(1)
}

let caughtParse = false
try {
  PdfDocument.open(Buffer.from('not a pdf'))
} catch (err) {
  caughtParse = true
  console.log(`[smoke] PdfDocument.open(garbage) → ${err.constructor.name}  ✓`)
}
if (!caughtParse) {
  console.error('[smoke] FAIL — expected open(garbage) to throw')
  process.exit(1)
}

// ── ESM interop check (Node 12+) ──────────────────────────────────────────────

const mjs = path.join(__dirname, '..', 'index.mjs')
if (!fs.existsSync(mjs)) {
  console.error('[smoke] FAIL — index.mjs not found')
  process.exit(1)
}
console.log('[smoke] index.mjs present  ✓')

// ── Done ──────────────────────────────────────────────────────────────────────

console.log(`\n[smoke] ALL CHECKS PASSED on ${platform}`)
