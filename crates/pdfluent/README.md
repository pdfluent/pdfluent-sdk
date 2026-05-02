# pdfluent

Pure-Rust PDF SDK — unified public API for the PDFluent ecosystem.

**Status:** `1.0.0-beta.1`. API surface frozen per [RFC 0001](../../docs/rfc/0001-sdk-core-api.md). Method bodies are wired progressively by milestone [#52](https://github.com/jasperdew/xfa-native-rust/milestone/52).

## Usage

```toml
[dependencies]
pdfluent = "1.0.0-beta"
```

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("input.pdf")?;
    doc.metadata_mut()
        .set_title("Processed Document")
        .commit()?;
    doc.add_watermark(
        "DRAFT",
        WatermarkOptions::centered().rotated(45.0).opacity(0.3),
    )?;
    doc.save("output.pdf")?;
    Ok(())
}
```

## Features

Capability-gated via Cargo features:

| Feature | Enables | Tier |
|---|---|---|
| `signing` (default) | Digital signatures, PAdES B-LT/B-LTA | Team+ |
| `pdfa` (default) | PDF/A validation and conversion | Team+ |
| `redaction` (default) | Content redaction | Team+ |
| `ocr-tesseract` | OCR via Tesseract | Business+ |
| `ocr-paddle` | OCR via PaddleOCR | Business+ |
| `html-to-pdf` | HTML/URL → PDF | Business+ |
| `docx-export` / `xlsx-export` / `pptx-export` | Office exports | Business+ |
| `xfa-flatten` | XFA → static PDF flattening | Business+ |
| `wasm` | WebAssembly target | any |
| `tracing` | Observability via `tracing` crate | any |

## Documentation

- [RFC 0001 — SDK Core API](../../docs/rfc/0001-sdk-core-api.md)
- [Design story](../../docs/design-stories/SDK_CORE_FACADE_DESIGN_STORY.md) (program office)
- API docs: <https://docs.rs/pdfluent>

## Status

This crate is **under active development**. Every `unimplemented!` in the scaffold points at an Epic 2 sub-issue under milestone #52. See the [tracking project](https://github.com/jasperdew/xfa-native-rust/milestone/52).

## License

MIT OR Apache-2.0
