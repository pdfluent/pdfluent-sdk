# Node.js License Activation

This document describes the license activation surface exposed by
`@pdfluent/node`.  All symbols are declared in
`crates/pdf-node/index.d.ts` and implemented in
`crates/pdf-node/src/license.rs` (Rust → NAPI) and `index.js` (JavaScript
wrapper that constructs typed errors).

The Node binding wraps the canonical Rust API
(`pdfluent::set_license_key` / `pdfluent::license_info`) and matches the
shape of the Python (`pdfluent._native.set_license_key`) and C ABI
(`pdfluent_license_activate_key`) surfaces.

## Surface

### `activate(licenseKey: string): void`

```ts
import { activate } from '@pdfluent/node';

activate('tier:developer');
```

Activate the process-global license from a key string.  The key is
consumed immediately and never stored locally.

The first successful call locks the resolved tier for the lifetime of the
Node process.  Subsequent calls with the **same** tier are idempotent
no-ops; calls with a **different** tier throw `PdfluentLicenseError`
(`code === "E-LICENSE-INVALID"`).

### `setLicenseKey(licenseKey: string): void`

Alias for `activate`.  Matches the Python / Rust naming
(`pdfluent.set_license_key`).

### `status(): LicenseStatus`

```ts
import { status } from '@pdfluent/node';

const s = status();
console.log(s.active, s.tier, s.source);
```

Return the current canonical license state.  Always succeeds; reads are
lock-free.

### `licenseStatus(): LicenseStatus`

Alias for `status`.  Matches the C ABI naming
(`pdfluent_license_status`).

## Types

### `LicenseStatus`

```ts
export interface LicenseStatus {
  active: boolean         // true when a paid tier is active
  tier: string            // "trial" | "developer" | "team" | "business" | "enterprise"
  source: string          // "default" | "env" | "explicit"
  outputIsMarked: boolean // true when output is marked via /Producer (Trial only)
  expiresAt?: string      // ISO-8601 expiry, undefined for evaluation keys
}
```

The `tier` field uses the canonical Rust spelling.  See
[`docs/licensing/cabi.md`](./cabi.md) for the integer-coded equivalent
emitted by the C ABI.

### `PdfluentLicenseError`

```ts
class PdfluentLicenseError extends PdfluentError {
  readonly code: string         // always begins with "E-LICENSE-"
  readonly operation: string    // "activate"
  readonly cause: string | null // optional underlying detail
}
```

Thrown by `activate` / `setLicenseKey` on:

| `code`                            | Cause                                              |
|-----------------------------------|----------------------------------------------------|
| `E-LICENSE-INVALID`               | Key malformed, tier unknown, or different tier locked |
| `E-LICENSE-FEATURE-NOT-IN-TIER`   | Feature requires a higher tier                     |
| `E-LICENSE-CAPABILITY-NOT-COMPILED` | Feature gated behind a Cargo flag at build time  |

Branch on `err.code`.  **Do not** pattern-match on `err.message`.

## Key format (1.0)

Synthetic evaluation keys: `"tier:<name>"` where `<name>` is one of
`trial`, `developer`, `team`, `business`, `enterprise`.

Cryptographically-signed payloads (Ed25519) are accepted by the same
function from release 1.1 onward without breaking the API.

## C8 error catalogue mapping

| C8 code (Rust `pdfluent::Error::code()`) | Node JS class            | `.code` string                         |
|------------------------------------------|--------------------------|----------------------------------------|
| `E-LICENSE-INVALID`                      | `PdfluentLicenseError`   | `"E-LICENSE-INVALID"`                  |
| `E-LICENSE-FEATURE-NOT-IN-TIER`          | `PdfluentLicenseError`   | `"E-LICENSE-FEATURE-NOT-IN-TIER"`      |
| `E-LICENSE-CAPABILITY-NOT-COMPILED`      | `PdfluentLicenseError`   | `"E-LICENSE-CAPABILITY-NOT-COMPILED"`  |

See [`docs/error_catalogue.md`](../error_catalogue.md) for the canonical
source.

## Example

See [`crates/pdf-node/examples/license.ts`](../../crates/pdf-node/examples/license.ts)
for a worked example that:

1. Reads the pre-activation status.
2. Activates a key.
3. Reads the post-activation status.
4. Demonstrates the typed-error path with a deliberately invalid key.

Type-check (no execution required):

```bash
cd crates/pdf-node
npx tsc --strict --noEmit examples/license.ts
```

## Thread safety and process-global state

The license tier is stored in a Rust `OnceLock` — it is written exactly
once per process and then read-only.  `status` and `licenseStatus` are
safe to call from any thread at any time.  `activate` / `setLicenseKey`
are safe to call concurrently but only the first successful call takes
effect.

Node tests that exercise activation must run in dedicated worker
processes (jest's default) so the OnceLock starts empty.

## Security notes

- The SDK never logs license keys.
- The structured error payload (`code`, `message`, `operation`, `cause`)
  reports parse failure modes only; the raw key string is never included.
- Tests use only synthetic evaluation keys (`tier:developer`, etc.).  No
  real signed key should be committed to source control.

## Wire format (internal)

The NAPI layer cannot attach a stable `code` field to `Error` directly
because napi-rs's `Status` enum is fixed.  Instead the Rust side
serialises the structured payload as JSON in the `reason` string and the
`index.js` wrapper unmarshals it into a typed `PdfluentError` subclass.

Wire payload:

```json
{
  "code": "E-LICENSE-INVALID",
  "message": "license key is invalid",
  "operation": "activate",
  "cause": "unknown tier \"platinum\"; expected trial/developer/team/business/enterprise"
}
```

The JS wrapper attaches `code`, `operation`, and `cause` as own
properties on the thrown error.  Consumers branch on `.code` — they do
not see the JSON envelope.

See `crates/pdf-node/src/error.rs` for the canonical encoder and
`crates/pdf-node/index.js#_toPdfluentError` for the decoder.
