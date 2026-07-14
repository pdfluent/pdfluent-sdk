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
| `ocr-tesseract` | OCR via Tesseract |
| `ocr-paddle` | OCR via PaddleOCR |
| `docx-export` | PDF → DOCX export |
| `xfa-flatten` | XFA form → static PDF flattening (experimental) |
| `wasm` | WebAssembly target |
| `tracing` | Observability via the `tracing` crate |

## Licensing

- **Free** for evaluation, development, testing, and demonstration. The unlicensed SDK is fully functional; output carries an "unlicensed evaluation" marker (Producer string in PDF metadata + a one-time stderr warning).
- **Production use requires a valid PDFluent commercial license.** Tiered pricing from Lite to Unlimited plus Enterprise; see [pdfluent.com/pricing](https://pdfluent.com/pricing).
- **OEM redistribution** (embedding the SDK in software you distribute to third parties) requires the OEM Redistribution add-on.

See the `LICENSE` file in this crate, or read the full commercial terms at <https://pdfluent.com/terms>.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **30-day clean trial key:** <https://pdfluent.com/trial>
- **Pricing:** <https://pdfluent.com/pricing>
- **Commercial terms:** <https://pdfluent.com/terms>
- **Support:** <https://pdfluent.com/support>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.

---

Built and maintained by [Innovation Trigger BV](https://pdfluent.com), operating as PDFluent.
