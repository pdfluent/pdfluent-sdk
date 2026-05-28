# pdf-text-format

Text run formatting for PDFs — inject font-size and color changes into existing content streams with state isolation.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Programmatically modifies text appearance inside PDF content streams without re-rasterising the page. Style mutations (font size, fill/stroke color) are wrapped in save/restore (`q`/`Q`) brackets so they never leak into surrounding state — the rest of the page renders exactly as before.

## Status

Beta. Production-grade for font-size and color overrides on existing text runs.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade.

For low-level access, see <https://pdfluent.com/docs>.

## Licensing

- Free for evaluation, development, and testing
- Production use requires a valid PDFluent commercial license
- See [LICENSE](LICENSE) for terms
- Contact <sales@pdfluent.com> for pricing
