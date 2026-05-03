# pdfluent-extract

PDF content extraction — text with positions, images, and full-text search.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Extracts text content from PDFs with positional metadata (bounding boxes, fonts, sizes), embedded images, and provides full-text search APIs. Supports ligature decomposition and ToUnicode-based character mapping.

## Status

Beta. Production-grade for text extraction (≥95% match vs Poppler on the 20K-PDF benchmark corpus).

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade:

```rust
use pdfluent::prelude::*;
```

For low-level access, see <https://pdfluent.com/docs>.

## Licensing

- Free for evaluation, development, and testing
- Production use requires a valid PDFluent commercial license
- Redistribution requires the OEM Redistribution add-on

See [LICENSE](LICENSE) for full terms, or visit <https://pdfluent.com/terms>.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Trial: <https://pdfluent.com/trial>
- Pricing: <https://pdfluent.com/pricing>
