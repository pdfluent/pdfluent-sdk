# pdfluent

**Pure-Rust PDF library** — read, edit, sign, redact, extract, and convert PDF documents.

[![crates.io](https://img.shields.io/crates/v/pdfluent.svg)](https://crates.io/crates/pdfluent)
[![docs.rs](https://docs.rs/pdfluent/badge.svg)](https://docs.rs/pdfluent)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://pdfluent.com/license)

---

## Quick install

```bash
cargo add pdfluent
```

Or in `Cargo.toml`:

```toml
[dependencies]
pdfluent = "1.0.0-beta.1"
```

## Quick example

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;
    println!("Pages: {}", doc.page_count());

    let text = doc.extract_text(0)?;
    println!("First page text: {}", text);

    Ok(())
}
```

## Features

Capability-gated via Cargo features (default: `signing`, `pdfa`, `redaction`):

| Feature | Enables | Licence tier |
|---|---|---|
| `signing` (default) | Digital signature validation — PAdES B-LT/B-LTA, CMS | Team+ |
| `pdfa` (default) | PDF/A validation and conversion | Team+ |
| `redaction` (default) | Content redaction | Team+ |
| `ocr-tesseract` | OCR via Tesseract | Business+ |
| `ocr-paddle` | OCR via PaddleOCR | Business+ |
| `html-to-pdf` | HTML/URL → PDF conversion | Business+ |
| `docx-export` | PDF → DOCX export | Business+ |
| `xfa-flatten` | XFA form → static PDF flattening | Business+ |
| `wasm` | WebAssembly target | any |
| `tracing` | Observability via the `tracing` crate | any |

## Evaluation

pdfluent ships with a built-in evaluation mode — **no sign-up, no API key required**. Output carries a producer stamp (`Unlicensed PDFluent evaluation`) until a licence key is activated. This lets you evaluate the full SDK in CI, local tooling, or prototypes without friction.

To remove the stamp: [request a free 30-day trial key](https://pdfluent.com/trial) or [buy a licence](https://pdfluent.com/pricing).

## Documentation

- **API reference:** <https://docs.rs/pdfluent>
- **Getting started guide:** <https://pdfluent.com/docs>
- **Trial (stamp-free evaluation):** <https://pdfluent.com/trial>
- **Pricing:** <https://pdfluent.com/pricing>
- **Changelog:** <https://github.com/pdfluent/pdfluent/blob/master/crates/pdfluent/CHANGELOG.md>

## Status

`1.0.0-beta.1` — API surface is frozen. Implementation is being wired progressively per milestone. See [the tracker](https://github.com/pdfluent/pdfluent/milestone/52) for what's fully wired vs. scaffolded.

## License

MIT OR Apache-2.0 — see [pdfluent.com/license](https://pdfluent.com/license) for commercial licence terms.
