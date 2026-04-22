# PDFluent error codes

Every `pdfluent::Error` variant exposes a stable
`code() -> &'static str` and a `docs_url() -> &'static str`.
The strings are frozen for the 1.x line per
[STABILITY.md §8](../../../../STABILITY.md).

## Full code list

| Code | Variant | When it fires |
|---|---|---|
| `E-IO-FILE-NOT-FOUND` | `Error::FileNotFound { path }` | `PdfDocument::open(path)` with a missing path. |
| `E-IO-GENERIC` | `Error::Io { source, path }` | Any underlying `std::io::Error` not reducible to a more specific code. |
| `E-MEMORY-BUDGET-EXCEEDED` | `Error::MemoryBudgetExceeded { requested, limit }` | Input size exceeds `OpenOptions::strict_memory_limit`. |
| `E-INVALID-PDF` | `Error::InvalidPdf { byte_offset, reason }` | PDF parse failure (not encryption-related). |
| `E-SECURITY-DECRYPTION-FAILED` | `Error::DecryptionFailed { reason }` | Wrong password, unsupported algorithm, malformed encryption dict. |
| `E-INVALID-SIGNATURE` | `Error::InvalidSignature { reason }` | Signing / verification failure surfaced from `pdf-sign`. |
| `E-INVALID-LICENSE` | `Error::InvalidLicense { reason }` | Malformed / expired license key. |
| `E-LICENSE-FEATURE-NOT-IN-TIER` | `Error::FeatureNotInTier { capability, your_tier, required_tier }` | Capability gated behind a higher tier than the active license grants. |
| `E-LICENSE-CAPABILITY-NOT-COMPILED` | `Error::CapabilityNotCompiled { capability, feature_flag }` | Capability gated behind a Cargo feature that wasn't enabled at build. |
| `E-ENV-UNSUPPORTED-ON-WASM` | `Error::UnsupportedOnWasm { operation }` | Method called on `wasm32-unknown-unknown` where only a native implementation exists. See [WASM_SUPPORT.md](../../../../WASM_SUPPORT.md). |
| `E-ENV-MISSING-DEPENDENCY` | `Error::MissingDependency { dep, install_hint }` | Deferred runtime (see STABILITY.md §3.3): `linearize`, `embed_font`, `add_decoration`, `add_watermark`, `flatten_forms`. |
| `E-INTERNAL` | `Error::Internal { message, crate_version }` | Catch-all with a descriptive message. Prefer filing an issue if you hit one — we want to promote these to typed variants over time. |

## For translators

- Treat every `E-…` code as a do-not-translate token.
- The `docs_url()` resolves to `https://pdfluent.com/errors/<code>`
  — these slugs are language-neutral. Per-language error pages
  may localise the **surrounding prose** but not the code in the
  URL.
- `Error` variant names (e.g. `FileNotFound`) are Rust identifiers
  → do not translate.
- The field names (`path`, `reason`, `your_tier`, etc.) are Rust
  identifiers → do not translate.
