# pdf-engine

Unified PDF rendering and processing engine — page rendering, text extraction, thumbnails, font orchestration, and document model.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Provides the orchestration layer that ties together PDF parsing (`pdf-syntax`, `pdf-interpret`), font handling (`pdf-font`), rendering (`pdf-render`), forms (`pdfluent-forms`), and (optionally) XFA (`pdf-xfa`) into a single coherent engine. It is the entry point used by the high-level `pdfluent` facade.

## Status

Beta. Parse, render, and text extraction paths are production-grade. XFA integration is feature-gated and experimental.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade crate, which re-exports the relevant API:

```rust
use pdfluent::prelude::*;
```

For lower-level access, see <https://pdfluent.com/docs>.

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
