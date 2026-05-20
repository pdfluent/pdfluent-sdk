# PDFluent — License UX (developer flow)

Backed by `scripts/bindings/check_license_e2e_parity.py`
(`COMMERCIAL_LICENSE_E2E_TRUE_100_PERCENT_GREEN`; 140 total / 124 supported)
and `docs/licensing.md`. The flow is identical in shape across bindings;
names follow [binding-api-conventions.md](binding-api-conventions.md).

## States

| State | How reached | Behaviour |
| --- | --- | --- |
| Trial / default | no key set | SDK works; output may be marked. |
| Activated | valid key via `set_license_key` or `PDFLUENT_LICENSE_KEY` | full entitlement per tier. |
| Already-set | activate when already active | idempotent; status reflects active key. |
| Invalid key | bad/garbage key | typed license error; **no silent fallback** — the call fails, state stays Trial. |
| Expired / bad signature | expired or wrong-signed payload | typed license error code; remains unlicensed. |

## Flow (all bindings)

1. **Activate** — `set_license_key(key)` (or env var `PDFLUENT_LICENSE_KEY`).
2. **Status** — `license_info()` → `{ tier, output_is_marked, ... }`.
3. **Deactivate / re-key** — set a new key or clear per the binding's API.

Rust example (from the verified `pdfluent-examples/rust`):
```rust
use pdfluent::{license_info, set_license_key};
set_license_key(std::env::var("PDFLUENT_LICENSE_KEY")?.as_str())?;
let info = license_info();
println!("tier={:?} marked={}", info.tier, info.output_is_marked);
```

## Environment variable

`PDFLUENT_LICENSE_KEY`, when set, is honoured as the activation source so
keys need not be hard-coded. Never commit real keys.

## No silent fallback

A failed activation (invalid/expired/bad-signature) returns a **typed
error**; the SDK does not silently continue as if activated. Verify with
`license_info()` after activation.
