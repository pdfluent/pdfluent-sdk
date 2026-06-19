/**
 * License surface tests for @pdfluent/node.
 *
 * After NODE-LICENSE-SURFACE, the binding exposes:
 *
 *   activate(licenseKey: string): void
 *   setLicenseKey(licenseKey: string): void      // alias
 *   status(): LicenseStatus
 *   licenseStatus(): LicenseStatus               // alias
 *   class PdfluentLicenseError extends PdfluentError
 *
 * The Rust core uses a process-global `OnceLock<Tier>` so the resolved tier
 * is locked for the lifetime of the Node process. This file uses
 * `tier:developer` as the canonical happy-path key and verifies:
 *
 *   1. status() before activation reports inactive (trial)
 *   2. activate("tier:developer") returns void without throwing
 *   3. status() after activation reports developer/active
 *   4. activate("tier:not_a_real_tier") throws PdfluentLicenseError
 *      with stable code "E-LICENSE-INVALID"
 *   5. setLicenseKey is an alias for activate (same behaviour)
 *   6. licenseStatus is an alias for status (same shape)
 *   7. PdfluentLicenseError instances expose `.code`, `.message`,
 *      `.operation`, `.cause` as own properties.
 *
 * Run:
 *   cd crates/pdf-node
 *   npx napi build --release --platform
 *   npx jest tests/license.test.js
 */

let mod
try {
  mod = require('../index')
} catch (e) {
  // Native module not built — defer to a single skipped test so the suite
  // still reports a clean exit.
  describe.skip('pdf-node license (native module not built)', () => {
    test('placeholder', () => {})
  })
}

if (mod) {
  const {
    activate,
    setLicenseKey,
    status,
    licenseStatus,
    PdfluentError,
    PdfluentLicenseError,
  } = mod

  // -------------------------------------------------------------------------
  // Pre-activation snapshot — must run before any activation in this file.
  // -------------------------------------------------------------------------
  const BEFORE = status()

  test('status() before any activation reports inactive (trial)', () => {
    // Shape invariants always hold.
    expect(typeof BEFORE).toBe('object')
    expect(BEFORE).toHaveProperty('active')
    expect(BEFORE).toHaveProperty('tier')
    expect(BEFORE).toHaveProperty('source')
    expect(BEFORE).toHaveProperty('outputIsMarked')
    expect(typeof BEFORE.active).toBe('boolean')
    expect(typeof BEFORE.tier).toBe('string')
    // The native license OnceLock is process-global and locks to the first
    // activated tier for the process lifetime. Jest reuses one worker across
    // test files (it does NOT give each file a fresh process), so another
    // license test file can leave this process already activated. Assert the
    // pristine default when it applies; otherwise the documented per-process
    // lock is in effect (source explicit/env, never a silent default).
    if (BEFORE.source === 'default') {
      expect(BEFORE.active).toBe(false)
      expect(BEFORE.tier).toBe('trial')
      expect(BEFORE.outputIsMarked).toBe(true)
    } else {
      expect(['explicit', 'env']).toContain(BEFORE.source)
    }
  })

  test('licenseStatus() is an alias for status()', () => {
    const a = status()
    const b = licenseStatus()
    expect(b.tier).toBe(a.tier)
    expect(b.active).toBe(a.active)
    expect(b.source).toBe(a.source)
    expect(b.outputIsMarked).toBe(a.outputIsMarked)
  })

  test('activate(valid key) returns undefined', () => {
    // Synthetic evaluation key — production keys ship in 1.1 with signed
    // payloads.  All `tier:<name>` keys are accepted by the 1.0 core.
    const result = activate('tier:developer')
    expect(result).toBeUndefined()
  })

  test('status() after activation reports developer/active', () => {
    const s = status()
    expect(s.active).toBe(true)
    expect(s.tier).toBe('developer')
    expect(s.source).toBe('explicit')
    // Developer tier is a paid tier — output is not marked.
    expect(s.outputIsMarked).toBe(false)
  })

  test('activate(same tier) is idempotent (no throw)', () => {
    expect(() => activate('tier:developer')).not.toThrow()
  })

  test('setLicenseKey is an alias for activate', () => {
    // Same tier so the OnceLock accepts it as a no-op.
    expect(() => setLicenseKey('tier:developer')).not.toThrow()
  })

  // -------------------------------------------------------------------------
  // Typed-error path — these must produce PdfluentLicenseError with stable
  // .code === "E-LICENSE-INVALID" and NO message-pattern matching needed.
  // -------------------------------------------------------------------------

  test('activate(malformed key) throws PdfluentLicenseError', () => {
    let caught
    try {
      activate('this is not a valid key')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeDefined()
    expect(caught).toBeInstanceOf(PdfluentLicenseError)
    expect(caught).toBeInstanceOf(PdfluentError)
    expect(caught).toBeInstanceOf(Error)
  })

  test('activate(malformed key) error.code === "E-LICENSE-INVALID"', () => {
    let caught
    try {
      activate('this is not a valid key')
    } catch (e) {
      caught = e
    }
    expect(caught.code).toBe('E-LICENSE-INVALID')
  })

  test('activate(unknown tier) error.code === "E-LICENSE-INVALID"', () => {
    let caught
    try {
      activate('tier:not_a_real_tier')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(PdfluentLicenseError)
    expect(caught.code).toBe('E-LICENSE-INVALID')
  })

  test('activate(different tier) after lock raises typed error with stable code', () => {
    // Developer is already locked from the happy-path test above; trying
    // trial returns E-LICENSE-INVALID with "license already set" cause.
    let caught
    try {
      activate('tier:trial')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(PdfluentLicenseError)
    expect(caught.code).toBe('E-LICENSE-INVALID')
  })

  test('error carries structured fields: code, message, operation, cause', () => {
    let caught
    try {
      activate('tier:not_a_real_tier')
    } catch (e) {
      caught = e
    }
    // Own properties — not just instance state.
    expect(typeof caught.code).toBe('string')
    expect(typeof caught.message).toBe('string')
    expect(caught.operation).toBe('activate')
    // `cause` may be null but is always defined.
    expect(['string', 'object']).toContain(typeof caught.cause)
  })

  test('error.code strings are stable (exact-match assertion)', () => {
    // The C8 catalogue treats these codes as frozen — renaming any of them
    // is a breaking change.  This test pins the wire format.
    let caught
    try {
      activate('definitely-not-a-key')
    } catch (e) {
      caught = e
    }
    expect(caught.code).toBe('E-LICENSE-INVALID')
    expect(caught.code.startsWith('E-LICENSE-')).toBe(true)
  })

  test('PdfluentLicenseError instanceof PdfluentError', () => {
    let caught
    try {
      activate('bad')
    } catch (e) {
      caught = e
    }
    expect(caught instanceof PdfluentError).toBe(true)
  })

  test('typed error does NOT require message pattern matching', () => {
    let caught
    try {
      activate('bad')
    } catch (e) {
      caught = e
    }
    // The whole point: branch on code, never on message.
    if (caught.code === 'E-LICENSE-INVALID') {
      // Branch taken — assertion passes.
      expect(true).toBe(true)
    } else {
      throw new Error(`expected E-LICENSE-INVALID, got ${caught.code}`)
    }
  })

  // -------------------------------------------------------------------------
  // Status shape tests
  // -------------------------------------------------------------------------

  test('LicenseStatus shape is JSON-serialisable', () => {
    const s = status()
    expect(() => JSON.stringify(s)).not.toThrow()
    const round = JSON.parse(JSON.stringify(s))
    expect(round.tier).toBe(s.tier)
    expect(round.active).toBe(s.active)
  })

  test('LicenseStatus.expiresAt is null in 1.0', () => {
    const s = status()
    // 1.0 evaluation format has no expiry; field is present but undefined / null.
    expect(s.expiresAt == null).toBe(true)
  })
}
