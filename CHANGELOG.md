# Changelog

All notable changes to the xfa-native-rust PDF engine are documented here.

## [Unreleased] — 2026-04-22

### Added

- `PadesProfile` enum and `SignerConfig` struct in `pdf-sign` — #1295
- `infer_pades_profile`: B-B / B-T / B-LT / B-LTA auto-selected from `tsa_url` + `enable_ltv` — #1295
- `Pkcs12Signer::with_config` builder + `effective_pades_profile()` — #1295

### Changed

- **[BEHAVIOR]** `Pkcs12Signer::effective_pades_profile()` now infers the correct PAdES level from the attached `SignerConfig`. Previously there was no inference and callers selected the signing entry point manually. Callers with an explicit `profile` field are unaffected — explicit always wins — #1295

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
- WASM build support is experimental; `xfa-wasm` compiles but browser integration
  is untested end-to-end.

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

- `pdf-python`: PyO3-based Python package (`xfa-pdf`) with maturin build; covers
  open/save, merge, text extraction, form fields, annotations, redact, encrypt/decrypt,
  PDF/A validate.
- `pdf-node`: Node.js native addon (NAPI) with npm-ready `package.json`; covers the
  same API surface as the Python bindings.
- `pdf-java`: JNI wrapper with Javadoc, Maven-ready POM, JUnit test scaffolding.
- `pdf-capi`: C API (`pdf-capi`) with 12+ core operations for FFI consumers.
- `xfa-wasm`: Rust→WASM target (wasm-pack); basic parse and text extract in browser.
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
