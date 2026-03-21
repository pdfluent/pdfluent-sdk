# XFA PDF SDK

High-performance PDF processing SDK in pure Rust — rendering, extraction, forms, signatures, and PDF/A compliance, with bindings for Python, Node.js, Java, and WebAssembly.

![Rust](https://img.shields.io/badge/rust-1.80%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Crates](https://img.shields.io/badge/crates-42-green)

---

## Features

| | |
|---|---|
| **Parse & Render** | Pure-Rust rasterizer (vello_cpu). No system dependencies, no PDFium. |
| **Text Extraction** | Plain text or structured blocks with position, font size, and coordinates. Full-text search across pages. |
| **AcroForms** | Read and write text, checkbox, dropdown, and signature fields. |
| **XFA Forms** | Full XFA engine: DOM/SOM resolution, FormCalc scripting, layout, font resolution. |
| **Annotations** | Read existing annotations; add highlight, freetext, underline, strikeout. |
| **Digital Signatures** | PAdES, CMS, TSA timestamping, certificate chain validation, DocMDP/FieldMDP, LTV. |
| **Redaction** | Permanent content-stream removal. Operates on text and image areas. |
| **PDF/A Compliance** | Validate and convert to PDF/A-1B, 2B, 3B. Issue-level reporting per clause. |
| **Encryption** | AES-256 (PDF 2.0), RC4-128. User and owner passwords. Decrypt in-place. |
| **Manipulation** | Merge, split, reorder, rotate pages. Watermarks, bookmarks, XMP metadata. |
| **Conversions** | Export to DOCX, XLSX, PPTX. Import/export FDF, XFDF. ZUGFeRD/Factur-X. |
| **OCR** | Tesseract and PaddleOCR backends, feature-gated. |
| **Image Codecs** | Built-in CCITT Group 3/4, JBIG2, JPEG 2000 decoders. |

---

## Language Bindings

| Language | Package | Import |
|---|---|---|
| Python | `pip install xfa-pdf` | `import xfa_pdf` |
| Node.js | `npm install @xfa-engine/pdf-node` | `require('@xfa-engine/pdf-node')` |
| Java | JNI — `com.xfa.pdf` | `import com.xfa.pdf.PdfDocument` |
| C / C++ | `pdf-capi` (PDFium-compatible ABI) | `#include "pdf_capi.h"` |
| WebAssembly | `xfa-wasm` (wasm-bindgen) | browser / Node.js WASM |

---

## Quick Start

### Python

```python
from xfa_pdf import Document, validate_pdfa, merge_pdfs

# Open, inspect, render
with Document("invoice.pdf") as doc:
    print(f"{doc.page_count} pages — {doc.metadata.title}")
    doc[0].render(dpi=150).save("page_0.png")   # requires Pillow

# Fill a form field
doc = Document("form.pdf")
doc.set_form_field("Address.Street", "123 Main St")
doc.save("form_filled.pdf")

# Redact sensitive text
doc = Document("contract.pdf")
report = doc.redact_text("CONFIDENTIAL")
doc.save("contract_redacted.pdf")

# Validate PDF/A
report = validate_pdfa("archive.pdf")
print(f"Compliant: {report.is_compliant}  Level: {report.pdfa_level}")
```

### Node.js

```javascript
const { openPdf, mergePdfs, validatePdfa } = require('@xfa-engine/pdf-node');

// Open and render
const doc = openPdf('invoice.pdf');
console.log(`${doc.pageCount} pages`);
const { data, width, height } = doc.renderPage(0, { dpi: 150 });

// Fill a form field and save
doc.setFieldValue('Address.Street', '123 Main St');
doc.save('form_filled.pdf');

// Merge and validate
mergePdfs(['a.pdf', 'b.pdf'], 'merged.pdf');
const report = validatePdfa('archive.pdf', '2b');
console.log(`Compliant: ${report.compliant}`);
```

### Rust

```rust
use std::sync::Arc;
use pdf_engine::{PdfDocument, RenderOptions};

fn main() -> anyhow::Result<()> {
    let data = Arc::new(std::fs::read("invoice.pdf")?);
    let doc = PdfDocument::open(data)?;

    println!("{} pages — {:?}", doc.page_count(), doc.info().title);

    let opts = RenderOptions { dpi: 150.0, ..Default::default() };
    let page = doc.render_page(0, &opts)?;
    println!("Rendered {}×{} px", page.width, page.height);

    let text = doc.extract_text(0)?;
    println!("{text}");
    Ok(())
}
```

---

## Architecture

The SDK is organized as a layered Rust workspace. At the base, two complementary PDF parsers handle read-only operations (`pdf-syntax`, a hardened hayro fork) and mutations (`lopdf`). Above that, a rendering pipeline (`pdf-interpret` → `pdf-render`) produces pixel output using the vello_cpu rasterizer. Domain crates (`pdf-forms`, `pdf-xfa`, `pdf-annot`, `pdf-sign`, `pdf-compliance`) are built on top of this foundation and expose a unified API through `pdf-engine`. Language bindings wrap `pdf-engine` for each target environment.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full dependency graph, design decisions, and implementation status.

---

## Crate Map

**Parsing & Decoding (7)**
- `pdf-syntax` — read-only PDF parser (hayro fork)
- `pdf-interpret` — content stream interpreter
- `pdf-font` — Type1, CFF, CMap, CID font parsing
- `hayro-ccitt` — CCITT Group 3/4 image codec
- `hayro-jbig2` — JBIG2 image codec
- `hayro-jpeg2000` — JPEG 2000 image codec
- `lopdf` — read-write PDF mutation (forked dependency)

**Rendering (2)**
- `pdf-render` — pure-Rust rasterizer (vello_cpu)
- `cff-parser` — CFF/Type2 charstring parser (forked, with CID glyph-width fix)

**Document API (1)**
- `pdf-engine` — unified public API: render, text extraction, thumbnails, metadata, bookmarks

**Forms & Annotations (3)**
- `pdf-forms` — AcroForm field read/write
- `pdf-xfa` — XFA engine: DOM extraction, layout rendering, font resolution
- `pdf-annot` — annotation read/write (highlight, freetext, underline, strikeout)

**XFA Core (4)**
- `xfa-dom-resolver` — SOM path resolution, template/data DOM management
- `formcalc-interpreter` — FormCalc scripting engine (lexer, parser, 80+ built-ins)
- `xfa-layout-engine` — XFA box model, pagination, reflow
- `xfa-json` — XFA layout → JSON serialization

**Manipulation & Security (4)**
- `pdf-manip` — merge, split, rotate, watermarks, bookmarks, XMP metadata
- `pdf-sign` — PAdES/CMS signatures, TSA timestamping, certificate chain, LTV
- `pdf-redact` — permanent content-stream redaction
- `pdf-compliance` — PDF/A and PDF/UA validation and conversion

**Content Intelligence (3)**
- `pdf-extract` — structured text with positions, image extraction, full-text search
- `pdf-ocr` — OCR integration (Tesseract + PaddleOCR, feature-gated)
- `pdf-diff` — visual page comparison (SSIM)

**Data Exchange (4)**
- `pdf-docx` — PDF → DOCX conversion
- `pdf-xlsx` — PDF → XLSX conversion
- `pdf-pptx` — PDF → PPTX conversion
- `pdf-invoice` — ZUGFeRD/Factur-X, FDF/XFDF

**Language Bindings (5)**
- `pdf-capi` — C API (PDFium-compatible ABI, cdylib + staticlib)
- `pdf-python` — Python bindings (PyO3, `xfa-pdf` on PyPI)
- `pdf-node` — Node.js bindings (napi-rs, `@xfa-engine/pdf-node`)
- `pdf-java` — Java bindings (JNI, `com.xfa.pdf`)
- `xfa-wasm` — WebAssembly bindings (wasm-bindgen)

**Applications (4)**
- `xfa-cli` — command-line tools (corpus runner, accuracy reports)
- `xfa-api-server` — HTTP REST API server
- `pdf-desktop` — desktop PDF viewer
- `pdfium-ffi-bridge` — PDFium FFI (optional; used by golden tests only)

**Testing & Infrastructure (5)**
- `xfa-golden-tests` — visual regression test suite
- `xfa-test-runner` — corpus-scale test runner with SQLite results
- `pdf-bench` — Criterion performance benchmarks
- `xfa-license` — license key validation
- `xfa-license-gen` — license key generation

---

## Minimum Supported Rust Version (MSRV)

**Rust 1.80.0** or later is required. The MSRV is declared in `Cargo.toml` via the `rust-version` field and tested in CI.

## Security

See [SECURITY.md](SECURITY.md) for our vulnerability disclosure policy, threat model, and security measures.

---

## License

MIT
