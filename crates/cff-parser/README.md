# pdfluent-cff

A CFF (Compact Font Format) parser in pure Rust, with extensions for
parsing CFF data embedded in PDF documents.

`pdfluent-cff` is one of the open-source foundation crates used by the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK. It is published as
**MIT OR Apache-2.0** so it can also be used standalone.

This crate is a **fork of the CFF1 parsing code from
[`ttf-parser`](https://crates.io/crates/ttf-parser)** by Evgeniy Reizner.
The fork adds the parts of the CFF specification that `ttf-parser` does not
need (because `ttf-parser` only sees CFF wrapped in OpenType), but a PDF
parser does.

## What this crate gives you

- Parse CFF Top DICT, name index, string index, charstring index, encoding,
  charsets, and FDSelect / FDArray (CID-keyed fonts)
- Standard PostScript glyph names lookup (`STANDARD_NAMES`)
- Glyph width helpers (`glyph_width_f64`)
- CIDFont font-matrix resolution (`glyph_fd_matrix`)

What this crate is **not**:
- Not a font rasterizer
- Not an OpenType parser (use `ttf-parser` for that)
- Not a font subsetter

## Install

```toml
[dependencies]
pdfluent-cff = "0.2"
```

The library re-exports under the `cff_parser` module path to match the
upstream API:

```rust
use cff_parser::Cff;
```

For full PDF font handling (Type 1, CFF, CMap, ToUnicode, Standard 14
fallbacks) use the [`pdf-font`](https://crates.io/crates/pdf-font) crate or
the [`pdfluent`](https://crates.io/crates/pdfluent) facade.

## License

**MIT OR Apache-2.0** — preserved from upstream `ttf-parser`.

```
Copyright (c) Evgeniy Reizner (ttf-parser CFF code)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream `ttf-parser`: <https://github.com/RazrFalcon/ttf-parser>
