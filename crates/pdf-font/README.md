# pdf-font

> **This crate is a fork.** It began as [`hayro-font`](https://github.com/LaurenzV/hayro) at version 0.5.0 and is
> maintained separately here. Upstream is actively developed and is the better
> choice if you do not need the changes made for PDFluent. Licensed as upstream
> (Apache-2.0 OR MIT); see `NOTICE` and `THIRD_PARTY_LICENSES.txt` for the full
> attribution.

PDF font handling — Type1 and CFF parsing, CMap parsing, PostScript scanning, ToUnicode mapping, and Standard 14 font fallbacks.

It merges `hayro-cmap` and `hayro-postscript` in, and is used by the
[PDFluent](https://pdfluent.com) Rust PDF SDK on the upstream terms:
**Apache-2.0 OR MIT**, for the whole crate, PDFluent's extensions included.

## What it does

Provides the font-handling layer used by both rendering and text extraction: parses embedded Type 1 / CFF / TrueType fonts, resolves CMap tables and ToUnicode mappings, and supplies metric data for the Standard 14 base fonts when fonts are missing.

## Status

Beta. Production-grade for common PDF font configurations. Coverage of edge-case CFF and Type 1 variants is being improved continuously.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade or [`pdf-engine`](https://crates.io/crates/pdf-engine).

## License

**Apache-2.0 OR MIT** — preserved from upstream `hayro-font`. Relicensing is
prohibited, so how far this fork is extended does not move it: the extensions
are licensed on the same terms as the code they were built on. The upstream
attributions are in `THIRD_PARTY_LICENSES.txt` in the repository root.

```
Copyright (c) Laurenz Stampfl (hayro-font, hayro-cmap, hayro-postscript)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
