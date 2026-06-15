# Changelog — pdfluent

All notable changes to the `pdfluent` crate are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased] — sdk/api-contract-alignment — 2026-06-15

### Changed

- **Canonical cross-language AcroForm API names.** The same operation now has
  one name in every binding (camelCase in Node/Java/WASM, snake_case in
  Python): `getFormFields` / `get_form_fields`, `setFormField` /
  `set_form_field`, `setMultiSelect` / `set_multi_select`. This reconciles the
  three pre-`beta.14` inconsistencies (Node `formFields`/`setFieldValue`, Python
  `set_form_field_multi`). See `docs/acroform-support-contract.md` §6.
- **Coordinated release version → `1.0.0-beta.14`.** The channels had drifted to
  different published versions (crates.io `pdf-engine`/`pdfluent-forms` at
  `beta.9`; PyPI at `b10`; npm `@pdfluent/node` at `beta.10`; npm
  `@pdfluent/sdk-wasm` at `beta.13`). `beta.14` is the first version free on
  every channel (crates.io, npm, PyPI, Maven), so all product crates and binding
  package manifests are aligned to it for the next coordinated release.

### Fixed

- **Node build no longer drops the typed-error layer.** `napi build` regenerates
  `index.js` / `index.d.ts` and used to wipe the hand-maintained
  `PdfluentError` / `PdfluentLicenseError` classes and license wrappers. They now
  live in `scripts/build/typed-error-layer.{js,d.ts}` and are re-applied by an
  explicit, idempotent postbuild step (`scripts/build/postbuild.cjs`), guarded by
  `tests/typed_error_layer.test.js`.
- **Node ESM entry (`index.mjs`) fixed** to re-export exactly the real CJS
  surface — dropped six phantom error classes (`PdfluentIoError` etc.) that were
  `undefined`, and added the missing `setLicensePublicKey` / `setLicensePayload`.

### Deprecated

- Node `formFields()` → use `getFormFields()`; Node `setFieldValue()` → use
  `setFormField()`; Python `set_form_field_multi()` → use `set_multi_select()`.
  The old names still work as forwarding aliases through the beta line and are
  removed in `1.0.0`.

## [Unreleased] — acroform/sdk-closure — 2026-06-13

### Added

- **`PdfFormMut::set_multi_select(name, &[&str])`** — fill a multi-select list
  box. Writes `/V` as an array of text strings and rebuilds `/I` (the sorted
  selected-index cache) to match Adobe Acrobat. Pass an empty slice to clear.
  Completes AcroForm field-type coverage on the facade (the last unreachable
  type, previously only callable via `pdf_forms::apply_choice_multi`).
- Multi-select fill is now reachable from **every** language binding:
  `setMultiSelect` (Node, Java, WASM `PdfDocMut`) and `set_form_field_multi`
  (Python).
- **Canonical Java binding (`com.pdfluent.PdfluentDocument`) gains AcroForm
  support**: `getFormFields()`, `setFormField()`, `setMultiSelect()`, `save()`,
  and a `FormField` value class. These JNI entry points existed but were never
  declared on the canonical class; Java form fill is now wired end-to-end
  (fill → save → reopen verified via JUnit).

### Documentation

- New [`docs/acroform-support-contract.md`](../../docs/acroform-support-contract.md)
  — the exact AcroForm support contract (field types, writeback behaviour,
  guarantees, exclusions, per-surface availability).

---

## [Unreleased] — xfa/sdk-phase1-fill-foundation — 2026-06-12

### Added — XFA fill API (Phase 1)

- **`PdfDocument::has_xfa_form() -> bool`** — cheap detection of an active
  `/AcroForm /XFA` template.
- **`PdfDocument::xfa_form_model() -> Result<XfaFormModel>`** — enumerate the
  currently layouted XFA fields: fully-qualified names + SOM paths, typed
  kinds (text/checkbox/radio-group/dropdown/date/numeric/…), values,
  read-only (template + saved-state `access` locks), required, multiline,
  hidden, choice options, checkbox on/off values, per-widget page mapping
  and geometry (XFA layout space), and data-binding state. Reports the XFA
  layout page count (a dynamic form's real page count, not the 1-page
  viewer-shell of the PDF container). The underlying parse/merge/layout
  session is built once per handle and cached.
- **`PdfDocument::set_xfa_field_value(name, XfaFieldValue) -> Result<XfaSetOutcome>`**
  — fill a field: updates the form tree, writes through to the bound
  `datasets` node (created on demand for default-bound fields), keeps an
  Adobe-saved form packet's values in sync (including radio-member
  semantics), and swaps the updated packets into the PDF. A subsequent
  `save()`/`to_bytes()` yields a PDF whose values Adobe Acrobat/Reader
  reads back from the datasets packet. Datasets writeback is a surgical
  XML splice (original packet preserved byte-for-byte outside the changed
  values) with a wrapper-preserving regeneration fallback. Read-only
  fields are rejected. New public DTOs in `pdfluent::xfa`:
  `XfaFormModel`, `XfaField`, `XfaFieldType`, `XfaFieldValue`,
  `XfaFieldOption`, `XfaWidget`, `XfaRect`, `XfaSetOutcome`.
- Both methods are gated on the existing `XfaParse` / `XfaFill`
  capabilities (Developer tier and up).

### Phase-1 scope notes

- No change/click/enter/exit event scripts run on value writes, the layout
  is not re-flowed after writes, and instanceManager add/remove is not
  exposed. `bind="none"` field values persist only to the saved form
  packet (matching Adobe), reported via
  `XfaSetOutcome::persisted_to_datasets == false`.
- Bindings follow-up: the XFA fill API is Rust-only in this phase —
  C-API/Python/Node/Java/WASM wiring is tracked as follow-up work.

## [Unreleased] — acroform/sdk-foundation — 2026-06-12

### Added

- **`PdfDocument::form_model() -> Result<Vec<FormFieldModel>>`** — inspect form
  structure before writing. Returns one `FormFieldModel` per logical field with
  typed kind (`Text{multiline, comb, password}`, `Checkbox{on_state, checked}`,
  `RadioGroup{options}`, `ComboBox`, `ListBox`, `PushButton`, `Signature`),
  kind-specific data, per-page widget rectangles, current/default values,
  read-only/required flags, `/MaxLen`, quadding, and resolved `/DA` font info.
  Returns empty `Vec` for documents without an AcroForm.

- **`PdfDocument::regenerate_form_appearances() -> Result<WriteOutcome>`** —
  materialises trustworthy `/AP /N` streams for every filled text and choice
  field; useful for PDFs filled by tools that only wrote `/V` + `/NeedAppearances`.

- **`PdfDocument::sync_engine()`** — re-parses the in-memory lopdf document and
  rebuilds the rendering engine so mutations are visible in subsequent
  `render_page()` calls on the same handle.

### Changed

- **`form_mut()` writeback chain unified.** All four setters (`set_text`,
  `set_checkbox`, `set_radio`, `set_dropdown`) now route through a single
  `apply_field_value` implementation in the `pdfluent-forms` crate. Every call
  updates `/V`, per-widget `/AS`, and a regenerated `/AP` appearance stream in
  one atomic operation. Previous behaviour was partially implemented across
  several independent code paths.

- **Hierarchical field names** (`"parent.child"`) are resolved through `/Kids`
  recursion in all four setters. Previously only flat top-level names were
  reliably supported.

- **Appearance encoding**: text appearance streams use WinAnsiEncoding with
  embedded Standard-14 AFM widths. Non-WinAnsi values (Cyrillic, CJK, etc.)
  fall back to `/NeedAppearances true` with the stale `/AP` removed.

- **Inline `/AcroForm` promotion**: AcroForm dictionaries stored inline in the
  document catalog are promoted to indirect objects before any mutation, matching
  how Adobe-generated forms are stored and ensuring compatibility with
  conforming readers.

- **Read-only enforcement**: fields with the `ReadOnly` flag set are rejected at
  set-time with `WritebackError::ReadOnly`, stricter than pdfium/mupdf which
  only enforce in the UI layer.

---

## [1.0.0-beta.4] — 2026-05-05

### Documentation

- Removed the "A note on prior versions" banner from the crate README.
  The yanked beta.1/beta.2 licensing correction is documented in this
  changelog (see `1.0.0-beta.3` entry) and does not need to be surfaced
  on every crates.io visit indefinitely.
- Fixed all 9 broken intra-doc links in `pdfluent` (paths in document.rs,
  encrypt.rs, error.rs). `cargo doc -p pdfluent` is now warning-free.
- Documented the **determinism contract** explicitly on `SaveOptions` and
  in `tests/determinism.rs`: byte-deterministic output for unencrypted
  documents under unchanged input/options. Encryption (random IVs) and
  caller-introduced timestamps remain non-deterministic by design.

### Tests

- New integration test file `tests/determinism.rs` with 4 tests:
  - `determinism_to_bytes_is_idempotent`
  - `determinism_multi_open_roundtrip`
  - `determinism_write_to_in_memory_cursor` (CI-safe, in-memory)
  - `determinism_to_bytes_matches_save_with` (disk-vs-memory parity)
  All passing on `1.0.0-beta.3` codebase as of 2026-05-03.

### Internal — workspace lints

Enabled `#![warn(missing_docs)]` across 14 commercial crates that did not
have it (the `pdfluent` facade already had it). Two crates
(`pdfluent-sign`, `pdf-compliance`) ship `#![deny(missing_docs)]` — no
change. `pdf-engine` upgraded from `#![allow]` → `#![warn]`. The
upstream-fork crates `pdf-render` and `pdf-font` still inherit Hayro's
permissive doc policy and are not yet at warn — left for a future pass.

This produces a baseline of ~895 doc-coverage warnings to address
incrementally; no compile errors. Customer-facing surface
(`pdfluent::*`) remains 0-warning.

### Public API

- No breaking changes vs `1.0.0-beta.3`. The audit (#1383) confirmed the
  facade is well-curated: 233 public items across 18 modules, no
  `#[doc(hidden)]` leakage, prelude is selective. No renames, no removals.

---

## [1.0.0-beta.3] — 2026-05-03 (license model correction)

### Status

- Beta software — public API surface is stabilizing for 1.0.
- Not all features are fully complete; capability-gated via Cargo features.
- **XFA support is experimental and under active development.** Visual
  fidelity and feature coverage are improving steadily but not yet
  recommended for production XFA workflows.
- Other features (PDF parse/save, AcroForms, signatures, PDF/A,
  redaction, text extraction, rendering) have completed their quality
  gates and are production-grade.


### Changed — BREAKING (LICENSE)

- **Licensing model updated to `PDFluent Commercial License`.** The
  `pdfluent` crate and the proprietary engine, manipulation, signing,
  compliance, redaction, extraction, forms, annotation, conversion, render,
  font, and XFA crates that it depends on now ship under a custom commercial
  license. Free for evaluation; production use requires a valid commercial
  license. See the `LICENSE` file at the crate root and `pdfluent.com/terms`.
- **Open-source foundation crates remain MIT or MIT/Apache-2.0:**
  `pdfluent-lopdf`, `pdfluent-cff`, `pdfluent-ccitt`, `pdfluent-jbig2`,
  `pdfluent-jpeg2000`, `pdf-syntax`, `pdf-interpret`. These are forks of
  upstream open-source libraries and may continue to be used under those
  terms.
- Per-crate license matrix is documented in the repository-root `NOTICE`
  file.

### Note on prior releases

**Earlier beta versions were published under permissive licenses and have
been yanked.** Specifically, `1.0.0-beta.1` and `1.0.0-beta.2` of `pdfluent`
(and the corresponding beta releases of the other commercial crates listed
above) were briefly published on crates.io under MIT or MIT-OR-Apache-2.0
before the licensing model was finalized. All 21 such versions across 19
commercial crates were yanked on 2026-05-02 to prevent new installations.

Crates.io versions are immutable: anyone who downloaded those versions
before the yank holds an MIT-licensed copy of those specific bytes
permanently. From `1.0.0-beta.3` onward, all production use of the
proprietary crates requires a valid commercial license under the terms of
the `LICENSE` file in this crate.

This release contains **no functional changes** versus the yanked beta.2 —
this is a license-correction release only.

---

## [1.0.0-beta.2] — 2026-05-02

### Changed
- Crate metadata only — no API or behavior changes.
- `repository`, `homepage`, `documentation` URLs corrected to point at
  `pdfluent.com` and the PDFluent GitHub organization.
- Keywords and categories aligned with PDF SDK discoverability conventions.

---

## [1.0.0-beta.1] — 2026-05-02

### Added
- `PdfDocument::open` / `save` / `save_to` — full read+write round-trip
- Page operations: rotate, extract, split, merge via `pdf-manip`
- Digital signatures: PAdES B-B / B-T / B-LT / B-LTA via `pdfluent-sign`
- PDF/A validation and conversion (PDF/A-1b, 2b, 3b) via `pdf-compliance`
- Content redaction (search-based + region-based) via `pdfluent-extract`
- Text extraction with ligature decomposition via `pdf-engine`
- Thumbnail and image rendering via `pdf-render` (native only)
- DOCX export via `pdf-docx` (native only)
- Evaluation mode: SDK fully functional without a licence; output stamped with
  `Producer: PDFluent (Unlicensed Evaluation — pdfluent.com/trial)` in PDF Info
  dict plus a one-time `stderr` warning on first use. No functionality is
  restricted. See `pdfluent.com/trial` to obtain a 30-day clean-trial key.
- Licence validation: offline Ed25519 signature verification; no network calls;
  works air-gapped. Licence file loaded from `PDFLUENT_LICENCE` env var or
  adjacent `pdfluent.licence.json`. Supported types: `trial` (30-day, expires)
  and `paid` (perpetual, `exp: null`).
- Stripe-backed purchase flow: perpetual licences (Lite / Plus / Professional /
  Unlimited) available at `pdfluent.com/pricing`; key delivered automatically
  after checkout via Cloudflare Worker webhook.
- `pdfluent::prelude::*` re-export for ergonomic imports
- Cargo features: `signing` (default), `pdfa` (default), `redaction` (default),
  `async-tokio`, `tracing`

### Known Limitations
- **Non-deterministic PDF output (#1308):** PDF byte streams may differ between
  runs due to non-deterministic object IDs or internal ordering. CI pipelines
  that compare file checksums will see spurious failures. Fix targeted for
  1.0.0-beta.2. Workaround: compare semantic content, not raw bytes.
- WASM target: `to_images` and `to_docx` are not available on
  `wasm32-unknown-unknown`; calls return `Error::UnsupportedOnWasm`.
- OCR, HTML-to-PDF, XLSX/PPTX export: behind feature flags, not wired in beta.

---

## [1.0.0-alpha.1] — 2026-04-02

Initial scaffold release. API surface frozen per RFC 0001. Method bodies wired
progressively; not suitable for production use.
