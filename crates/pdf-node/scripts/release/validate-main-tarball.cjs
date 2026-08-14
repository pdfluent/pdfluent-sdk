#!/usr/bin/env node
'use strict'

// Validates the EXACT tarball that `npm publish` would upload for the main
// `@pdfluent/node` meta-package — the artefact, not the workspace directory.
//
// Packs with `npm pack --ignore-scripts` (lifecycle scripts can never run here,
// so nothing mutates the manifest during validation), unpacks the .tgz, and
// asserts the mandatory release invariants. Exits non-zero on ANY violation, so
// it can gate a publish.
//
// Invariants (the four the 1.0.0-beta.17 incident proved we must enforce):
//   1. optionalDependencies lists the FULL six-platform matrix (no subset).
//   2. NO native `*.node` binary inside the main meta-package.
//   3. LICENSE, README.md, index.js, index.d.ts all physically present.
//   4. No private paths / corpus / key leakage in any packed file.

const fs = require('fs')
const os = require('os')
const path = require('path')
const { execFileSync } = require('child_process')
const { MAIN_PKG, PLATFORM_PKG_NAMES } = require('./platform-matrix.cjs')

const ROOT = path.resolve(__dirname, '..', '..') // crates/pdf-node
const problems = []
const fail = (m) => problems.push(m)

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'pdfnode-validate-'))
try {
  // `npm pack --ignore-scripts` → produces the real tarball with NO lifecycle
  // script execution (the manifest cannot be mutated during validation).
  // On Windows `npm` is a .cmd shim and execFileSync does no PATHEXT
  // resolution, so the bare name fails with ENOENT.
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm'
  const out = execFileSync(npm, ['pack', '--ignore-scripts', '--json', '--pack-destination', tmp], {
    cwd: ROOT,
    encoding: 'utf8',
  })
  const meta = JSON.parse(out)[0]
  const tgz = path.join(tmp, meta.filename)
  execFileSync('tar', ['-xzf', tgz, '-C', tmp])
  const pkgDir = path.join(tmp, 'package')
  const manifest = JSON.parse(fs.readFileSync(path.join(pkgDir, 'package.json'), 'utf8'))

  // (0) identity
  if (manifest.name !== MAIN_PKG) fail(`name is ${manifest.name}, expected ${MAIN_PKG}`)

  // (1) FULL six-platform optionalDependencies — the core regression
  const od = manifest.optionalDependencies || {}
  const names = Object.keys(od).sort()
  const want = [...PLATFORM_PKG_NAMES].sort()
  if (names.length !== want.length || names.some((n, i) => n !== want[i])) {
    fail(`optionalDependencies must be exactly the 6 platforms.\n    have: ${names.join(', ') || '(none)'}\n    want: ${want.join(', ')}`)
  }
  // every pin must be a concrete version (no ranges/empties)
  for (const [k, v] of Object.entries(od)) {
    if (!/^\d+\.\d+\.\d+/.test(String(v))) fail(`optionalDependency ${k} has non-concrete version "${v}"`)
  }

  // (2) NO native binary in the main meta-package
  const entries = execFileSync('tar', ['-tzf', tgz], { encoding: 'utf8' }).split('\n').filter(Boolean)
  const nodeBins = entries.filter((e) => e.endsWith('.node'))
  if (nodeBins.length) fail(`main meta-package must ship NO .node binary; found: ${nodeBins.join(', ')}`)

  // (3) required files physically present
  for (const f of ['LICENSE', 'README.md', 'index.js', 'index.d.ts', 'index.mjs']) {
    if (!fs.existsSync(path.join(pkgDir, f))) fail(`missing required file in tarball: ${f}`)
  }

  // (4) no leakage
  const leakRe = /\/Users\/[a-z]+|BEGIN [A-Z ]*PRIVATE KEY|xfa-corpus|xfa-golden|\/opt\/xfa|openrouter|glpat-/
  for (const e of entries) {
    if (e.endsWith('/')) continue
    const p = path.join(tmp, e)
    if (!fs.existsSync(p)) continue
    const buf = fs.readFileSync(p)
    if (leakRe.test(buf.toString('latin1'))) fail(`leakage pattern in packed file: ${e}`)
  }

  console.log(`[validate-main-tarball] ${meta.filename}: ${entries.length} files, ${names.length}/6 platforms, ${nodeBins.length} binaries`)
} finally {
  fs.rmSync(tmp, { recursive: true, force: true })
}

if (problems.length) {
  console.error('[validate-main-tarball] FAIL:')
  for (const p of problems) console.error('  - ' + p)
  process.exit(1)
}
console.log('[validate-main-tarball] PASS — main meta-package is release-safe')
