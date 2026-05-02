# Changelog — pdfluent

All notable changes to the `pdfluent` crate are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [1.0.0-beta.3] — Unreleased (license model correction)

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
