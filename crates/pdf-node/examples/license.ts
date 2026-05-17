/**
 * License activation example for @pdfluent/node.
 *
 * Demonstrates the canonical surface:
 *   - {@link activate} / {@link setLicenseKey}  →  activate a key
 *   - {@link status} / {@link licenseStatus}    →  query current state
 *   - {@link PdfluentLicenseError}              →  typed error with .code
 *
 * Compile & type-check (no execution required):
 *
 * ```bash
 * cd crates/pdf-node
 * npx tsc --strict --noEmit examples/license.ts
 * ```
 */

import {
  activate,
  status,
  PdfluentError,
  PdfluentLicenseError,
  type LicenseStatus,
} from '../index.js'

function describe(s: LicenseStatus): string {
  return [
    `active=${s.active}`,
    `tier=${s.tier}`,
    `source=${s.source}`,
    `outputIsMarked=${s.outputIsMarked}`,
    `expiresAt=${s.expiresAt ?? 'null'}`,
  ].join(' ')
}

function main(): number {
  // 1. Snapshot the pre-activation state.
  const before: LicenseStatus = status()
  console.log('before:', describe(before))

  // 2. Activate.  In production replace 'tier:developer' with a real key.
  try {
    activate('tier:developer')
  } catch (e) {
    if (e instanceof PdfluentLicenseError) {
      // Stable code — no message-pattern matching required.
      console.error(`activation failed: code=${e.code} message=${e.message}`)
      if (e.cause) {
        console.error(`  cause: ${e.cause}`)
      }
      return 1
    }
    // Any other PdfluentError still carries `.code` so the same pattern
    // works for non-license failures.
    if (e instanceof PdfluentError) {
      console.error(`unexpected error: code=${e.code} message=${e.message}`)
      return 2
    }
    throw e
  }

  // 3. Snapshot the post-activation state.
  const after: LicenseStatus = status()
  console.log('after:', describe(after))

  // 4. Demonstrate the typed-error path with a deliberately invalid key.
  //    The Rust OnceLock rejects a second different-tier activation, so this
  //    branch reliably produces a PdfluentLicenseError.
  try {
    activate('tier:not_a_real_tier')
  } catch (e) {
    if (e instanceof PdfluentLicenseError && e.code === 'E-LICENSE-INVALID') {
      console.log(`expected typed error: code=${e.code}`)
    } else {
      console.error('unexpected error shape:', e)
      return 3
    }
  }

  return 0
}

process.exitCode = main()
