# Changelog — pdfluent

All notable changes to the `pdfluent` crate are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
