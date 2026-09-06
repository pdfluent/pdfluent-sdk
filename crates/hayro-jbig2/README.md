# pdfluent-jbig2

A pure-Rust **JBIG2** image decoder, used to decode JBIG2-compressed images
embedded in PDF documents.

`pdfluent-jbig2` is one of the open-source foundation crates used by the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK. It is published as
**MIT OR Apache-2.0** so it can also be used standalone.

This crate is a **fork of [`hayro-jbig2`](https://github.com/LaurenzV/hayro)**
by Laurenz Stampfl. The fork is maintained by PDFluent for coordinated
exact-version pinning across the workspace.

If you do not need PDFluent's changes, upstream
[`hayro-jbig2`](https://github.com/LaurenzV/hayro) is the better choice: it is the original,
it is actively maintained, and it does not carry our requirements.

## What this crate gives you

- Decode JBIG2-encoded image data (the `/Filter /JBIG2Decode` PDF stream)
- Generic-region, text-region, and pattern-dictionary segments
- Both end-of-stream and per-segment generic decoding modes used in PDF
- Pure Rust — memory-safe, no C dependency, compiles to WebAssembly

JBIG2 is mostly used for scanned monochrome documents where it produces
significantly smaller files than CCITT G4.

What this crate is **not**:
- Not a JBIG2 encoder (we only decode what is already in PDFs)
- Not a generic image processing library

## Usage

This crate is most often invoked transitively via PDFluent's PDF stream
filtering layer. Direct usage:

```toml
[dependencies]
pdfluent-jbig2 = "0.2"
```

For PDF-level JBIG2 decoding (with the right glyph segment context handled
by the PDF reader), prefer the [`pdfluent`](https://crates.io/crates/pdfluent)
facade.

## License

**MIT OR Apache-2.0** — preserved from upstream `hayro-jbig2`.

```
Copyright (c) Laurenz Stampfl (hayro-jbig2)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
