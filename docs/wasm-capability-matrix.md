# PDFluent WASM Capability Matrix

Status of each SDK capability in the `@pdfluent/sdk-wasm` browser build,
versus the native Rust crates / Tauri-side bindings.

Versions: `1.0.0-beta.11` of `@pdfluent/sdk-wasm`; corresponding Rust crates
at `1.0.0-beta.5` (engine) / `1.0.0-beta.4` (pdf-annot). (Package was
previously published as `@pdfluent/xfa-wasm`; renamed to `@pdfluent/sdk-wasm`
in `1.0.0-beta.11`.)

Wave 3 adds the stateful `PdfDocMut` editing handle. All mutating
methods are available on **both** classes:

- `PdfDoc.*` — stateless / one-shot; returns new bytes per call
- `PdfDocMut.*` — stateful; mutates in place; single `save()` at the end

`PdfDocMut` is **2.7× faster** than the stateless chain for a 4-mutation
editor session. See `benchmarks/runs/wasm_sdk_dx/ROUND3_PERFORMANCE_BENCHMARK.md`.

| # | Capability | Rust crate | WASM (1.0.0-beta.11) | Native | Notes |
|---|------------|------------|---------------------|--------|-------|
| 1 | Open PDF from bytes | `pdf-engine` + `pdf-syntax` | ✅ | ✅ | `PdfDoc.open(bytes)` |
| 2 | Page count, dimensions | `pdf-engine` | ✅ | ✅ | `pageCount()`, `pageWidth()`, `pageHeight()` |
| 3 | Document metadata (Title, Author, …) | `pdf-engine` | ✅ | ✅ | `metadata()` returns JSON |
| 4 | Text extraction per page | `pdf-engine` + `pdf-extract` | ✅ | ✅ | `text(pageIndex)` |
| 5 | Per-glyph text positions | `pdf-engine` | ✅ | ✅ | `getTextPositions(pageIndex)` |
| 6 | Render page to RGBA | `pdf-render` (vello_cpu) | ✅ | ✅ | `renderPage(index, scale)` |
| 7 | Render page directly to Canvas2D | `pdf-render` + custom device | ✅ | n/a | `renderPageToCanvas(canvas, index, scale)` |
| 8 | Render thumbnail | `pdf-render` | ✅ | ✅ | `renderThumbnail(index, maxDim)` |
| 9 | Read AcroForm fields | `pdf-forms` | ✅ | ✅ | exposed via `metadata()` for inspection; full API on native |
| 10 | **Write AcroForm fields** | `pdf-forms` (unified writeback chain) | ✅ **(Wave 2)** | ✅ | `PdfDoc.setFormField`: text fields only; `PdfDocMut.setFormField`: all types (text/checkbox/radio/choice); `PdfDocMut.setMultiSelect`: multi-select list boxes |
| 11 | XFA detection + parse | `xfa-dom-resolver` + `formcalc-interpreter` | ✅ | ✅ | `XfaEngine.fromJson` etc. |
| 12 | XFA flatten | `pdf-xfa` (via xfa-wasm engine) | ✅ | ✅ | `flattenXfa()` |
| 13 | Read annotations | `pdf-annot` | ✅ | ✅ | `getAnnotations(pageIndex)` |
| 14 | **Add highlight annotation** | `pdf-annot` | ✅ **(Wave 2)** | ✅ | `addHighlight` |
| 15 | **Add sticky-note annotation** | `pdf-annot` | ✅ **(Wave 2)** | ✅ | `addStickyNote` |
| 16 | **Add free-text annotation** | `pdf-annot` | ✅ **(Wave 2)** | ✅ | `addFreeText` |
| 17 | **Page delete** | `pdf-manip::pages` | ✅ **(Wave 2)** | ✅ | `deletePages` |
| 18 | **Page rotate** | `pdf-manip::pages` | ✅ **(Wave 2)** | ✅ | `rotatePage` |
| 19 | **Page reorder** | `pdf-manip::pages` | ✅ **(Wave 2)** | ✅ | `reorderPages` |
| 20 | **Page extract / split** | `pdf-manip::pages` | ✅ **(Wave 2)** | ✅ | `extractPages` |
| 21 | Merge documents | `pdf-manip::pages` | ✅ | ✅ | `merge(other)` |
| 22 | **Text watermark** | `pdf-manip::watermark` | ✅ **(Wave 2)** | ✅ | `addTextWatermark` |
| 23 | Image watermark | `pdf-manip::watermark` + `image` | ❌ planned 1.1 | ✅ | larger bundle; deferred |
| 24 | **Stream compress** | `pdf-manip::optimize` + `flate2` | ✅ **(Wave 2)** | ✅ | `compress()` |
| 25 | **Redact by region** | `pdf-redact` | ✅ **(Wave 2)** | ✅ | `redactRegion` |
| 26 | **Redact by search** | `pdf-redact` | ✅ **(Wave 2)** | ✅ | `redactSearch` |
| 27 | PDF/A validate | `pdf-compliance` | ✅ | ✅ | `validatePdfA(level)` |
| 28 | PDF/A convert | `pdf-manip::pdfa_*` | ✅ | ✅ | `convertToPdfa(level)` |
| 29 | Read digital signatures | `pdf-sign` | ✅ | ✅ | `signatures()`, `verifySignatures()` |
| 30 | DSS info | `pdf-sign` | ✅ | ✅ | `dssInfo()` |
| 31 | Create digital signature | `pdf-sign` + crypto | ❌ planned later | ✅ | needs key-input UX |
| 32 | Encrypt / password-protect | `pdf-manip::encrypt` + RustCrypto | ❌ planned later | ✅ | bundle cost; consumer demand low |
| 33 | OCR (PaddleOCR / Tesseract) | `pdf-ocr` | ❌ never (use `tesseract.js`) | ✅ | C++ engines + model files |
| 34 | Export to DOCX / XLSX / PPTX | `pdf-docx`, `pdf-xlsx`, `pdf-pptx` | ❌ blocked on native | ⏳ scaffolded | wait until native produces output |
| 35 | License activation | `pdfluent::license` | ✅ | ✅ | `activateLicenseKey(key)`, `licenseStatus()` |
| 36 | XFA field fill (enumerate + set + datasets save) | `pdf-xfa` `XfaSession` via `pdfluent::xfa` | ❌ planned (bindings follow-up) | ✅ (Rust, Phase 1) | `PdfDocument::xfa_form_model` / `set_xfa_field_value`; WASM/Node/Python/Java/C-API wiring tracked as XFA Phase-1 follow-up |

Legend:
- ✅ available
- ⏳ work-in-progress
- ❌ deliberately not yet exposed (with reason)

## Wave 2 entries

The bold rows above (rows 10, 14–22, 24–26) are the additions in
`1.0.0-beta.9` per the Wave 2 implementation in `crates/xfa-wasm/src/edits.rs`.

## Bundle size

- 1.0.0-beta.8 (before Wave 2): 3.6 MB tarball / 10.2 MB unpacked
- 1.0.0-beta.9 (post-Wave 2):   3.8 MB tarball / ~11 MB unpacked
- **1.0.0-beta.11 (current)**:  see `crates/xfa-wasm/CHANGELOG.md` for the
  current bundle size after the B3 wasm-opt -O3 pass.

Under the soft target of 5 MB gzipped. Hard limit 15 MB.
