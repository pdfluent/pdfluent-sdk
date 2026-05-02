# xfa-license

License-key validation runtime — Ed25519-signed license files, feature flags, quotas, and rate limiting.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Validates Ed25519-signed license files offline (no network calls), enforces feature gates by tier, and applies usage quotas and sliding-window rate limits. Used by the SDK to enable production-grade output (no watermark) when a valid license file is present.

## Status

Beta. Used by all PDFluent commercial crates.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade — it loads license keys from `PDFLUENT_LICENCE` env var or an adjacent `pdfluent.licence.json`.

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
