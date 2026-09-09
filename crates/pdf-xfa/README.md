# pdf-xfa

XFA processing engine — extraction, layout rendering, font resolution. **Experimental — under active development.**

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Implements the XFA (XML Forms Architecture) processing pipeline used by Dutch government forms, banking forms, and other complex interactive PDFs that AcroForms cannot represent. Parses the XFA template, merges with data, runs FormCalc expressions, lays out pages, and renders to PDF or paginated output.

## Status

**Experimental.** XFA support is still under active development. Structural flatten and the public XFA API are panic-free and propagate all errors as typed Results; crash-safety and timeout gates run on an internal corpus of real forms. Visual fidelity versus reference output is not a published claim; validate on your own forms before relying on rendering parity. Use behind a feature flag.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `xfa-flatten` feature:

```rust
use pdfluent::prelude::*;
```

For low-level access, see <https://pdfluent.com/docs>.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
