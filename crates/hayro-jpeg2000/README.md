# pdfluent-jpeg2000

A pure-Rust **JPEG 2000** image decoder, used to decode JPEG 2000-compressed
images embedded in PDF documents.

`pdfluent-jpeg2000` is one of the open-source foundation crates used by the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK. It is published as
**MIT OR Apache-2.0** so it can also be used standalone.

This crate is a **fork of [`hayro-jpeg2000`](https://github.com/LaurenzV/hayro)**
by Laurenz Stampfl. The fork is maintained by PDFluent for coordinated
exact-version pinning across the workspace.

If you do not need PDFluent's changes, upstream
[`hayro-jpeg2000`](https://github.com/LaurenzV/hayro) is the better choice: it is the original,
it is actively maintained, and it does not carry our requirements.

## What this crate gives you

- Decode JPEG 2000-encoded image data (the `/Filter /JPXDecode` PDF stream)
- Pure Rust — memory-safe, no C dependency, compiles to WebAssembly

JPEG 2000 is used in PDFs for high-quality continuous-tone color images
(scanned photographs, archival imaging) and for some PDF/A workflows where
better color fidelity than JPEG is desired.

What this crate is **not**:
- Not a JPEG 2000 encoder
- Not a generic image processing library

## Usage

This crate is most often invoked transitively via PDFluent's PDF stream
filtering layer. Direct usage:

```toml
[dependencies]
pdfluent-jpeg2000 = "0.4"
```

For PDF-level rendering, prefer the
[`pdfluent`](https://crates.io/crates/pdfluent) facade.

## License

**MIT OR Apache-2.0** — preserved from upstream `hayro-jpeg2000`.

```
Copyright (c) Laurenz Stampfl (hayro-jpeg2000)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
