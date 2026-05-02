# pdf-compliance

PDF/A, PDF/UA, and PDF/X compliance — validation and conversion.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Validates PDFs against archival and accessibility standards (PDF/A-1b, 2b, 3b; PDF/UA; PDF/X) and converts compliant-leaning PDFs to strict compliance. Validation parity with veraPDF — 0 false negatives on the audit corpus.

## Status

Beta. Production-grade — PDF/A validation has 0 false negatives vs veraPDF, conversion has a 99.8% pass rate on the 20K-PDF corpus.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `pdfa` feature (enabled by default):

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
