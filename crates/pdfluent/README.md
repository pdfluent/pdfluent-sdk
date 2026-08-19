# pdfluent

**PDFluent is a commercial Rust PDF SDK.**

**Free for evaluation. A valid license is required for production use.**

[![crates.io](https://img.shields.io/crates/v/pdfluent.svg)](https://crates.io/crates/pdfluent)
[![Commercial License](https://img.shields.io/badge/license-PDFluent%20Commercial-blue.svg)](https://pdfluent.com/terms)

A pure-Rust PDF SDK with XFA, PDF/A, digital signatures, redaction, text extraction, forms, and a WebAssembly target. Designed as a modern alternative to iText, Apryse/PDFTron, PDFBox, and Foxit — without the JVM, without C++ memory unsafety, without "Contact Sales" pricing.

---

## Install

```bash
cargo add pdfluent
```

## Minimal example

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;
    println!("Pages: {}", doc.page_count());

    // All pages combined:
    let text = doc.extract_text()?;
    println!("{text}");

    // Per-page:
    for page in doc.pages() {
        println!("{}", page?.text()?);
    }

    Ok(())
}
```

For more, see <https://pdfluent.com/docs>.

## Status

**Beta software — public API is stabilizing.**

- Public API surface is frozen for `1.0.0-beta.x` but may receive small breaking changes before `1.0.0`.
- Not all features are fully complete; capability-gated via Cargo features.
- **XFA support is still under active development.** `xfa-flatten` is feature-gated and not yet ready for general production use; see the changelog for current XFA fidelity status.
- PDF/A, digital signatures, redaction, AcroForm fill/flatten, and text extraction have completed quality gates and are production-grade.

## Capability features

Default: `signing`, `pdfa`, `redaction`.

| Feature | Enables |
|---|---|
| `signing` (default) | PAdES B-LT / B-LTA digital signatures, CMS verification |
| `pdfa` (default) | PDF/A-1b/2b/3b validation and conversion |
| `redaction` (default) | Content redaction (search-based and region-based) |
| `ocr-tesseract` | reserved, no effect — see OCR below |
| `ocr-paddle` | reserved, no effect — see OCR below |
| `docx-export` | PDF → DOCX export |
| `xfa-flatten` | XFA form → static PDF flattening (experimental) |
| `wasm` | WebAssembly target |
| `tracing` | Observability via the `tracing` crate |

## OCR

Three routes, all supported, all through one seam. OCR lives in the separate
[`pdf-ocr`](https://crates.io/crates/pdf-ocr) crate — the `ocr-*` flags on this
crate are reserved names that currently enable nothing, so depend on `pdf-ocr`
directly.

| route | feature | needs |
|---|---|---|
| **Your cloud provider** | none | implement `OcrEngine` against Google Cloud Vision, AWS Textract, Azure Document Intelligence, or anything else |
| **PaddleOCR** | `paddle` | ONNX Runtime as a shared library; you supply the model weights, or pin their digests and let the crate fetch them |
| **Tesseract** | `tesseract` | Tesseract and Leptonica installed on the system |

Whichever you pick, the PDF side is ours: `make_searchable` renders each page,
hands the image to the recognizer, and writes the returned words and bounding
boxes back as an invisible text layer over the scan. The output is a searchable,
copyable PDF.

The facade stays free of all three so that a plain `pdfluent` dependency pulls in
no system libraries and no model downloads. Opting in is your decision to make,
not a side effect of using the SDK.

## HTML to PDF

Not offered. Rendering modern HTML and CSS correctly means shipping a browser
engine, and a rendering engine that is nearly right is worse than none — the
output looks plausible and is wrong.

Use headless Chrome or Chromium for the conversion, then hand the PDF to
PDFluent for everything after that: merging, page operations, compression,
watermarks, encryption, signing, PDF/A conversion, redaction. That combination is
well supported and is what we recommend.

## Licensing

- **Free** for evaluation, development, testing, and demonstration. The unlicensed SDK is fully functional; output carries an "unlicensed evaluation" marker (Producer string in PDF metadata + a one-time stderr warning).
- **Production use requires a valid PDFluent commercial license.** Tiered pricing from Lite to Unlimited plus Enterprise; see [pdfluent.com/pricing](https://pdfluent.com/pricing).
- **OEM redistribution** (embedding the SDK in software you distribute to third parties) requires the OEM Redistribution add-on.

See the `LICENSE` file in this crate, or read the full commercial terms at <https://pdfluent.com/terms>.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **30-day evaluation key** (full features, no output watermark): <https://pdfluent.com/trial>
- **Pricing:** <https://pdfluent.com/pricing>
- **Commercial terms:** <https://pdfluent.com/terms>
- **Support:** <https://pdfluent.com/support>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.

---

Built and maintained by [Innovation Trigger BV](https://pdfluent.com), operating as PDFluent.
