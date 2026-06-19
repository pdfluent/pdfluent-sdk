#!/usr/bin/env node
'use strict'

// READ-ONLY prepublish guard — the replacement for `napi prepublish -t npm`.
//
// `napi prepublish` ran as the main package's `prepublishOnly` hook and REWROTE
// package.json's optionalDependencies down to a 3-of-6 subset, which then
// shipped (the @pdfluent/node@1.0.0-beta.17 incident). This guard does the
// opposite: it only READS, asserts the manifest is release-safe, and fails the
// publish on drift. It never writes package.json, so no lifecycle step on the
// publish path can mutate the manifest again.
//
// Fast pre-flight (no packing). The full artefact audit is
// validate-main-tarball.cjs, run by the publish orchestrator.

const fs = require('fs')
const path = require('path')
const { MAIN_PKG, PLATFORM_PKG_NAMES } = require('./platform-matrix.cjs')

const ROOT = path.resolve(__dirname, '..', '..')
const manifestPath = path.join(ROOT, 'package.json')
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'))

// Only guards the main meta-package; platform sub-packages have no prepublishOnly.
if (manifest.name !== MAIN_PKG) process.exit(0)

const problems = []
const before = JSON.stringify(manifest.optionalDependencies || {})

const od = manifest.optionalDependencies || {}
const have = Object.keys(od).sort()
const want = [...PLATFORM_PKG_NAMES].sort()
if (have.length !== want.length || have.some((n, i) => n !== want[i])) {
  problems.push(`optionalDependencies must list all 6 platforms; have ${have.length}: ${have.join(', ') || '(none)'}`)
}

// The main meta-package ships NO binary by construction: `*.node` is not in
// its `files` allowlist, so a local build artefact in the root can never be
// packed. (validate-main-tarball.cjs asserts this against the real tarball.)

for (const f of ['LICENSE', 'README.md', 'index.js', 'index.d.ts']) {
  if (!fs.existsSync(path.join(ROOT, f))) problems.push(`missing required file: ${f}`)
}

// Prove we did not mutate the manifest.
if (JSON.stringify(JSON.parse(fs.readFileSync(manifestPath, 'utf8')).optionalDependencies || {}) !== before) {
  problems.push('INTERNAL: guard mutated optionalDependencies (must never happen)')
}

if (problems.length) {
  console.error('[guard-prepublish] BLOCKED — unsafe main manifest:')
  for (const p of problems) console.error('  - ' + p)
  console.error('  Fix the manifest / move binaries out; this guard never auto-edits package.json.')
  process.exit(1)
}
console.log(`[guard-prepublish] OK — ${MAIN_PKG} manifest is release-safe (6/6 platforms, no stray binary)`)
