# pdfluent-sign

PDF digital signatures — PAdES B-B / B-T / B-LT / B-LTA, CMS / PKCS#7, certificate-chain validation, DocMDP / FieldMDP, LTV.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Signs and validates PDFs to PAdES profile compliance: B-B (basic), B-T (timestamp), B-LT (long-term validation material), and B-LTA (long-term archival). Supports incremental update signing, signature appearances, and DocMDP enforcement.

## Status

Beta. Production-grade — 0 fails on 20K-PDF sign+verify roundtrip suite.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `signing` feature (enabled by default):

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
