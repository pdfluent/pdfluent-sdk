'use strict'

// Regression tests for the @pdfluent/node release process.
//
// These lock in the invariants whose violation shipped the broken
// @pdfluent/node@1.0.0-beta.17 (main meta-package with only 3 of 6 platform
// optionalDependencies, caused by a `napi prepublish` lifecycle hook that
// rewrote package.json during `npm publish`).

const fs = require('fs')
const path = require('path')
const { execFileSync } = require('child_process')

// On Windows `npm` is a .cmd shim; execFileSync does no PATHEXT resolution, so
// spawning it by the bare name fails with ENOENT. `node` is a real .exe and
// needs no such treatment. Even with the .cmd name, Windows refuses to spawn
// a .cmd/.bat file directly (EINVAL) unless it goes through a shell.
const NPM = process.platform === 'win32' ? 'npm.cmd' : 'npm'
const NPM_SHELL_OPT = process.platform === 'win32' ? { shell: true } : {}
const {
  MATRIX,
  PLATFORM_PKG_NAMES,
  MAIN_PKG,
  buildPlatformPkg,
  nodeFileFor,
} = require('../scripts/release/platform-matrix.cjs')

const ROOT = path.resolve(__dirname, '..')
const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'))

// Pack the EXACT main tarball file list once (lifecycle scripts disabled).
function mainPackFiles() {
  const out = execFileSync(NPM, ['pack', '--ignore-scripts', '--dry-run', '--json'], {
    cwd: ROOT,
    encoding: 'utf8',
    ...NPM_SHELL_OPT,
  })
  return JSON.parse(out)[0].files.map((f) => f.path)
}

describe('platform matrix', () => {
  test('defines exactly the six supported platforms', () => {
    expect(MATRIX).toHaveLength(6)
    expect(PLATFORM_PKG_NAMES).toEqual([
      '@pdfluent/node-darwin-arm64',
      '@pdfluent/node-darwin-x64',
      '@pdfluent/node-linux-x64-gnu',
      '@pdfluent/node-linux-x64-musl',
      '@pdfluent/node-linux-arm64-gnu',
      '@pdfluent/node-win32-x64-msvc',
    ])
  })
})

describe('main meta-package manifest', () => {
  test('optionalDependencies lists the FULL six-platform matrix (no subset)', () => {
    const od = manifest.optionalDependencies || {}
    expect(Object.keys(od).sort()).toEqual([...PLATFORM_PKG_NAMES].sort())
    for (const v of Object.values(od)) {
      expect(String(v)).toMatch(/^\d+\.\d+\.\d+/) // concrete version pin
    }
  })

  test('prepublishOnly does NOT run the mutating napi prepublish hook', () => {
    const hook = (manifest.scripts || {}).prepublishOnly || ''
    expect(hook).not.toMatch(/napi\s+prepublish/)
    expect(hook).toMatch(/guard-prepublish/)
  })
})

describe('main tarball contents', () => {
  let files
  beforeAll(() => {
    files = mainPackFiles()
  })

  test('ships NO native .node binary', () => {
    expect(files.filter((f) => f.endsWith('.node'))).toEqual([])
  })

  test('includes LICENSE, README, loader and typings', () => {
    for (const required of ['LICENSE', 'README.md', 'index.js', 'index.d.ts', 'index.mjs']) {
      expect(files).toContain(required)
    }
  })
})

describe('deterministic platform-dir generation', () => {
  test('buildPlatformPkg is a pure function (identical JSON across runs)', () => {
    const a = JSON.stringify(MATRIX.map((m) => buildPlatformPkg(m, '9.9.9', manifest)))
    const b = JSON.stringify(MATRIX.map((m) => buildPlatformPkg(m, '9.9.9', manifest)))
    expect(a).toEqual(b)
  })

  test('each platform package carries correct os/cpu/libc + its single binary', () => {
    for (const m of MATRIX) {
      const p = buildPlatformPkg(m, '1.2.3', manifest)
      expect(p.name).toBe(`@pdfluent/node-${m.tag}`)
      expect(p.os).toEqual([m.os])
      expect(p.cpu).toEqual([m.cpu])
      if (m.libc) expect(p.libc).toEqual([m.libc])
      expect(p.main).toBe(nodeFileFor(m.tag))
      expect(p.files).toContain('LICENSE')
      expect(p.files).toContain(nodeFileFor(m.tag))
    }
  })
})

describe('prepublish guard is read-only (no manifest mutation)', () => {
  test('running the guard does not modify package.json', () => {
    const before = fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')
    // Guard exits 0 (manifest is valid) or non-zero (drift) — either way it
    // must NEVER write package.json. The 1.0.0-beta.17 defect was a hook that
    // DID write it.
    try {
      execFileSync('node', ['scripts/release/guard-prepublish.cjs'], { cwd: ROOT, encoding: 'utf8' })
    } catch (_) {
      /* a non-zero exit is still a valid read-only outcome */
    }
    const after = fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')
    expect(after).toEqual(before)
  })
})
