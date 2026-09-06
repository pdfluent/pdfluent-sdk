# pdfluent-ccitt

A pure-Rust **CCITT** decoder for Group 3 and Group 4 facsimile-encoded
bitmap images, used to decode CCITT-compressed images embedded in PDF
documents.

`pdfluent-ccitt` is one of the open-source foundation crates used by the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK. It is published as
**MIT OR Apache-2.0** so it can also be used standalone.

This crate is a **fork of [`hayro-ccitt`](https://github.com/LaurenzV/hayro)**
by Laurenz Stampfl. The fork is maintained by PDFluent for coordinated
exact-version pinning across the workspace.

If you do not need PDFluent's changes, upstream
[`hayro-ccitt`](https://github.com/LaurenzV/hayro) is the better choice: it is the original,
it is actively maintained, and it does not carry our requirements.

## What this crate gives you

- Decode CCITT G3 / G4 (one-dimensional and two-dimensional) bitmap data
  (the `/Filter /CCITTFaxDecode` PDF stream)
- Pure Rust — memory-safe, no C dependency, compiles to WebAssembly

CCITT is the dominant compression scheme for monochrome scanned documents
in older PDFs and for fax workflows.

What this crate is **not**:
- Not a CCITT encoder
- Not a generic image processing library

## Usage

This crate is most often invoked transitively via PDFluent's PDF stream
filtering layer. Direct usage:

```toml
[dependencies]
pdfluent-ccitt = "0.2"
```

For PDF-level rendering, prefer the
[`pdfluent`](https://crates.io/crates/pdfluent) facade.

## License

**MIT OR Apache-2.0** — preserved from upstream `hayro-ccitt`.

```
Copyright (c) Laurenz Stampfl (hayro-ccitt)
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream hayro: <https://github.com/LaurenzV/hayro>
