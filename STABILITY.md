# PDFluent Stability Contract

**Status:** Active for 1.0 GA. Supersedes any earlier informal policy.
**Last updated:** 2026-04-21 (Slag 2 / Epic 4 / #1232).
**Frozen API source of truth:** [`docs/rfc/0001-sdk-core-api.md`](docs/rfc/0001-sdk-core-api.md).

This document is the single public contract users and downstream binding
authors can rely on when depending on the `pdfluent` crate. It binds both
us (the SDK maintainers) and consumers.

---

## 1. Scope

The contract covers **the `pdfluent` crate** published to crates.io:

- The `pdfluent::*` public surface (types, functions, traits, modules).
- The `pdfluent::prelude::*` re-exports.
- The `Error` variant codes and `docs_url` deep-links.
- The Cargo feature flag set and its semantics.
- The cross-language binding ABI exposed via `pdf-capi` / `pdf-java` /
  `pdf-node` / `pdf-python` (covered in the binding crates' own
  stability docs; kept consistent with this one).

The contract does **not** cover:

- Internal crates (`pdf-engine`, `lopdf`, `pdf-manip`, `pdf-sign`,
  `pdf-redact`, `pdf-forms`, …). These are implementation details and
  may break between any two `pdfluent` releases.
- Unstable features (see §3).
- Behaviour that falls outside RFC 0001 (see §4).

---

## 2. Semver policy

We follow [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html) strictly
over the 1.x line, with the following concrete rules.

### 2.1 MAJOR (1.x → 2.0)

Required for any of:

- Removing a public item.
- Removing a variant of an enum not marked `#[non_exhaustive]`.
- Changing a public function's parameter count or types.
- Changing a public struct's fields (other than adding behind
  `#[non_exhaustive]`).
- Changing an `Error` variant's stable `code()` string.
- Changing a `docs_url()` path prefix.
- Removing or renaming a Cargo feature that is not in the
  deprecated-aliases list.
- Dropping a platform from the support matrix (see [WASM_SUPPORT.md](WASM_SUPPORT.md)).
- Raising MSRV by more than one stable Rust release.

### 2.2 MINOR (1.x → 1.(x+1))

Allowed without a major bump:

- Adding new public items (functions, types, modules, traits).
- Adding new variants to an enum marked `#[non_exhaustive]`.
- Adding new fields to a struct marked `#[non_exhaustive]`.
- Adding new Cargo features.
- Adding new `Error` variants behind `#[non_exhaustive]`.
- Raising MSRV by exactly one stable Rust release, provided the
  previous MSRV was supported for at least 6 months.
- Promoting an Unstable item to Stable (see §3).

### 2.3 PATCH (1.x.y → 1.x.(y+1))

Bug fixes only. **No** additions to the public surface, no new features,
no new feature flags. If a fix requires a new knob, it ships in the next
MINOR.

A fix that changes observable behaviour for a previously-working caller
(e.g. correcting a Unicode corruption bug) is still a PATCH — the
previous behaviour was a bug, not a contract.

### 2.4 Pre-1.0 behaviour

Versions `1.0.0-alpha.*` / `1.0.0-beta.*` / `1.0.0-rc.*` do **not**
carry the stability contract. Breaking changes between pre-releases
are allowed.

---

## 3. Stability classification

Every item in the public surface is classified as one of:

| Classification | Meaning |
|---|---|
| **Stable** | Follows the full §2 semver policy. |
| **Unstable** | In the public API but marked (see §3.2). May change in any MINOR. |
| **Deferred** | Named in RFC 0001 but not yet implemented; calling returns `Error::MissingDependency` or a similar documented no-op. Runtime wiring follows semver in a later MINOR. |
| **Internal** | `pub(crate)` or in a non-exported module. No contract. |

### 3.1 Stable set (1.0)

Everything re-exported from the crate root or `pdfluent::prelude::*` at
1.0.0 GA, except items explicitly tagged Unstable or Deferred below.

Concretely, this covers:

- `PdfDocument`, `OpenOptions`, `SaveOptions`, `PdfVersion`, `Page`, `Pages`, `TextBlock`
- `PdfMerger`, `MergeOptions`, `BookmarkMergeStrategy`
- `Metadata`, `MetadataMut`
- `PdfFormMut`, `FormField`, `FieldType`
- `EncryptOptions`, `EncryptionAlgorithm`, `Permissions`
- `PdfSigner`, `Pkcs12Signer`, `SignOptions`, `SignatureInfo`, `SignatureValidation`, `SignatureValidationReport`, `SignatureStatus`, `PadesProfile`
- `RedactOptions`
- `WatermarkOptions`, `Position`, `Layer`, `Rotation`
- `PageDecoration`
- `CompressOptions`, `CompressReport`, `FontSubsetReport`, `ToImagesOptions`, `ToImagesReport`, `ImageFormat`, `ImageInsert`, `ImageInsertReport`, `InsertImageFormat`
- `PdfAProfile`, `PdfAValidationReport`, `Violation`
- `Capability`, `CapabilitySet`, `Tier`
- `LicenseInfo`, `set_license_key`, `license_info`
- `Error`, `Result`, `Error::code() -> &'static str`, `Error::docs_url() -> &'static str`

The following methods were added after 1.0 GA and carry the same Stable
contract as §3.1:

| Method | Added | Tracking |
|---|---|---|
| `PdfDocument::extract_text` — Stable | 1.0.1 | #1316 (M4-FACADE-01) |
| `PdfFormMut::set_multi_select` — Stable | `acroform/sdk-closure` | multi-select list box fill (completes AcroForm field-type coverage on the facade) |
| `PdfDocument::has_xfa_form` — Stable | `xfa/sdk-phase1-fill-foundation` | XFA Phase 1 |
| `PdfDocument::xfa_form_model` + `pdfluent::xfa::*` DTOs — Stable | `xfa/sdk-phase1-fill-foundation` | XFA Phase 1; `XfaFieldType` is `#[non_exhaustive]` |
| `PdfDocument::set_xfa_field_value` — Stable | `xfa/sdk-phase1-fill-foundation` | XFA Phase 1; no event scripts / reflow (documented scope) |

### 3.2 Unstable items in 1.0

None at GA. Any future Unstable addition must:

1. Live in a module named `pdfluent::unstable::*` **or** carry a
   rustdoc comment (`///` — a real doc comment, not `//`) whose
   first line starts with `UNSTABLE:` and includes a tracking-issue
   link. The `///` form guarantees the marker is visible in
   `cargo doc` output and can be lint-enforced by doc tooling.
2. Not be re-exported from `pdfluent::prelude`.
3. Be listed in this file.

### 3.3 Deferred (truth-gaps) in 1.0

The following items are on the Stable API surface but their runtime is
deliberately deferred. Calling them returns a typed error — **not** a
silent no-op. Users MUST check the result.

| Item | Current behaviour at 1.0 GA | Tracking |
|---|---|---|
| `PdfDocument::linearize()` | `Error::MissingDependency { dep: "pdf-manip::linearize", .. }` | 1.1 follow-up to #1224 |
| `PdfDocument::embed_font(..)` | `Error::MissingDependency { dep: "pdf-manip::embed_font", .. }` | 1.1 follow-up to #1224 |
| `PdfDocument::add_decoration(..)` (all variants) | `Error::MissingDependency { dep: "pdf-manip::watermark", .. }` | #1223 watermark runtime |
| `PdfDocument::add_watermark(..)` | same as above (delegates) | #1223 |
| `PdfDocument::flatten_forms()` | `Error::MissingDependency { dep: "pdf-manip::flatten_forms", .. }` | #1223 |
| `SaveOptions::with_linearize(true)` | Accepted, currently a no-op on save | 1.1 |
| ~~`PdfFormMut::set_checkbox` / `set_radio` — `/AS` not synced~~ | **Promoted to Stable** in `acroform/sdk-foundation`. The unified writeback chain now updates per-widget `/AS` and regenerates `/AP` appearance streams in every `set_*` call. No longer deferred. | Closed #1245 |
| `EncryptOptions::aes128()` | Backend currently uses `aes256_encryption_state` regardless of algorithm tag. Output is AES-256. | 1.1 follow-up to #1244 |

**flatten_forms GA-blocker is resolved.** The method previously
panicked via `unimplemented!()` in alpha / beta; it now returns
`Error::MissingDependency` matching the other deferred items in
this table. No public method panics on its happy path at 1.0 GA.

**Promotion rule.** When a deferred item acquires its runtime in a
future MINOR, the semver policy for that item becomes Stable. The
change from "always errors with `MissingDependency`" to "succeeds" is
**not** a breaking change — users who check the result flow through
either way.

### 3.4 Explicitly out of 1.0

These were considered and ruled out for 1.0; they are not on the public
surface and will be added in future MINORs without breaking:

- Async (`pdfluent::r#async` module, `async-tokio` feature).
- Image watermarks (`add_image_watermark`).
- `to_xlsx` / `to_pptx`.
- `pages_mut` batch builder (direct methods `rotate_page` / `split_pages` /
  `extract_pages` cover 1.0 use cases).

---

## 4. Frozen RFC relationship

The public API is frozen per [RFC 0001](docs/rfc/0001-sdk-core-api.md).

- The RFC is the design contract. This document is the
  version/compatibility contract.
- Any change to the Stable set requires either an RFC amendment
  (MINOR-compatible additions per §2.2) or a new RFC (MAJOR-only
  changes).
- The RFC's revision log (§14) is the authoritative changelog of
  API-shape decisions. CHANGELOG.md tracks per-release changes.

---

## 5. Prelude policy

`pdfluent::prelude` is the "import everything for 95% of uses" entry
point. Policy:

- **Additions** to the prelude are MINOR-compatible *unless* the added
  name collides with a commonly-imported `std` or `core` type. In that
  case the addition is treated as MAJOR.
- **Removals** from the prelude are MAJOR.
- A type re-exported from the crate root but not from the prelude is
  a **soft signal** that callers should prefer to reach for it via its
  module path (e.g. `pdfluent::decoration::PageDecoration`) when
  readability benefits. Both paths are Stable.

The prelude star-import is verified collision-free by the
`prelude_star_import_exposes_new_types_without_collision` test in
`tests/dx_consolidation.rs`.

---

## 6. Doc example policy

All `/// ``` ... ```` examples in rustdoc **must** compile under
`cargo test --doc`. CI enforces this.

Non-compiling examples are disallowed; a "hypothetical future API" note
must be written as plain prose, not fenced code.

Examples marked `no_run` are allowed only when they genuinely cannot
run in the doctest sandbox (require external files, network, GUI).
They still must compile.

This policy is enforced automatically via `cargo test -p pdfluent
--doc` in CI.

Relation to website content: the website's `how-to-*` Rust snippets
are extracted and compile-tested against master via the Slag 2
pipeline (#1236/#1237/#1238/#1246).

---

## 7. Cargo feature policy

### 7.1 Current features (1.0)

| Feature | Default | Gates |
|---|---|---|
| `signing` | yes | PAdES signing + verification. Compiles `pdf-sign`. |
| `pdfa` | yes | PDF/A validation + conversion. |
| `redaction` | yes | Content + region redaction. |
| `ocr-tesseract` | no | **Reserved name, enables nothing.** Use `pdf-ocr` with its own `tesseract` feature (needs Tesseract + Leptonica installed). |
| `ocr-paddle` | no | **Reserved name, enables nothing.** Use `pdf-ocr` with its own `paddle` feature (ONNX Runtime as a shared library; weights fetched once, then offline). |
| `html-to-pdf` | no | **Not offered, and not planned.** Rendering modern HTML/CSS means shipping a browser engine; an almost-right renderer produces output that looks plausible and is wrong. Use headless Chrome or Chromium, then process the resulting PDF with PDFluent. |
| `docx-export` | no | `to_docx` wiring. Currently compile-time reserved; 1.0 always compiles `to_docx` on non-wasm. |
| `xlsx-export` / `pptx-export` | no | Reserved for future `to_xlsx` / `to_pptx`. |
| `xfa-flatten` | no | XFA flatten runtime. |
| `wasm` | no | Marker feature for bindings that want to assert WASM intent. |
| `internal-legacy` | no | Reserved for legacy shim layer during binding migration. Do not enable. |
| `tracing` | no | Enables `tracing` spans on facade methods. See [OBSERVABILITY.md] (#1235). |

### 7.2 Feature-flag policy

- Removing a feature is **MAJOR**.
- Renaming a feature is **MAJOR** unless a deprecated alias is added
  that forwards for at least one MINOR.
- Changing a feature's default (on ↔ off) is **MAJOR**.
- Adding a feature is **MINOR**.
- A feature that gates compile-time availability of a method must
  document what happens when disabled (the method typically ceases to
  exist — a MAJOR would be required to change this to a run-time
  error, or vice versa).

### 7.3 Default features

The default set (`signing`, `pdfa`, `redaction`) is chosen so that
"depend on `pdfluent` with defaults" gives you the common enterprise
feature set. OCR / office conversion / HTML-to-PDF are opt-in because
they pull in heavy deps.

---

## 8. Error-code contract

- Every `Error` variant has a stable `code() -> &'static str` of the
  form `E-<CATEGORY>-<SPECIFIC>`. The string is **frozen for 1.x**.
- Every variant has a stable `docs_url() -> &'static str` of the form
  `https://pdfluent.com/errors/<code>`. The prefix and the per-variant
  suffix are both frozen for 1.x.
- Snapshot-tested in `tests/error_codes_stable.rs`. A change to any
  frozen code fails CI.
- Adding new variants is MINOR (behind `#[non_exhaustive]`).

The `Error::source()` chain is also part of the contract:

- Only `Error::Io` chains to `std::io::Error` (a public std type —
  allowed).
- Every `From<internal_crate::Error>` path produces a variant with
  `source() == None`, so internal error types never leak via
  `std::error::Error::source()`.

---

## 9. MSRV

- **1.0 MSRV:** Rust 1.80.0 (workspace-level `rust-version`).
- MSRV raises follow §2.1 / §2.2: more than one stable release
  requires MAJOR, exactly one release is MINOR with 6 months' notice.
- MSRV is tested in CI against the declared version and against
  `stable` and `beta`.

---

## 10. Platform support matrix

See [`WASM_SUPPORT.md`](WASM_SUPPORT.md) for the wasm32 matrix and
per-method availability (#1233).

Native targets supported at 1.0:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Dropping any of these platforms is MAJOR.

---

## 11. Release cadence

- **MAJOR:** at most once per 12 months, with an RFC and a MIGRATION
  guide (see [`MIGRATION.md`](MIGRATION.md), #1241).
- **MINOR:** target every 6–8 weeks, driven by features and
  truth-gap promotions.
- **PATCH:** as needed for bug fixes; no fixed cadence.

---

## 12. Deprecation policy

- A Stable item may be marked `#[deprecated]` in any MINOR.
- A deprecated item stays compilable for at least **two MINOR
  releases** before removal in a MAJOR.
- The deprecation attribute must carry a `since = "1.x"` tag and a
  `note = "..."` pointing to the replacement.
- A deprecated feature flag stays accepted for the same duration; its
  default (on/off) may not change while deprecated.

---

## 13. Amendment process

This document is the operational version of the stability contract.
To change it:

- **Editorial** (typo, clarification that doesn't shift meaning): PR
  with a single reviewer approval.
- **Substantive** (loosening or tightening a rule): RFC amendment,
  referenced from this document's §4 pointer.

The document's effective date is tracked in its header. Historical
versions are reachable via git history.

---

## Appendix A — Relation to Slag 1

The 1.0 stability surface was frozen in Slag 1 via:

- RFC 0001 v1.3 (API freeze)
- PRs #1261 (capability enforcement), #1262 (error system), #1264
  (form mutation), #1269 (parity methods), #1270 (DX consolidation)

No Stable item listed in §3.1 is unimplemented as of master
`06f906c6c`. The Deferred items in §3.3 are listed verbatim in this
file and covered by `Error::MissingDependency` returns, not panics.
