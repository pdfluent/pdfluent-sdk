# PDFluent SDK

Pure Rust PDF/A SDK with XFA support and WASM bindings.

![Crates.io](https://img.shields.io/crates/v/pdfluent)
![License](https://img.shields.io/badge/license-MIT-blue)
![Build](https://img.shields.io/github/actions/workflow/status/jasperdewinter/xfa-native-rust/ci.yml)

```rust
use pdfluent::Document;

let doc = Document::from_path("input.pdf")?;
for page in doc.pages() {
    println!("{}", page.text()?);
}
let pdfa = doc.convert_to_pdf_a()?;
pdfa.save("output.pdf")?;
```

---

## Features
