# pdf-text-format

Text run formatting for PDFs — inject font-size and color changes into existing content streams with state isolation.

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Programmatically modifies text appearance inside PDF content streams without re-rasterising the page. Style mutations (font size, fill/stroke color) are wrapped in save/restore (`q`/`Q`) brackets so they never leak into surrounding state — the rest of the page renders exactly as before.

## Status

Font-size and colour overrides on existing text runs.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade.

For low-level access, see <https://pdfluent.com/docs>.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.
