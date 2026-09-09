# pdf-engine

Unified PDF rendering and processing engine — page rendering, text extraction, thumbnails, font orchestration, and document model.

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Provides the orchestration layer that ties together PDF parsing (`pdf-syntax`, `pdf-interpret`), font handling (`pdf-font`), rendering (`pdf-render`), forms (`pdfluent-forms`), and (optionally) XFA (`pdf-xfa`) into a single coherent engine. It is the entry point used by the high-level `pdfluent` facade.

## Status

Parsing, rendering and text extraction are the default paths. XFA integration is feature-gated and experimental.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade crate, which re-exports the relevant API:

```rust
use pdfluent::prelude::*;
```

For lower-level access, see <https://pdfluent.com/docs>.

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
