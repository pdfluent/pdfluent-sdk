# pdf-compliance

PDF/A, PDF/UA, and PDF/X compliance — validation and conversion.

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Validates PDFs against archival and accessibility standards (PDF/A-1b, 2b, 3b; PDF/UA; PDF/X) and converts compliant-leaning PDFs to strict compliance. Validation parity with veraPDF — 0 false negatives on the audit corpus.

## Status

Validation is checked against veraPDF in CI, and conversion is measured on four axes (conformance, text retention, visible change, output size). The figures are published with a claim ID at <https://pdfluent.com/benchmarks/how-we-measure>; none are repeated here.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `pdfa` feature (enabled by default):

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
