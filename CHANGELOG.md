# Changelog

All notable changes to PDFluent are documented here.

## [1.0.0] — 2026-08-23

General availability. Every binding channel moves to `1.0.0`; the crates.io
graph stays deliberately heterogeneous (upstream forks keep their `0.x` tracks),
which is the topologically-proven publish set rather than an inconsistency.

Four of the fixes below could not be caught by validating the output. A file can
lose most of its text and still pass PDF/A validation, because what is gone
cannot be wrong. They were found by measuring text retention as a second axis
against the source document.

### Fixed

- **A backslash in a page's text no longer costs everything after it.** The
  PDF/A implementation-limit pass located the end of a `(...)` string with the
  rule "the previous byte is not a backslash". An escape consumes exactly the
  byte after it, so in `(\\)` the backslash escapes itself and the `)` does
  close the string. Reading it the other way ran the scan to the end of the
  content stream, came out over the 32767-byte limit, and kept the first 32767
  bytes — discarding the rest of the page. Any document whose text contains a
  backslash was affected; pdfTeX writes one as `(\x00\\\\)`. Measured on a
  govdocs holdout document: page 3 fell from 67,786 to 36,668 bytes, and veraPDF
  called the result compliant.
- **Spaces no longer render as `)` on subset fonts.** A glyph with an empty
  outline was treated as one the subsetter had stripped, and replaced. A space
  *has* no outline. With `glyph_index(' ')` finding nothing in the subset, the
  replacement fell back to the lowest CID that did have an outline. `/ToUnicode`
  now separates the two cases.
- **Character codes below 32 survive where `/Differences` names them.** They are
  control characters only when the font says nothing about them; a TeX subset
  encoding starts its `/Differences` at code 1, so on a Computer Modern subset
  those are the document's ordinary letters.
- **`sign_pdf_incremental` works.** It failed on every input with "PDF has no
  pages": a fresh incremental revision starts with no objects, so the signing
  pass had no page tree to attach the widget to. The catalog and page tree are
  now carried over, and the previous revision survives byte for byte — which is
  the property that keeps signatures already in the file verifying.
- **A certified signature reports its own DocMDP level.** The reader looked up
  `/SigRef`; ISO 32000-1 Table 252 names the key `/Reference`, and `SigRef` is
  the `/Type` inside each entry. Every "may this edit be applied" decision
  downstream treated certified documents as uncertified. The same call feeds
  `/FieldMDP`, so form-field locks were invisible too.
- **The PNG Average predictor encoder can be undone by its own decoder.** RFC
  2083 §6.5 averages the left and above bytes in nine bits; adding them as `u8`
  wraps whenever the sum reaches 256.

### Changed

- `pdfluent-lopdf` 0.39.4 → 0.39.5 (the predictor fix above).
- Binding channels unified on `1.0.0`: `@pdfluent/node` and its six per-platform
  packages, `@pdfluent/sdk-wasm`, PyPI `pdfluent`, Maven `com.pdfluent:pdfluent`,
  NuGet `PDFluent`.

## [1.0.0-beta.17] — 2026-06-18

Repairs the OSS-fork dependency drift that blocked beta.16 from publishing to
crates.io (beta.16 was yanked after 13 sub-crates published but the facade hit
the wall). Functionally identical to beta.16 (the cross-binding parity closure
below) plus:

### Fixed

- **OSS forks republished with their actual APIs.** `pdf-syntax` (→ 0.5.6),
  `pdf-interpret` (→ 0.5.7) and `pdf-font` (→ 1.0.0-beta.5) carried local APIs
  (`Cache`, `LoadRecovery`, `OutlineFontData::ascent`, interpreter cache
  settings, load-recovery) that had never been published under their prior
  version numbers — so `pdf-engine` and the `pdfluent` facade could not build
  against the published forks. The forks are bumped so those APIs ship, and all
  product crates re-pin to the new fork versions.
- **`publish_ordered.sh`**: added the missing `xfa-js-sandboxed` crate, fixed
  the topological order (`pdf-engine` was listed before its dependencies
  `pdf-xfa` / `pdfluent-forms`), dropped `--allow-dirty`, and made the negative
  array index bash-3.2 compatible.
- **Node npm packaging hotfix → `@pdfluent/node@1.0.0-beta.17.1`.** The
  `1.0.0-beta.17` main meta-package shipped with only 3 of 6 platform
  `optionalDependencies`: the `prepublishOnly: napi prepublish` lifecycle hook
  rewrote `package.json` during `npm publish`. Recovered as a packaging-only
  **beta.17.1** (same native build) pinning the six already-correct
  `1.0.0-beta.17` platform packages; the broken `beta.17` main is deprecated.
- **Hardened, deterministic Node release process** (`crates/pdf-node/scripts/release/`):
  single-source-of-truth platform matrix; deterministic `npm/` generation;
  a **non-mutating** read-only prepublish guard replacing `napi prepublish`;
  actual-tarball validation (full 6-platform matrix, no native binary in the
  meta-package, LICENSE/README, no leakage); idempotent `--ignore-scripts`
  publishing; regression tests. `*.node` removed from the main `files` so the
  meta-package is binary-free by construction.

### Changed

- Product line re-versioned beta.16 → **beta.17** (beta.16 product versions are
  spent/yanked). Independent OSS image-codec forks unchanged.

## [1.0.0-beta.16] — 2026-06-18

Coherent release train across all product channels. Closes the remaining
commercial-license/signing parity gaps so every language binding exposes the
same capability surface, and ships the SDK features accumulated since the last
published binding release (beta.10).

### Added — cross-binding commercial-license & signing parity

- **Java** (`com.pdfluent:pdfluent`, canonical `bindings/java`): license
  public-key injection (`PdfluentLicensing.setPublicKey`), signed-payload
  activation (`PdfluentLicensing.activatePayload`), document signing
  (`PdfDocument.sign`), and signature inspection (`signatureCount`,
  `isSignatureValid`, `verifySignatures`) — all bound to the C ABI with typed
  `PdfluentException` mapping. (Previously these existed only in the
  deploy-disabled legacy `xfa-pdf` artifact.)
- **Python** (`pdfluent`): signature verification on the document
  (`validate_signatures`/`verify_signatures`/`signatures` returning
  `SignatureResult`); license `set_license_public_key`/`set_license_payload`
  surfaced at the top-level package and fully declared in the `.pyi` stubs.
- **.NET** (`PDFluent`): `Licensing.SetPublicKey`/`ActivatePayload`,
  `PdfDocument.Sign`/`SignatureCount`/`IsSignatureValid`/`VerifySignatures`
  with the typed `PdfluentException` hierarchy preserved; the previously
  skipped signature-verification test is now implemented.
- **WASM** (`@pdfluent/sdk-wasm`): `setLicensePublicKey`/`setLicensePayload`
  exports; every editing/annotation error now uses the typed error model
  (code/operation/help) instead of ad-hoc `JsError` (61 sites converted).
- **Node** (`@pdfluent/node`): already parity-complete — the reference surface.

### Fixed

- **wasm32 build of the `pdfluent` facade**: diagnostics now import the
  leniency/interpreter-warning types directly from `pdf-syntax`/`pdf-interpret`
  (lightweight, wasm-safe) instead of through the native-only `pdf-render`
  re-export, which broke `cargo build --target wasm32-unknown-unknown` since
  the Phase-1 fill foundation landed.

### Changed

- **`xfa-cli` is now `publish = false`** — it bundles internal research
  binaries; the user-facing `pdfluent` CLI is distributed as signed platform
  binaries, not as crates.io source.
- Independent OSS-fork crates (`pdf-syntax`, `pdf-interpret`, `pdf-font`,
  `pdfluent-lopdf`, and the image-codec forks) keep their own semver and are
  **not** rebumped — already current and published on crates.io.

### Included since the last published binding release (beta.10)

AcroForm multi-select on every binding; a public diagnostics model
(decode-leniency + interpreter-warning collection); tagged-PDF logical
structure tree; `/PageLabels` reading; public incremental-save; idempotent
annotation flattening; XFA interactive-fill foundation; and an expanded
text/font-metrics surface (glyph transforms, ascent/descent, Standard-14 AFM
fallback).

## [acroform/sdk-foundation] — 2026-06-12

### Added

- **`PdfDocument::form_model()`** (`pdfluent`): returns one `FormFieldModel` per
  logical field — typed kind (text/checkbox/radio-group/combo/listbox) with
  kind-specific data (comb/multiline/password flags, on-state names per widget,
  choice options), per-page widget rectangles, current and default values,
  read-only/required flags, `/MaxLen`, quadding, and resolved `/DA` font info.
  Fully-qualified names are accepted by all `form_mut()` setters without any
  additional lookups. Empty `Vec` for documents without an AcroForm (including
  XFA-only documents). Commits `d884fc649`, `aaf3e9d97`.

- **`PdfDocument::regenerate_form_appearances()`** (`pdfluent`): materialises
  trustworthy `/AP /N` appearance streams for every filled text and choice field
  in the document. Necessary for PDFs filled by tools that only write `/V` (often
  paired with `/NeedAppearances true`) — such documents display stale or empty
  values in viewers that do not regenerate appearances, including the SDK's own
  renderer. Call [`sync_engine`] afterwards to see the change in rendering on the
  same handle. Commit `aaf3e9d97`.

- **`PdfDocument::sync_engine()`** (`pdfluent`): re-parses the in-memory lopdf
  document and rebuilds the rendering engine, so mutations applied through
  `form_mut()` or `regenerate_form_appearances()` are visible in subsequent
  `render_page()` calls on the same handle. Commit `aaf3e9d97`.

- **`pdf_forms::apply_field_value`**: single writeback chain owned by the
  `pdfluent-forms` crate. Handles text, checkbox, radio, and choice fields;
  updates `/V`, per-widget `/AS`, and regenerated `/AP` streams in one call.
  UTF-16BE+BOM encoding for non-ASCII, WinAnsiEncoding-aware appearance streams
  with embedded Standard-14 AFM widths, comb/multiline/quadding support.
  Previously there were seven independent writeback paths across the SDK, all
  incomplete. Commit `d884fc649`.

- **`pdf_forms::regenerate_appearances`**: stand-alone function that regenerates
  all appearance streams in a `lopdf::Document`; used by
  `regenerate_form_appearances()` above. Commit `d884fc649`.

- **`pdf_forms::build_form_model`**: builds the `Vec<FormFieldModel>` from a
  parsed `FieldTree`. Used by `PdfDocument::form_model()`. Commit `d884fc649`.

- **`pdf_forms::WriteValue`**, **`pdf_forms::WriteOutcome`**,
  **`pdf_forms::WritebackError`**: public types accompanying `apply_field_value`
  for callers that need typed writeback. Commit `d884fc649`.

- **Renderer `/AP /N` substate fix** (`pdf-interpret`): widget annotation
  appearance streams whose `/AP /N` is a sub-state dictionary (radio buttons,
  checkboxes) are now rendered by selecting the entry whose name matches `/AS`.
  Previously, dictionary-valued `/AP /N` entries were silently dropped and both
  states appeared blank. Includes byte-exact matching, a pdfium `/V`→Parent-`/V`
  fallback, and a zero-area bounding-box guard. Commit `ef122d4c7`.

### Changed

- **All seven SDK writeback surfaces rewired** to `apply_field_value`:
  `pdfluent::PdfFormMut`, `xfa-cli fill`, `pdf-node setFieldValue`,
  `pdf-python set_form_field`, `pdf-java setFormField`, and both
  `xfa-wasm PdfDoc.setFormField` / `PdfDocMut.setFormField`.
  Behaviour improvements shared by all surfaces:
  - Hierarchical field names (`parent.kid`) resolved through `/Kids` recursion
    (previously top-level-only on most surfaces).
  - `/AP` appearance stream regenerated; field value is now visible in all
    viewers including the SDK's own renderer without needing viewer-side
    `/NeedAppearances` processing.
  - `/AS` kept consistent per widget (mupdf `set_check_grp` rule).
  - Text encoding: ASCII → PDF literal, non-ASCII → UTF-16BE+BOM whole string;
    `pdf-python` and `pdf-java` previously wrote a misspelled
    `/NeedsAppearances` key via an inline-dict-only path that silently did
    nothing.
  - Inline `/AcroForm` dictionaries (92% of the hybrid corpus are LiveCycle
    static shells with inline AcroForms) promoted to indirect objects before
    mutation.
  Commits `aaf3e9d97`, `d884fc649`.

- **`rotate_page` and `set_outlines`** (`pdfluent::PdfDocument`) now call
  `refresh_from_lopdf()` after mutating the lopdf layer, keeping the rendering
  engine in sync. Previously the engine was left stale, causing subsequent
  renders to show the pre-mutation state. Commit `a39de08bb`.

### Notes

- **No breaking API changes.** All new methods are additive. The writeback
  behaviour change is a correctness fix; the only semantic tightening is that
  **read-only fields are now rejected at set-time** (`WritebackError::ReadOnly`)
  rather than written through silently. pdfium and mupdf enforce read-only in
  their UI layers only; our SDK has no UI layer, so the set-time check is the
  only place the constraint can be enforced.

- **Non-WinAnsi values** (Cyrillic, CJK, …): `/V` is always lossless via
  UTF-16BE+BOM; appearance generation falls back to `/NeedAppearances true`
  (stale `/AP` removed). The SDK renderer does not honour `/NeedAppearances` —
  call `regenerate_form_appearances()` after opening such documents for correct
  rendering.

- **Widget rotation** (`/MK /R`): not yet applied in generated appearances.
  `flatten_forms` (deferred to post-freeze) will handle full widget rendering
  with rotation.

## [@pdfluent/sdk-wasm@1.0.0-beta.11] — 2026-05-16

### Changed

- **Build hygiene — no API change.** Rebuilt `@pdfluent/sdk-wasm` with path
  remapping active (`--remap-path-prefix` in `.cargo/config.toml`) so that
  dependency source paths from the build machine are replaced by neutral
  prefixes (`/registry`, `/git`, `/src`) in the published `.wasm` binary.
  Beta.10 contained 493 embedded private filesystem paths
  (`/Users/<developer>/.cargo/registry/...`); beta.11 contains zero.

- **wasm-opt -O3 enabled** (B3). Cold init -13.3%, median key-op -7.9%,
  raw `.wasm` -7.0%. Wire size slightly larger (+3% gzip) as expected for
  speed-optimised builds.

- **Runtime path fix** (`pdf-manip`). `CMAP_SEARCH_DIRS` and
  `PREDEFINED_CMAP_SEARCH_DIRS` now exclude the `CARGO_MANIFEST_DIR`-based
  entry when building for `target_arch = "wasm32"`. That entry is a
  development-local path unreachable at WASM runtime and was the last
  remaining private-path string in the binary.

Deprecation notice: `@pdfluent/sdk-wasm@1.0.0-beta.10` is deprecated; see
npm registry for the deprecation message.

## [1.0.0-beta.8] — 2026-05-27

Workspace-wide line bump aligning every first-party crate to `1.0.0-beta.8`. Default flatten behaviour is
**byte-identical** to beta.5: all new runtime capabilities below are **default-off / sandboxed-only**
(opt-in via `XFA_JS_EXECUTION_MODE=sandboxed` and feature `xfa-js-sandboxed`); the shipping default binary
contains no rquickjs and never invokes them.

### Added

- **XFA runtime observability** (`pdf-xfa`, Epic A — feature `xfa-js-sandboxed`): six diagnostic trace
  fields surface dynamic-script behaviour when `XFA_FLATTEN_TRACE=1` and `XFA_RUNTIME_DIAG=1` are set —
  `script.lifecycle[]` (per-script outcome), `skipped_activities{}`, `som_fail_log[]`,
  `instance_write_log[]`, `presence_mutation_log[]`, and `form_dom_match_failures` (+ log). Trace schema
  bumped from 1.0 → 1.1. Default-OFF; flag-off output byte-identical. Commits `1a6992ac7`, `4242dad91`.
- **XFA SOM hardening — `$data` and `#items`** (`pdf-xfa`, Epic B — sandboxed-only): the implicit-global
  resolver now handles XFA §3.3.2 `$data` (data-DOM root) and §7.7 `#items` (choiceList substitute with
  `.nodes` collection), letting more dynamic scripts resolve correctly without throwing. Commit `dad9bf056`.
- **Benign absent-declared-node SOM façade** (`pdf-xfa`, sandboxed-only): a bare implicit identifier that
  fails the scope resolve but names a template-declared container (`subform`/`subformSet`/`exclGroup`/
  `area`) now resolves to a benign empty node (`isNull === true`, chainable, writes absorbed) instead of
  `undefined`. Adobe-aligned: guarded scripts like
  `if (!Sub.Child.Field.isNull) {…} else { … }` correctly take the empty branch instead of throwing
  `TypeError: cannot read property '…' of undefined`. Gated host-side by `is_declared_absent_node`
  (template container-name set). Undeclared names still surface as `undefined` (D-θ.1 contract preserved).
  Commit `31b5d4b94`, merge `bedf357df`.
- **`XFA_JS_HARVEST_MODE`** (`pdf-xfa`, default-off, sandboxed-only): when set, the §4.3
  `data_empty_dropped` page-suppression decides on a **pre-JS** data-bound emptiness snapshot rather than
  the post-JS live tree, preserving the static data-empty drop under the sandboxed runtime so JS field
  population (`#items` list writes + value mutations) cannot keep an otherwise data-empty page alive.
  Net: the full sandboxed runtime path now corpus-wide matches the static default and unlocks parity on
  the `2ff85101` wall doc (9 → 4 = oracle). Commit `df92232a8` (renamed `bebf4f0c3`), merge `99f31f52b`.

### Changed

- **`flatten` rejects non-PDF input** (`pdf-xfa` / `xfa-cli`): non-PDF bytes now return
  `Error::NotPdf` immediately instead of being silently passed through. Commit `11a22c2ac`, merge
  `e0810724e`.
- **CLI User-Agent neutralised** (`xfa-cli`): the collector no longer references a retired GitHub repo
  URL; the User-Agent is now neutral. Commits `6cd623154`, `0834456f9`.

### Fixed

- **`pdfluent_cli_package.sh` SHA256SUMS generation**: deterministic checksum file for staged release
  binaries. Commit `a392c0f68`.
- **CI: cargo-deny self-installs the tool** when the runner is missing it, restoring the license gate
  that had silently dropped. Commit `4c7cf8b70`.
- **CI: transient-infra auto-retry** (~1 s fast-fails from runner/API rate-limit no longer mark the
  pipeline red; `script_failure` excluded so genuine failures still fail fast). Commit `f966664e9`.

### Notes

- BE-1 runtime parity engine — convergent verdict: the merged foundations (observability, SOM hardening,
  benign façade, harvest-mode) give the sandboxed runtime path **static-default parity** corpus-wide and
  resolve `2ff85101` (9 → 4 sandboxed). The "B-default-on" prize (making `XFA_SUPPRESSION_TRUST_LAYOUT`
  the default, 99.3% trustworthy) is documented as **not technically achievable** as a regression-safe
  default for the current corpus — page-local indistinguishability between legitimate keeps and
  over-keeps is fundamental across suppression / presence / instanceManager / saved-form-DOM. See
  `benchmarks/runs/xfa_enterprise_plan/_orchestration/BE1_RUNTIME_PARITY_ENGINE_REPORT.md`.

## [1.0.0-beta.5] — 2026-05-07

### Security

- **LOPDF-ZBOMB-01** — `pdfluent-lopdf`: FlateDecode and LZWDecode decompression is now capped at 256 MiB per stream. Crafted zip-bomb PDFs that previously caused unbounded memory growth now return `Error::StreamTooLarge` at the cap. Commit `c9c7110`.
- **JBIG2-HUF-01** — `pdfluent-jbig2`: Over-committed Huffman prefix trees (crafted JBIG2 streams where two length-1 codes fill the binary tree) previously caused an unreachable `panic!` in `set_child`. The path now returns `DecodeError::Huffman(HuffmanError::MalformedTable)` instead. Commit `9393410`.
- **J2K-BUF-01** — `pdfluent-jpeg2000`: `Image::decode()` buffer-size arithmetic is now guarded with checked multiplication. Images with extreme dimensions that overflow `usize` return `DecodeError::Validation(ValidationError::ImageTooLarge)` instead of allocating an incorrect buffer. Commit `10b1bd6`.
- **JPX-01/02/03** — `pdf-syntax`: Three integer-overflow paths in the JPX inline-image decoder are hardened. Commit `8174006`.
- **PDFA-CS-DOS-01** — `pdf-manip`: PDF/A colour-space conversion no longer iterates a synthetic `1..=max_id` range but walks the live xref table, preventing a DoS on PDFs with sparse xref entries. Commit `97e623d`.

### Added

- **XFA DataDom / instanceManager / listbox** (`pdf-xfa`): Full M3-B Phase D integration — `instanceManager` API, listbox `boundItem` wiring, and `DataDom` traversal for dynamic XFA forms. Commit `7001108`.
- **XFA form-level globals** (`pdf-xfa`): `<variables>` and `<script>` globals defined at the form level are now persisted across page renders. Commit `668994d`.
- **XFA SOM disambiguation** (`pdf-xfa`): Ambiguous multi-segment SOM paths are now resolved deterministically; implicit and explicit form-node lookups use a consistent resolution strategy. Commits `f918276`, `8f3d153`, `f91827607`.

### Changed

- `pdf-annot` builder refactored into focused per-type modules; `StampName` variants unified. External API is unchanged. Commit `c03b89466`.
- CI: GitHub Actions migrated to Node 24; continue-on-error audit + release smoke pipeline hardened. Commit `64b8d76`.
- `pdf-sign`: signing is now fail-closed on certificate-chain errors; partial signatures are rejected rather than silently succeeding.

### Fixed

- `pdf-xfa`: `xfa.event.newText` and `boundItem` listbox lookup corrected. Commit `11ad7d2`.
- `pdf-xfa`: Underscore shorthand handler ordering fixed (M3-B Phase D-δ.1). Commit `8efdad4`.
- `pdf-xfa`: Flatten JS chain fix applied to best-effort static flatten path. Commit `f91827607`.

---

## [Unreleased] — 2026-04-23

### Added

- feat(pdf-xfa): feature-gated JavaScript runtime adapter skeleton (`xfa-js-sandboxed`, default off) for M3-B Phase B — opt-in `JsExecutionMode::SandboxedRuntime`, rquickjs backend, no host bindings yet
- `/ActualText` extraction from BDC marked-content in `pdf-extract`; `TextBlock.actual_text: Option<String>` field — #1313
- Ligature decomposition in `pdf-extract` text extraction (FB00–FB06 + `st` / `ct`) with NFKD fallback inside FB00–FB4F — #1314

### Changed

- **[BEHAVIOR]** `pdf-extract` now decomposes ligature glyphs to constituent characters by default (`fi` → `fi`, `ffi` → `ffi`, etc.). Extracted text from PDFs with ligature-enabled fonts will read `office` instead of `o\u{FB03}ce`. Decomposition is toggled via an internal `LIGATURE_DECOMP` constant (default ON); public API is unchanged. `PositionedChar` bounding boxes for decomposed glyphs are split proportionally within the original glyph footprint — #1314

### Fixed

- fix(xfa-layout): suppress trailing empty pageArea continuation (M2 / C2a)
- fix(pdf-xfa): best-effort static flatten when JavaScript is present (M2 / CJS1)

## [Unreleased] — 2026-04-22

### Added

- `PadesProfile` enum and `SignerConfig` struct in `pdf-sign` — #1295
- `infer_pades_profile`: B-B / B-T / B-LT / B-LTA auto-selected from `tsa_url` + `enable_ltv` — #1295
- `Pkcs12Signer::with_config` builder + `effective_pades_profile()` — #1295
- Native AES-128 encryption in `pdf-manip` (V=4, R=4, CFM AESV2) — #1296

### Changed

- **[BEHAVIOR]** `Pkcs12Signer::effective_pades_profile()` now infers the correct PAdES level from the attached `SignerConfig`. Previously there was no inference and callers selected the signing entry point manually. Callers with an explicit `profile` field are unaffected — explicit always wins — #1295
- **[BREAKING]** `EncryptionAlgorithm::Aes128` now produces genuine AES-128 (V=4, R=4, CFM AESV2) instead of silently upgrading to AES-256. PDFs encrypted with `Aes128` will now correctly identify as AES-128 in conforming readers. Callers that relied on the silent upgrade to AES-256 must switch to `EncryptionAlgorithm::Aes256` explicitly — #1296

## [Unreleased] — 2026-04-02

### Added

- Canvas2D vector renderer (`renderPageToCanvasVector`) — #589
- License system met `PDFLUENT_LICENSE_KEY` env var + watermark — #614
- Elm-style error messages in CLI — #618
- 10 WASM playground demos — #584
- AI docs assistant op pdfluent.com — #620
- Shell completions + man pages in CLI — #613
- `pdfluent doctor` command — #613
- 15 XFA PDFs toegevoegd aan golden corpus — #594
- GA integration tests (8 tests) — nieuw
- Soak test scripts — #606

### Fixed

- XFA auto-flatten in `render_page` (elimineert AcroForm fallback) — #588
- Text extraction custom font encoding — #586
- Font widths + subsets PDF/A compliance — #631, #632
- Undefined operators in annotation streams — #624
- JPEG2000 EnumCS patching — #623
- DeviceCMYK OutputIntent — #625
- XMP rebuild + date sync — #629
- `lopdf` stream `/Length` recalculation + xref EOL — #627
- CIDSystemInfo/CMap registry — #628
- Annotation `/AP` + `/CA` compliance — #626

### Changed

- Website volledig herpositioneerd als SDK
- Error context propagated naar WASM en C-API

### Milestone

- **50K corpus fix-run:** §6.2.11.x failures van 20K+ naar <350 residual — #598

---

## [1.0.0-beta.1] — 2026-03-19

First public beta release. Full-stack pure-Rust PDF SDK with PDF/A validation,
manipulation, digital signing, content extraction, and multi-language bindings.

### Breaking changes from 0.1.x

- `pdf-compliance`: `validate()` now returns a `ComplianceReport` with structured
  `issues` instead of a flat `Vec<String>`. Use `report.compliant` for pass/fail.
- `pdf-engine`: `PdfDocument::extract_text()` returns `Vec<PageText>` instead of
  a single `String`. Use `.iter().map(|p| &p.text).collect::<Vec<_>>().join("\n")`.
- `pdf-sign`: `sign_pdf()` signature changed — now takes `SignOptions` struct.
- Workspace crates bumped from `0.1.0` to `1.0.0-beta.1`.

### Known limitations

- Memory usage can spike above 1 GB on adversarial/malformed large PDFs (#499).
- WASM build support is experimental; `@pdfluent/sdk-wasm` (previously
  `@pdfluent/wasm`) compiles but browser integration is untested end-to-end.

---

## [0.x.x] — Ronde 1–4 (2025)

### Ronde 1: Fundament

Foundation of the pure-Rust PDF stack.

- `pdf-syntax`: zero-copy PDF parser based on the hayro fork; handles xref tables,
  cross-reference streams, linearized PDFs, and ObjStm object streams.
- `pdf-interpret`: content stream interpreter (path/text/graphics operators).
- `pdf-font`: TrueType, CFF, Type1, Type3 font parsing and metric extraction.
- `pdf-render`: rasterization via PDFium FFI bridge; page-to-PNG rendering.
- `cff-parser`: standalone CFF/OpenType introspection with charstring width extraction.
- `hayro-ccitt` / `hayro-jbig2` / `hayro-jpeg2000`: image codec crates.

### Ronde 2: Core Engine

Full-featured PDF document operations.

- `pdf-engine`: high-level `PdfDocument` API — open, render pages, extract text,
  inspect structure, page count, metadata.
- `pdf-forms`: AcroForm field reading and writing (`form_write`).
- `pdf-annot`: annotation creation, inspection, and removal.
- `pdf-sign`: PKCS#12 digital signing and signature verification.
- `lopdf` fork: extended with AES-256 encryption, lazy ObjStm loading, memory limits,
  and 20-byte xref entry enforcement.
- `pdf-manip`: page merging, splitting, content-stream rewriting.
- `pdf-extract`: positioned character extraction with bounding boxes.

### Ronde 3: Integratie & Compliance

PDF/A validation and PDF manipulation hardening.

- `pdf-compliance`: ISO 19005-1/2/3/4 (PDF/A-1/2/3/4) compliance checker.
  Initial coverage: file structure (§6.1), color spaces (§6.2), fonts (§6.3),
  annotations (§6.5), actions (§6.6), XMP metadata (§6.7).
- `pdf-manip`: font embedding pipeline — CFF width correction, TrueType encoding
  normalization, CIDSet repair, symbolic font width fixes, colorspace normalization.
- `pdf-redact`: text redaction with XObject recursion and spatial fallback.
- `pdf-ocr`: PaddleOCR-based OCR backend for scanned PDFs.
- `xfa-test-runner`: corpus test runner with 24 test categories, SQLite results DB,
  veraPDF oracle integration, and per-PDF memory instrumentation.

### Ronde 4: API & Bindings

Multi-language SDK surface for external consumers.

- `pdf-python`: PyO3-based Python package (`pdfluent` on PyPI) with maturin build; covers
  open/save, merge, text extraction, form fields, annotations, redact, encrypt/decrypt,
  PDF/A validate.
- `pdf-node`: Node.js native addon (NAPI) with npm-ready `package.json`; covers the
  same API surface as the Python bindings.
- `pdf-java`: JNI wrapper with Javadoc, Maven-ready POM, JUnit test scaffolding.
- `pdf-capi`: C API (`pdf-capi`) with 12+ core operations for FFI consumers.
- `@pdfluent/sdk-wasm` (previously `@pdfluent/wasm`): Rust→WASM target (wasm-pack); basic parse and text extract in browser.
- CI matrix: Python (maturin), Node.js (napi-rs), Java (JNI) build verification.

---

## Compliance checker — detailed progress (§6.x coverage)

The PDF/A compliance checker (`pdf-compliance`) reached 100% pass rate on the
curated-4K corpus and ~98.5% on the 20K corpus as of the 1.0-beta release.
The following rules were implemented or significantly fixed during the beta cycle:

### File structure (§6.1)
- §6.1.2 — File header `%PDF-n.m` format validation.
- §6.1.3 — Trailer `/ID` presence, non-empty identifiers, linearized-PDF
  `/ID` consistency across all trailer dicts.
- §6.1.4 — xref header spacing (single-space vs double-space detection).
- §6.1.6 / §6.1.6.1 / §6.1.6.2 — Stream filter constraints (LZWDecode,
  JBIG2Decode); PDF/A-4 §6.1.6.1 remap.
- §6.1.7 / §6.1.7.1 — Stream keyword EOL, endstream EOL; name UTF-8 validity
  including names in colorspace arrays and nested Resources dicts.
- §6.1.8 / §6.1.9 — Object-syntax keyword spacing (PDF/A-1/4 vs PDF/A-2/3).
- §6.1.11 — Stream Length accuracy.
- §6.1.12 — PDF name length limit; raw-byte scan for long names; large /Kids arrays.
- §6.1.13 — Subnormal floats; CID values > 65535 in CMap streams.

### Color spaces (§6.2)
- §6.2.2 / §6.2.3.x — Device color space constraints, Default* CS detection,
  Form XObject inherited resource names.
- §6.2.4.2 / §6.2.4.3 / §6.2.4.4 — ICC profile constraints; OutputIntent ICC
  identity by content hash (ignoring Profile ID field); page-level OutputIntents.
- §6.2.5 — Halftone/TransferFunction constraints; Type5 indirect ref resolution;
  any-TF forbidden (not just non-Identity).
- §6.2.6 — Rendering intent validity; inline image rendering intents.
- §6.2.8.3 — JPEG2000 colorspace (`colr` box).
- §6.2.10.3.1 / §6.2.10.3.3 — CIDSystemInfo compatibility; CIDFont Supplement ≤
  CMap Supplement (correct rule emitted for PDF/A-4).
- §6.2.10.4.1 — TrueType Mac Roman cmap violations (PDF/A-4).
- §6.2.10.5 — Type3 font CharProc d0/d1 width check; Type3 embedding skip.
- §6.2.10.6 / §6.2.11.6 — Font BaseEncoding constraints.
- §6.2.10.7 / §6.2.10.9 / §6.2.11.7.x — ToUnicode CMap PUA detection, coverage,
  and presence requirements (PDF/A-2u/3u).
- §6.2.10.8 — PUA ActualText requirement.
- §6.2.11.4.1 / §6.2.11.4.2 — FontDescriptor presence; CharSet completeness for
  Type1 subset fonts.
- §6.2.11.5 — Font width consistency (CFF/Type1/TrueType/CIDFontType2);
  CIDFontType2 /W arrays; CID-keyed font /W arrays when CIDToGIDMap absent;
  Type1 FontFile width corrections (subset-only guard).
- §6.2.11.7.2 — ToUnicode required for all fonts in PDF/A-2u/3u.
- §6.2.11.8 — .notdef glyph references in content streams.

### Fonts & encodings (§6.3)
- §6.3.1 — Symbolic flag consistency.
- §6.3.3.3 / §6.2.11.3.3 / §6.2.10.3.3 — CMap embedding; /UseCMap in embedded
  CMaps; WMode mismatch; non-Identity Name CMap.
- §6.3.4 / §6.3.5 — Font program embedding; corrupt font programs; CIDToGIDMap;
  tiling pattern font resources; CIDFontType2 /W widths.
- §6.3.6 / §6.3.7 — Font width consistency (Type1 FontFile); symbolic TrueType
  cmap subtable count.

### Annotations & actions (§6.4–§6.6)
- §6.4.2 — NeedsRendering flag (PDF/A-2/3/4).
- §6.5.1 / §6.5.2 / §6.5.3 — Annotation type constraints; non-Btn /AP/N dict.
- §6.6.1 / §6.6.2 / §6.6.3 — Action type constraints; Widget/Field/Catalog /AA
  actions; PDF/A-4 forbidden action types; catalog /AA as §6.6.3 violation.
- §6.6.4 — Embedded file specification constraints.
- §6.7.3 — ICC profile constraints.

### XMP metadata (§6.7)
- §6.7.2 / §6.7.2.1 / §6.7.2.2 — XMP schema violations; bytes attribute.
- §6.7.3 / §6.7.3.3 / §6.7.3.4 — Info/XMP consistency (both directions);
  date values; role mapping.
- §6.7.4 — Lang attribute (PDF/A-2/3).
- §6.7.8 — dc:title/Info mismatch false positive removed.
- §6.7.9 / §6.7.9.1 / §6.7.9.3 — XMP property type validation; SeqDate items;
  malformed XMP; rdf:li missing xml:lang.
- §6.7.11 — XMP namespace detection.
- XMP extension schemas — closed-schema check.

### PDF structure (§6.8–§6.12)
- §6.8 — Embedded file specs; F/UF keys (PDF/A-2).
- §6.9 — Non-embedded FileSpec; RichMedia embedded files.
- §6.10 — Optional Content /Name uniqueness (PDF/A-4).
- §6.11 / §6.12 — Tagged PDF, structure types.

---

## Performance

- `ObjectCache`: pre-collected indirect object map; avoids repeated xref lookups in
  compliance hot paths.
- HashSet lookups replace O(n²) `Vec::contains` patterns across compliance checks.
- Process-pool orchestrator (`pool` subcommand) in `xfa-test-runner` for parallel
  corpus runs with per-PDF RSS watchdog and graceful OOM handling.
- RLIMIT_AS raised to 8 GB for veraPDF/Java compatibility on Linux.

## Test infrastructure

- `xfa-test-runner`: 24 test categories including `parse`, `render`, `text_extract`,
  `compliance`, `text_replace`, `redact`, `sign_roundtrip`, `form_write`,
  `annot_create`, `pdfa_convert`, `ocr`, and more.
- veraPDF oracle integration for compliance false-negative/false-positive tracking.
- SQLite results database with per-run IDs for regression tracking.
- `cargo-fuzz` targets for CFF parser and PDF syntax; CI nightly fuzz.
- Criterion benchmark suite (`pdf-bench`) for parse/render/extract speed baselines.
