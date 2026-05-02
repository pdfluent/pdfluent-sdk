# pdfluent-cff

A CFF (Compact Font Format) parser in pure Rust.

This crate is a fork of the CFF1 parsing code from the [`ttf-parser`](https://crates.io/crates/ttf-parser) crate by Evgeniy Reizner, with added features needed for parsing CFF data embedded in PDF files (where CFF appears stand-alone rather than wrapped in an OpenType font).

It is a low-level component used by the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

## License

**MIT OR Apache-2.0** (preserved from upstream `ttf-parser`).

Original copyright: Evgeniy Reizner. PDFluent fork: Innovation Trigger BV. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Usage

This crate can be used standalone. For the full PDFluent SDK, see [`pdfluent`](https://crates.io/crates/pdfluent).

## Links

- PDFluent: <https://pdfluent.com>
- Upstream ttf-parser: <https://github.com/RazrFalcon/ttf-parser>
