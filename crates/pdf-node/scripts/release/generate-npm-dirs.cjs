#!/usr/bin/env node
'use strict'

// Deterministic generator for the six platform sub-package directories under
// `npm/<tag>/`. Replaces BOTH:
//   - `napi create-npm-dir` (non-deterministic: emitted only a 3-of-6 subset
//     on this repo), and
//   - `napi prepublish` (mutated the MAIN package.json's optionalDependencies
//     in a prepublishOnly hook → shipped the broken `@pdfluent/node@1.0.0-beta.17`).
//
// Properties:
//   - Refuses to run unless ALL six built `index.<tag>.node` artefacts exist,
//     so a partial build can never produce a partial platform set.
//   - Pure function of (matrix, version, built binaries): running it twice
//     yields byte-identical package.json files (asserted by the regression test).
//   - Each platform package ships exactly its one binary + LICENSE + README,
//     with correct os/cpu/libc so npm installs only the matching platform.
//
// Platform-package version: defaults to the main package version. Override with
// PDFLUENT_NODE_PLATFORM_VERSION for the packaging-only-main-patch case
// (e.g. main 1.0.0-beta.17.1 re-pinning already-published platforms at beta.17).

const fs = require('fs')
const path = require('path')
const { MATRIX, nodeFileFor, buildPlatformPkg } = require('./platform-matrix.cjs')

const ROOT = path.resolve(__dirname, '..', '..') // crates/pdf-node
const NPM_DIR = path.join(ROOT, 'npm')

function die(msg) {
  console.error(`[generate-npm-dirs] ERROR: ${msg}`)
  process.exit(1)
}

const base = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'))
const platformVersion = process.env.PDFLUENT_NODE_PLATFORM_VERSION || base.version
if (!platformVersion) die('could not resolve platform-package version')

for (const f of ['LICENSE', 'README.md']) {
  if (!fs.existsSync(path.join(ROOT, f))) die(`missing ${f} in package root`)
}

// Fail loud unless every platform binary is present BEFORE writing anything —
// no partial platform set is ever produced.
const missing = MATRIX.map((m) => nodeFileFor(m.tag)).filter(
  (n) => !fs.existsSync(path.join(ROOT, n)),
)
if (missing.length) {
  die(`missing built binaries (need all 6 before generating): ${missing.join(', ')}`)
}

fs.rmSync(NPM_DIR, { recursive: true, force: true })

for (const m of MATRIX) {
  const dir = path.join(NPM_DIR, m.tag)
  fs.mkdirSync(dir, { recursive: true })
  const nodeFile = nodeFileFor(m.tag)
  const pkg = buildPlatformPkg(m, platformVersion, base)
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify(pkg, null, 2) + '\n')
  fs.copyFileSync(path.join(ROOT, nodeFile), path.join(dir, nodeFile))
  fs.copyFileSync(path.join(ROOT, 'LICENSE'), path.join(dir, 'LICENSE'))
  fs.copyFileSync(path.join(ROOT, 'README.md'), path.join(dir, 'README.md'))
  console.log(`  ${m.tag.padEnd(18)} v${platformVersion}  ${nodeFile} + LICENSE + README`)
}

console.log(`[generate-npm-dirs] wrote ${MATRIX.length} platform packages to npm/`)
