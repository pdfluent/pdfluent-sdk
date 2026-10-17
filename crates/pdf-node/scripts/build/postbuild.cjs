#!/usr/bin/env node
/*
 * Postbuild: re-apply the hand-maintained layers that `napi build`
 * regenerates away.
 *
 * `napi build` rewrites `index.js` and `index.d.ts` from the Rust source on
 * every build, which silently deletes:
 *
 * - the `PdfluentError` class and the license-function wrappers (turning
 *   typed errors back into raw `GenericFailure`), re-applied from the
 *   source-of-truth template files (`typed-error-layer.js` / `.d.ts`); and
 * - the `iterChunks` re-export, whose implementation lives in the
 *   hand-written `chunks.js` / `chunks.d.ts` beside the entry point (the
 *   native side does not know about it either).
 *
 * Wired as the second half of the package `build` script
 * (`napi build ... && node scripts/build/postbuild.cjs`) so it always runs
 * right after generation. Idempotent: keyed on the `@pdfluent-typed-error-layer`
 * and `@pdfluent-chunks-layer` markers, so re-running it (or running it on an
 * already-patched file) is a no-op.
 *
 * Self-check at the end: exactly one `PdfluentError` class must exist across
 * the JS entry surfaces, and it must be the shared one in `pdfluent-error.js`
 * (required by the stub this script appends). A stale inline class left in
 * `index.js` next to the stub — or any second definition — revives the
 * two-class `instanceof` failure, so the build fails loudly instead.
 */
'use strict'

const fs = require('fs')
const path = require('path')

const MARKER = '@pdfluent-typed-error-layer'
const CHUNKS_MARKER = '@pdfluent-chunks-layer'
const pkgRoot = path.resolve(__dirname, '..', '..') // crates/pdf-node

const targets = [
  { generated: 'index.js', template: 'typed-error-layer.js' },
  { generated: 'index.d.ts', template: 'typed-error-layer.d.ts' },
]

// The chunker export: `chunks.js` holds the implementation, so the entry
// point only re-exports it. Kept inline (it is two lines); the typed-error
// layer above keeps templates because it is a hundred.
const chunksJs = `
// @pdfluent-chunks-layer
// Re-applied by scripts/build/postbuild.cjs after every napi build (see
// chunks.js, the source of truth). Do NOT edit this copy.
module.exports.iterChunks = require('./chunks').iterChunks
`
const chunksDts = `
// @pdfluent-chunks-layer
// Re-applied by scripts/build/postbuild.cjs after every napi build (see
// chunks.d.ts, the source of truth). Do NOT edit these lines.
export { iterChunks } from './chunks'
export type { Chunk, ChunkMetadata, ChunkTable, IterChunksOptions, TocItem } from './chunks'
`
const chunksTargets = [
  { generated: 'index.js', block: chunksJs },
  { generated: 'index.d.ts', block: chunksDts },
]

function appendBlock(genPath, generated, marker, block) {
  let content = fs.readFileSync(genPath, 'utf8')
  if (content.includes(marker)) return false // already patched — idempotent no-op
  if (!content.endsWith('\n')) content += '\n'
  fs.writeFileSync(genPath, `${content}${block}`)
  return true
}

let applied = 0
let skipped = 0

for (const { generated, template } of targets) {
  const genPath = path.join(pkgRoot, generated)
  const tplPath = path.join(__dirname, template)

  if (!fs.existsSync(genPath)) {
    console.error(`[postbuild] ERROR: generated file missing: ${generated}`)
    process.exit(1)
  }

  const layer = fs.readFileSync(tplPath, 'utf8')
  if (appendBlock(genPath, generated, MARKER, `\n${layer}`)) {
    applied++
    console.log(`[postbuild] re-applied typed-error layer -> ${generated}`)
  } else {
    skipped++
  }
}

for (const { generated, block } of chunksTargets) {
  const genPath = path.join(pkgRoot, generated)

  if (!fs.existsSync(genPath)) {
    console.error(`[postbuild] ERROR: generated file missing: ${generated}`)
    process.exit(1)
  }

  if (appendBlock(genPath, generated, CHUNKS_MARKER, block)) {
    applied++
    console.log(`[postbuild] re-applied chunks layer -> ${generated}`)
  } else {
    skipped++
  }
}

if (applied === 0) {
  console.log(`[postbuild] layers already present (idempotent, ${skipped} file(s) checked)`)
}

// Self-check (runs on every build, applied or skipped): the built entry
// points must surface exactly one PdfluentError class — the shared one.
function fail(message) {
  console.error(`[postbuild] ERROR: ${message}`)
  process.exit(1)
}

function countClassDefinitions(text) {
  const matches = text.match(/\bclass\s+PdfluentError\b/g)
  return matches ? matches.length : 0
}

const entryJs = fs.readFileSync(path.join(pkgRoot, 'index.js'), 'utf8')
if (!/require\(['"]\.\/pdfluent-error['"]\)/.test(entryJs)) {
  fail('built index.js does not require ./pdfluent-error (stale inline layer?) — regenerate with `napi build`')
}

const classCounts = {}
for (const file of ['index.js', 'index.mjs', 'chunks.js', 'pdfluent-error.js']) {
  const filePath = path.join(pkgRoot, file)
  if (!fs.existsSync(filePath)) fail(`entry surface missing: ${file}`)
  classCounts[file] = countClassDefinitions(fs.readFileSync(filePath, 'utf8'))
}
const total = Object.values(classCounts).reduce((a, b) => a + b, 0)
if (total !== 1 || classCounts['pdfluent-error.js'] !== 1) {
  fail(
    `expected exactly one PdfluentError class (in pdfluent-error.js), found ${total}: ` +
      Object.entries(classCounts)
        .map(([file, count]) => `${file}=${count}`)
        .join(' ')
  )
}

const esm = fs.readFileSync(path.join(pkgRoot, 'index.mjs'), 'utf8')
if (!esm.includes('PdfluentError')) {
  fail('index.mjs does not re-export PdfluentError')
}
console.log('[postbuild] self-check: one shared PdfluentError across CJS, ESM and chunks surfaces')
