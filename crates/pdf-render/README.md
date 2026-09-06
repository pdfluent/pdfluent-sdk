# pdf-render

> **This crate is a fork.** It began as [`hayro`](https://github.com/LaurenzV/hayro) at version 0.5.0 and is
> maintained separately here. Upstream is actively developed and is the better
> choice if you do not need the changes made for PDFluent. Licensed as upstream
> (Apache-2.0 OR MIT); see `NOTICE` and `THIRD_PARTY_LICENSES.txt` for the full
> attribution.

PDF page rasterizer in pure Rust — vello-backed rendering, no PDFium, WASM-compatible.

It is used by the [PDFluent](https://pdfluent.com) Rust PDF SDK, on the
upstream terms: **Apache-2.0 OR MIT**, for the whole crate, PDFluent's
extensions included.

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

## License

**Apache-2.0 OR MIT** — preserved from upstream `hayro`. Relicensing is
prohibited, so how far this fork is extended does not move it: the extensions
are licensed on the same terms as the code they were built on. The upstream
attributions are in `THIRD_PARTY_LICENSES.txt` in the repository root.

```
Copyright (c) Laurenz Stampfl (hayro)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
