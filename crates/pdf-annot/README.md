# pdf-annot

PDF annotation engine — typed access to all annotation types per ISO 32000-2 §12.5.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Reads, creates, and edits PDF annotations — text notes, highlights, underlines, squigglies, link annotations, popup notes, file attachments, and more. Provides typed accessors aligned with the ISO 32000-2 specification.

## Status

Beta. Read access is production-grade. Annotation creation and editing are stable but evolving.

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
