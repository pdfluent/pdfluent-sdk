# PDFluent SDK

Pure Rust PDF/A SDK with XFA support and WASM bindings.

![Crates.io](https://img.shields.io/crates/v/pdfluent)
![License](https://img.shields.io/badge/license-PDFluent%20Commercial-blue)
![Build](https://img.shields.io/github/actions/workflow/status/jasperdewinter/xfa-native-rust/ci.yml)

See [SETUP.md](SETUP.md) for contributor onboarding.

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;
    for i in 0..doc.page_count() {
        println!("{}", doc.extract_text(i)?);
    }
    let pdfa = doc.convert_to_pdfa("2b")?;
    pdfa.save("output.pdf")?;
    Ok(())
}
```

---

## Features

### XFA Forms
- **Full XFA 3.3 support** — dynamic forms with scriptable calculations
- **Font embedding pipeline** — automatic resolution of embedded, system, and fallback fonts
- **Image embedding** — JPEG/PNG XObjects with alpha transparency via SMask
- **Layout engine** — positioned, flowed (tb/lr-tb), and table layouts with pagination
- **FormCalc interpreter** — 90+ built-in functions for form calculations
- **SOM path resolution** — `xfa.form.subform[3].field[*]` expressions

### PDF Rendering
- **Pure Rust rasterizer** — vello_cpu for memory-safe rendering
- **Multi-format output** — PNG, JPEG, PDF (rasterized)
- **SSIM quality metrics** — visual comparison against Adobe Reader ground truth

### PDF/A Compliance
- **PDF/A-1a, A-2a, A-3a** — ZUGFeRD/Factur-X invoice support
- **Font embedding** — automatic embedding with subsetting
- **Color space normalization** — OutputIntent injection
- **Metadata repair** — XMP writer integration

### Document Manipulation
- **Page operations** — merge, split, insert, delete, rearrange
- **Encryption** — AES-256 PDF 2.0 encryption/decryption
- **Content editing** — find-and-replace text in content streams
- **Watermarks** — text and image overlay

### WASM & Bindings
- **WebAssembly** — runs in browser via xfa-wasm
- **Node.js** — napi-rs bindings
- **Python** — PyO3 bindings (separate crate)
- **C FFI** — pdf-capi for C/C++ integration
