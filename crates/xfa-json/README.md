# xfa-json

XFA template / data JSON conversion. **Experimental — part of the XFA stack.**

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Converts XFA templates and data sets between their native XML form and JSON, for storage, transport, or interop with non-XML systems. Round-trips faithful enough to reconstruct the original XFA tree.

## Status

Experimental — XFA support is under active development. See [`pdf-xfa`](https://crates.io/crates/pdf-xfa) for current status.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade.

## Licensing

- Free for evaluation, development, and testing
- Production use requires a valid PDFluent commercial license
- Redistribution requires the OEM Redistribution add-on

See [LICENSE](LICENSE) for full terms, or visit <https://pdfluent.com/terms>.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
