# pdf-manip

Page-level PDF manipulation — merge, split, extract, rotate, watermark, encrypt/decrypt, header/footer, and compression.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Implements the page-level operations exposed by the `pdfluent` facade: assembling and disassembling documents, applying watermarks and headers/footers, AES-256 encryption with permission flags, and content-preserving compression.

## Status

Beta. Production-grade — 0 crashes on the SafeDocs corpus.

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
