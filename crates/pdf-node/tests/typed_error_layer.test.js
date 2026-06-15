// Regression guard for the Node build hazard.
//
// `napi build` regenerates index.js / index.d.ts from the Rust source and would
// silently delete the hand-maintained typed-error layer (PdfluentError /
// PdfluentLicenseError + license-function wrappers). `scripts/build/postbuild.cjs`
// re-applies it after every build. If that re-application ever breaks (or someone
// runs `napi build` without the postbuild step), these assertions fail loudly
// instead of shipping raw `GenericFailure` errors to users.
const fs = require('fs')
const path = require('path')

let mod
try {
  mod = require('../index')
} catch (e) {
  mod = null
}

const d = mod ? describe : describe.skip

d('typed-error layer survives napi build', () => {
  test('PdfluentError / PdfluentLicenseError are exported classes', () => {
    expect(typeof mod.PdfluentError).toBe('function')
    expect(typeof mod.PdfluentLicenseError).toBe('function')
    expect(mod.PdfluentLicenseError.prototype instanceof mod.PdfluentError).toBe(true)
  })

  test('license functions are the typed-error wrappers', () => {
    expect(typeof mod.activate).toBe('function')
    expect(typeof mod.setLicenseKey).toBe('function')
    expect(typeof mod.setLicensePublicKey).toBe('function')
    expect(typeof mod.setLicensePayload).toBe('function')
  })

  test('activate(malformed) throws a typed PdfluentLicenseError, not raw GenericFailure', () => {
    // A malformed key is rejected regardless of any ambient license state, so
    // this is environment-independent.
    let caught = null
    try {
      mod.activate('this-is-not-a-valid-license-key')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(mod.PdfluentLicenseError)
    expect(caught).toBeInstanceOf(mod.PdfluentError)
    expect(typeof caught.code).toBe('string')
    expect(caught.code).not.toBe('GenericFailure')
  })

  test('index.d.ts still declares the typed-error classes', () => {
    const dts = fs.readFileSync(path.join(__dirname, '..', 'index.d.ts'), 'utf8')
    expect(dts).toMatch(/export declare class PdfluentError/)
    expect(dts).toMatch(/export declare class PdfluentLicenseError extends PdfluentError/)
  })
})

// Static (no native addon required): the hand-maintained ESM wrapper must not
// drift from the real CJS surface — every name it re-exports has to exist in
// index.js, or ESM consumers get `undefined`.
describe('ESM wrapper (index.mjs) matches CJS surface (index.js)', () => {
  const root = path.join(__dirname, '..')
  const cjs = fs.readFileSync(path.join(root, 'index.js'), 'utf8')
  const mjs = fs.readFileSync(path.join(root, 'index.mjs'), 'utf8')
  const exportBlock = (mjs.match(/export\s*\{([^}]*)\}/) || [, ''])[1]
  const esmExports = exportBlock.split(',').map((s) => s.trim()).filter(Boolean)
  const cjsExports = new Set(
    [...cjs.matchAll(/module\.exports\.([A-Za-z]+)/g)].map((m) => m[1])
  )

  test('every ESM export exists in the CJS entry (no phantom exports)', () => {
    const phantom = esmExports.filter((n) => !cjsExports.has(n))
    expect(phantom).toEqual([])
  })

  test('ESM re-exports the typed-error classes', () => {
    expect(esmExports).toContain('PdfluentError')
    expect(esmExports).toContain('PdfluentLicenseError')
  })
})
