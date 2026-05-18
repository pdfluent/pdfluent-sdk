# Changelog

All notable changes to the `@pdfluent/sdk-wasm` npm package (previously
published as `@pdfluent/xfa-wasm`; renamed to `@pdfluent/sdk-wasm`).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this package adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased] — 1.0.0-beta.10

### Added — Wave 3: `PdfDocMut`

- **`PdfDocMut` — stateful editing handle.** New top-level class
  alongside `PdfDoc`. Holds one mutable `lopdf::Document` for the
  lifetime of the session. All 13 mutation methods are mirrored with
  `&mut self` semantics; final `save()` returns `Uint8Array`.
- Methods: `open`, `pageCount`, `save`, `free` (auto), `deletePages`,
  `rotatePage`, `reorderPages`, `extractPages`, `setFormField`,
  `setFormFields`, `addHighlight`, `addStickyNote`, `addFreeText`,
  `addTextWatermark`, `redactRegion`, `redactSearch`, `compress`.
- `save()` is non-consuming — take intermediate snapshots and keep editing.
- `extractPages` returns bytes for a NEW subdocument; current editor unchanged.
- Performance benchmark demonstrates **2.7× wall-clock speedup** vs.
  the stateless `PdfDoc` chain for a 4-mutation editor session
  (12 parse/serialise cycles → 2). Output bytes byte-identical.
- 14 new native tests + 6 wasm-bindgen-test error-path tests for
  `PdfDocMut`.

### Documentation

- README: PdfDocMut section recommended for editor workflows; existing
  stateless `PdfDoc` mutations kept as one-shot helpers.
- `docs/wasm-capability-matrix.md`: PdfDocMut entries added.

### Compatibility

- `PdfDoc` and all its stateless Wave 2 mutations are unchanged. No
  breaking changes to any existing API.

## [Unreleased] — 1.0.0-beta.9

### Added

Wave 2 browser-SDK edit operations on `PdfDoc`:

- `deletePages(pages)` — remove 0-based pages; returns new bytes
- `rotatePage(pageIndex, degrees)` — rotate by 90/180/270 (or negative);
  rejects non-orthogonal values
- `reorderPages(newOrder)` — re-arrange pages by permutation
- `extractPages(pages)` — extract a subset into a new PDF (use for split)
- `setFormField(path, value)` — set a single AcroForm text field
- `setFormFields(jsonObject)` — bulk-set from `{path: value}` map
- `addTextWatermark(text, opacity)` — diagonal text watermark, all pages
- `addHighlight(pageIndex, x, y, w, h, colorHex?)` — highlight annotation
- `addStickyNote(pageIndex, x, y, contents)` — sticky-note annotation
- `addFreeText(pageIndex, x, y, w, h, contents)` — free-text annotation
- `redactRegion(pageIndex, x, y, w, h)` — permanent rectangle redaction
- `redactSearch(query)` — search-and-redact literal text matches
- `compress()` — re-deflate content streams for smaller file size

### Changed

- `addHighlight`, `addStickyNote`, `addFreeText` are now **instance methods**
  on `PdfDoc` (call them as `doc.addHighlight(...)`). Their previous
  static form (`PdfDoc.addHighlight(bytes, ...)`) from 1.0.0-beta.8 is
  removed. Migration: open the document once with `PdfDoc.open(bytes)`,
  then call the method on the instance.
- `wasm` feature now includes `annotate` (was excluded; the stale
  "native-only" exclusion dated from before the lopdf `wasm_js` fix).
- Package description updated to reflect the full PDFluent browser-SDK
  surface, not only XFA.

### Test coverage

- Native tests: 11 happy-path tests for new methods (`cargo test -p xfa-wasm --test edits`)
- Wasm-pack tests: 6 error-path tests under wasm32 target
- Total Wave 2 test count: 17, all passing
- Existing smoke + license suite: 21 tests, still passing

### Bundle size

- 1.0.0-beta.8: 3.6 MB tarball / 10.2 MB unpacked
- 1.0.0-beta.9: 3.8 MB tarball / ~11 MB unpacked
- Wave 2 adds ~200 KB total binary, well under the 5 MB gzipped soft target

## [1.0.0-beta.8] — 2026-05-15

### Added

- License activation API: `activateLicenseKey(key)`, `licenseStatus()`,
  and the `LicenseStatus` class with `tier`/`source`/`outputIsMarked`.
- `PDFLUENT_LICENSE_KEY` env var honoured automatically by the Rust core
  when the host (Node) exposes env vars.

### Notes

- Process-global, set-once tier semantics. Reload the WASM instance to
  switch tiers.
- `activateLicenseFile` is intentionally not exposed (no browser FS).

## [1.0.0-beta.7] — 2026-05-15 (deprecated)

Published outside the canonical release flow. Lacks the license-activation
API and the canonical PDFluent package metadata. Use 1.0.0-beta.8 or later.

## [1.0.0-beta.6] and earlier

XFA form processing + PDF analysis surface. See git history for details.
