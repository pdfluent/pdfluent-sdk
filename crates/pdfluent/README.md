# pdfluent

[Source on GitHub](https://github.com/pdfluent/pdfluent-sdk) · [Documentation](https://pdfluent.com/docs) · [Report an issue](https://github.com/pdfluent/pdfluent-sdk/issues)

**A pure-Rust PDF SDK: render, extract, edit, sign, redact, and validate PDF/A.**

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

[![crates.io](https://img.shields.io/crates/v/pdfluent.svg)](https://crates.io/crates/pdfluent)
[![Licence](https://img.shields.io/badge/licence-AGPL--3.0--only%20OR%20Commercial-111111)](https://github.com/pdfluent/pdfluent-sdk/blob/main/LICENSE)

PDF/A validation and conversion, digital signatures, redaction, text extraction,
AcroForm fill and flatten, page manipulation, encryption, and a WebAssembly
target, with XFA behind a feature flag. The default build is pure Rust with no
C or C++ dependency (claim A07): no JVM to run, no native library to ship, and a
price list you can read without talking to sales.

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

From 1.0.0 the public API follows semantic versioning: a breaking change means
a new major version.

- PDF/A validation and conversion, digital signatures, redaction, AcroForm fill
  and flatten, and text extraction are the supported surface, each behind the
  Cargo feature named below.
- **XFA is experimental.** `xfa-flatten` is feature-gated and not part of what
  the release supports; see the changelog for its current state.
- Rendering fidelity, PDF/A conversion and text extraction are measured on
  fixed corpora against readers that are not ours, and every published figure
  carries a claim ID at <https://pdfluent.com/benchmarks/how-we-measure>. This
  README repeats none of them.

## Capability features

Default: `signing`, `pdfa`, `redaction`.

| Feature | Enables |
|---|---|
| `signing` (default) | PAdES B-B / B-T / B-LT digital signatures, CMS verification. B-LTA is not implemented |
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

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **Pricing:** <https://pdfluent.com/sdk/pricing>
- **Commercial terms:** <https://github.com/pdfluent/pdfluent-sdk/blob/main/LICENSE-COMMERCIAL>
- **Support:** <https://pdfluent.com/support>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.

---

Built and maintained by [Innovation Trigger BV](https://pdfluent.com), operating as PDFluent.
