# pdf-render

> **This crate is a fork.** It began as [`hayro`](https://github.com/LaurenzV/hayro) at version 0.5.0 and is
> maintained separately here. Upstream is actively developed and is the better
> choice if you do not need the changes made for PDFluent. Licensed as upstream
> (Apache-2.0 OR MIT); see `NOTICE` and `THIRD_PARTY_LICENSES.txt` for the full
> attribution.

PDF page rasterizer in pure Rust — vello-backed rendering, no PDFium, WASM-compatible.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Rasterizes PDF pages to raster images (PNG, JPEG, raw pixel buffers). Uses `vello_cpu` for content-stream rendering — no C++ dependency, compiles to WebAssembly, and produces output with high SSIM fidelity vs PDFium on the standard benchmark corpus.

## Status

Beta. Median SSIM ~0.98 vs PDFium oracle on the 1000-PDF benchmark; 87% of pages score ≥0.95 (production threshold). Improvement work continues on near-miss cases.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade or [`pdf-engine`](https://crates.io/crates/pdf-engine):

```rust
use pdfluent::prelude::*;
```

For low-level access, see <https://pdfluent.com/docs>.

## Origin

This crate was forked from [`hayro`](https://github.com/LaurenzV/hayro) by Laurenz Stampfl and has been substantially extended. The original Apache-2.0/MIT-licensed contributions remain attributed; PDFluent extensions are governed by the PDFluent Commercial License. See `THIRD_PARTY_LICENSES.txt` in the repository root.

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
