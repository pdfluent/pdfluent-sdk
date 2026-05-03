# pdf-redact

True PDF content redaction — search-based and region-based, removes the underlying content (not just visual occlusion).

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Permanently removes content from a PDF: text matched by string or regex, image regions, and annotations. Unlike viewer-level black-box redaction, the content is genuinely deleted from the underlying object stream — safe for compliance and disclosure workflows.

## Status

Beta. Production-grade for text and region redaction. Image XObject redaction is in progress.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `redaction` feature (enabled by default):

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
