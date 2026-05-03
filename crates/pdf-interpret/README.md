# pdf-interpret

A PDF **content-stream interpreter** in pure Rust — the layer that turns
PDF page operators (text-show, path-paint, image-draw, etc.) into a
typed event stream that a renderer or extractor can consume.

`pdf-interpret` is one of the open-source foundation crates used by the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK. It is published as
**MIT OR Apache-2.0** so it can also be used standalone.

This crate is a **fork of [`hayro-interpret`](https://github.com/LaurenzV/hayro)**
by Laurenz Stampfl, with PDFluent-specific font integration through
[`pdf-font`](https://crates.io/crates/pdf-font).

## What this crate gives you

- Walk a PDF page's content stream and emit typed graphics operations
- Resolve graphics state, text state, and color spaces
- Image XObject and Form XObject expansion
- Type 1, CFF, and TrueType font glyph resolution (via `pdf-font`)
- Pure Rust — memory-safe, no C dependency, compiles to WebAssembly

What this crate is **not**:
- Not a renderer (it produces a stream of operations; rasterization is
  separate — see `pdf-render`)
- Not a high-level extractor (use `pdfluent-extract` for text + positions)
- Not a writer (read-only)

## Usage

This crate is typically used through the
[`pdfluent`](https://crates.io/crates/pdfluent) facade or via
[`pdf-engine`](https://crates.io/crates/pdf-engine), which wires it
together with the rendering and extraction backends.

Direct usage is possible if you are building a custom PDF processing tool
that needs the operator stream:

```toml
[dependencies]
pdf-interpret = "0.5"
```

## License

**MIT OR Apache-2.0** — preserved from upstream `hayro-interpret`.

```
Copyright (c) Laurenz Stampfl (hayro-interpret)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
