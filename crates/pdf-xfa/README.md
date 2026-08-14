# pdf-xfa

XFA processing engine — extraction, layout rendering, font resolution. **Experimental — under active development.**

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Implements the XFA (XML Forms Architecture) processing pipeline used by Dutch government forms, banking forms, and other complex interactive PDFs that AcroForms cannot represent. Parses the XFA template, merges with data, runs FormCalc expressions, lays out pages, and renders to PDF or paginated output.

## Status

**Experimental.** XFA support is still under active development. Structural flatten and the public XFA API are panic-free and propagate all errors as typed Results. Crash-safety and timeout gates are verified on a 1150-document internal corpus. Visual fidelity versus reference output is not yet a published claim; validate on your specific corpus before relying on rendering parity. Use behind a feature flag.

For non-XFA PDF processing, the rest of the SDK is production-grade.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `xfa-flatten` feature:

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
