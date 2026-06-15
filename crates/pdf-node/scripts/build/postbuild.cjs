#!/usr/bin/env node
/*
 * Postbuild: re-apply the hand-maintained typed-error layer that `napi build`
 * regenerates away.
 *
 * `napi build` rewrites `index.js` and `index.d.ts` from the Rust source on
 * every build, which silently deletes the `PdfluentError` / `PdfluentLicenseError`
 * classes and the license-function wrappers (turning typed license errors back
 * into raw `GenericFailure`). This script appends those layers back from their
 * source-of-truth template files (`typed-error-layer.js` / `.d.ts`).
 *
 * Wired as the second half of the package `build` script
 * (`napi build ... && node scripts/build/postbuild.cjs`) so it always runs
 * right after generation. Idempotent: keyed on the `@pdfluent-typed-error-layer`
 * marker, so re-running it (or running it on an already-patched file) is a no-op.
 */
'use strict'

const fs = require('fs')
const path = require('path')

const MARKER = '@pdfluent-typed-error-layer'
const pkgRoot = path.resolve(__dirname, '..', '..') // crates/pdf-node

const targets = [
  { generated: 'index.js', template: 'typed-error-layer.js' },
  { generated: 'index.d.ts', template: 'typed-error-layer.d.ts' },
]

let applied = 0
let skipped = 0

for (const { generated, template } of targets) {
  const genPath = path.join(pkgRoot, generated)
  const tplPath = path.join(__dirname, template)

  if (!fs.existsSync(genPath)) {
    console.error(`[postbuild] ERROR: generated file missing: ${generated}`)
    process.exit(1)
  }

  let content = fs.readFileSync(genPath, 'utf8')
  if (content.includes(MARKER)) {
    skipped++
    continue // already patched — idempotent no-op
  }

  const layer = fs.readFileSync(tplPath, 'utf8')
  if (!content.endsWith('\n')) content += '\n'
  fs.writeFileSync(genPath, `${content}\n${layer}`)
  applied++
  console.log(`[postbuild] re-applied typed-error layer -> ${generated}`)
}

if (applied === 0) {
  console.log(`[postbuild] typed-error layer already present (idempotent, ${skipped} file(s) checked)`)
}
