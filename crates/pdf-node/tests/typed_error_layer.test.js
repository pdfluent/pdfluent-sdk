// Regression guard for the Node build hazard.
//
// `napi build` regenerates index.js / index.d.ts from the Rust source and would
// silently delete the hand-maintained typed-error layer (the PdfluentError
// class). `scripts/build/postbuild.cjs` re-applies it after every build. If that re-application ever breaks (or someone
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
  test('PdfluentError is an exported class', () => {
    expect(typeof mod.PdfluentError).toBe('function')
    expect(mod.PdfluentError.prototype instanceof Error).toBe(true)
  })

  test('no licence entry point survives on the module surface', () => {
    // The inverse of what this file asserted until #226. A `napi build` that
    // resurrected the old layer, or a hand-edit that put a key back, would be
    // invisible otherwise: nothing else here reads the module's export list.
    for (const gone of [
      'activate',
      'setLicenseKey',
      'setLicensePublicKey',
      'setLicensePayload',
      'licenseStatus',
      'PdfluentLicenseError',
    ]) {
      expect(mod[gone]).toBeUndefined()
    }
  })

  test('index.d.ts still declares the typed-error class and no licence one', () => {
    const dts = fs.readFileSync(path.join(__dirname, '..', 'index.d.ts'), 'utf8')
    expect(dts).toMatch(/export declare class PdfluentError/)
    expect(dts).not.toMatch(/PdfluentLicenseError/)
    expect(dts).not.toMatch(/LicenseStatus/)
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

  test('ESM re-exports the typed-error class', () => {
    expect(esmExports).toContain('PdfluentError')
    expect(esmExports).not.toContain('PdfluentLicenseError')
  })
})
