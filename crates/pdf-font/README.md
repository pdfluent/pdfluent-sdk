# pdf-font

PDF font handling — Type1 and CFF parsing, CMap parsing, PostScript scanning, ToUnicode mapping, and Standard 14 font fallbacks.

This crate is part of the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

**Free for evaluation. Production use requires a valid license.**

## What it does

Provides the font-handling layer used by both rendering and text extraction: parses embedded Type 1 / CFF / TrueType fonts, resolves CMap tables and ToUnicode mappings, and supplies metric data for the Standard 14 base fonts when fonts are missing.

## Status

Beta. Production-grade for common PDF font configurations. Coverage of edge-case CFF and Type 1 variants is being improved continuously.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade or [`pdf-engine`](https://crates.io/crates/pdf-engine).

## Origin

This crate was forked from [`hayro-font`](https://github.com/LaurenzV/hayro) (with `hayro-cmap` and `hayro-postscript` merged in) by Laurenz Stampfl, and has been substantially extended. Original Apache-2.0/MIT contributions are attributed in `THIRD_PARTY_LICENSES.txt`; PDFluent extensions are governed by the PDFluent Commercial License.

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
