# PDFluent SDK

Pure Rust PDF/A SDK with WASM bindings and experimental, feature-gated XFA support.

![Crates.io](https://img.shields.io/crates/v/pdfluent)
![License](https://img.shields.io/badge/license-PDFluent%20Commercial-blue)

See [SETUP.md](SETUP.md) for contributor onboarding.

## Cookbook

Ten recipes with the explanation around them: <https://pdfluent.com/cookbook/>.

Every block on that page is one of the `// site:<name>` markers in
[`crates/pdfluent/examples/site_snippets.rs`](crates/pdfluent/examples/site_snippets.rs),
which CI compiles, and each recipe links back to it. An example that stops
building breaks the build rather than a reader's first attempt (#164, #247).

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;
    for page in doc.pages() {
        println!("{}", page.text()?);
    }
    let report = doc.validate_pdfa(PdfAProfile::A2b)?;
    if report.is_compliant() {
        println!("PDF/A-2B ✓");
    }
    Ok(())
}
```

---

## Features

### XFA Forms (experimental, feature-gated — not production-supported)
- **XFA 3.3 feature set** — dynamic forms with scriptable calculations (experimental, behind the `xfa` feature gate)
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
- **WebAssembly** — runs in browser via the `@pdfluent/sdk-wasm` npm package
- **Node.js** — napi-rs bindings
- **Python** — PyO3 bindings (separate crate)
- **C FFI** — pdf-capi for C/C++ integration

## License

PDFluent crates are commercially licensed (see [LICENSE](LICENSE)); a few
dependencies are separately available under MIT or Apache-2.0 (see
[NOTICE](NOTICE)).

**Is the SDK covered by the free PDFluent editor license?** No. The PDFluent
desktop editor is free to use, including at work, but that license covers
the application itself. Embedding, linking, or calling this SDK (or any of
its crates or language bindings) from your own software requires a separate
SDK license. See [pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing).
