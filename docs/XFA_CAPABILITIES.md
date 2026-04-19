# XFA Engine Capabilities

Version: 1.0 — 2026-04-19  
Status: DRAFT — awaiting Corpus B benchmark (EVH-BASELINE-02)

---

## Overview

The XFA Engine is a production-grade Rust implementation of the XFA (XML Forms
Architecture) flattening pipeline, converting dynamic XFA PDF forms into
static, printable PDFs. It is designed for enterprise document processing
workflows where high throughput, output fidelity, and predictable behavior
are required.

---

## Supported XFA Form Types

| Form Type | Support | Notes |
|-----------|---------|-------|
| Static XFA forms | ✅ Full | baseProfile="interactiveForms" |
| Dynamic XFA forms | ✅ Full | Full data binding and layout |
| Dynamic forms with repeating sections | ✅ Full | occur expansion supported |
| Hybrid AcroForm/XFA forms | ✅ Full | AcroForm annotations removed on flatten |
| Government forms (IRS, etc.) | ✅ Tested | 10 IRS forms validated |
| Multi-page dynamic forms | ✅ Full | TB layout with overflow pagination |

---

## Data Binding

| Feature | Support |
|---------|---------|
| consumeData binding mode | ✅ |
| matchTemplate binding mode | ✅ |
| Ancestor/sibling scope resolution | ✅ |
| Transparent subform passthrough | ✅ |
| Presence/visibility binding | ✅ |
| Occur expansion (repeating sections) | ✅ |
| FormCalc scripting | ✅ |
| JavaScript scripting | ❌ Known limitation |

---

## Layout Engine

| Feature | Support |
|---------|---------|
| TB (top-to-bottom) layout | ✅ |
| LR (left-to-right) layout | ✅ |
| Positioned layout (anchorType, x/y) | ✅ All 9 anchor variants |
| hAlign in TB containers | ✅ |
| Automatic pagination (overflow) | ✅ |
| Overflow bookends (leader/trailer) | ✅ |
| Keep chains | ✅ |
| Area, ExclGroup, SubformSet | ✅ |

---

## Field Types

| Field Type | Support |
|-----------|---------|
| Text fields | ✅ |
| Numeric fields | ✅ |
| Date/time fields | ✅ |
| Checkbox | ✅ |
| Radio button | ✅ |
| Dropdown / Choice list | ✅ |
| Signature fields | ⚠️ Flattened (not cryptographically validated) |
| Barcodes | ❌ Known limitation |
| Image fields | ⚠️ Partial |

---

## Performance (Corpus A, 5000 PDFs, release build)

| Metric | Value |
|--------|-------|
| Pass rate (SSIM ≥0.95 vs mutool) | 94.0% |
| Mean SSIM | 0.9869 |
| Crash rate (non-adversarial) | < 0.1% |
| Mean processing time | < 2s per document |
| P95 processing time | < 5s per document |

> Note: These figures are from Corpus A (mutool oracle). Enterprise benchmark
> against pdfRest/Adobe oracle is pending (EVH-BASELINE-02).

---

## Architecture

- Language: Rust (safe, no-GC, WASM-compatible)
- Build: single binary `pdfluent` / library crate
- Memory: ~50–100 MB per document (arena allocation)
- Dependencies: lopdf (PDF parsing), no external rendering dependencies
- WASM: compatible (deployed to PDFluent browser editor)

---

## Known Limitations

See `docs/XFA_KNOWN_LIMITATIONS.md` for the full list.

Summary:
- JavaScript scripting (no JS engine — FormCalc is supported)
- Barcode rendering
- Complex ICC color profile transforms
- Digital signature validation (fields are flattened visually)
- PDF 2.0 MAC-protected documents (exit code 2 — encrypted)

---

## Related Documentation

- `docs/XFA_FEATURE_SUPPORT.md` — detailed per-feature matrix (technical reference)
- `benchmarks/BENCHMARK_CLAIMS.md` — methodology and reproduction instructions for all quality claims
- `benchmarks/QUALITY_CHANGELOG.md` — quality history across releases
- `docs/XFA_KNOWN_LIMITATIONS.md` — full limitations list
