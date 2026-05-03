# pdfluent-lopdf

A Rust library for low-level PDF document manipulation.

This crate is a fork of [`lopdf`](https://crates.io/crates/lopdf) maintained by the PDFluent team. It is the low-level PDF object model used by the [PDFluent](https://pdfluent.com) commercial Rust PDF SDK.

## License

**MIT** (preserved from the upstream `lopdf` project).

This crate remains under the MIT license — including PDFluent's modifications — so that the broader Rust PDF ecosystem can continue to benefit from it.

Original copyright: Junfeng Liu, Emulator (and contributors). PDFluent fork: Innovation Trigger BV.

See [LICENSE](LICENSE) for full text.

## Why a fork?

PDFluent maintains a small set of additions on top of upstream `lopdf`:

- AES-256 encryption (V=5, R=6, /CFM /AESV3) — required for PDF 2.0 compliance
- Performance tuning for the parse / save pipeline used by PDFluent
- Coordination with PDFluent's exact-version pinning across the workspace

We aim to upstream non-PDFluent-specific improvements where possible.

## Usage

This crate can be used standalone like any MIT-licensed library. For the full PDFluent SDK, see [`pdfluent`](https://crates.io/crates/pdfluent).

## Links

- PDFluent: <https://pdfluent.com>
- Upstream lopdf: <https://github.com/J-F-Liu/lopdf>
