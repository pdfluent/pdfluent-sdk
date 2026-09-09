# pdf-redact

True PDF content redaction — search-based and region-based, removes the underlying content (not just visual occlusion).

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Permanently removes content from a PDF: text matched by string or regex, image regions, and annotations. Unlike viewer-level black-box redaction, the content is genuinely deleted from the underlying object stream — safe for compliance and disclosure workflows.

## Status

Text and region redaction remove the content from the page. Image XObject redaction is in progress.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade with the `redaction` feature (enabled by default):

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
