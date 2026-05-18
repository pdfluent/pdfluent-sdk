# xfa-js-sandboxed

Sandboxed FormCalc / XFA-JS interpreter used by `pdf-xfa` and the PDFluent runtime.

This crate exposes a minimal, hardened JavaScript-like sandbox for evaluating XFA form expressions (per XFA 3.3 §13) and FormCalc scripts (per XFA 3.3 §25). Designed for embedding into PDF processing pipelines that need to evaluate user-supplied form scripts without giving them filesystem or network access.

## Usage

This crate is intended for internal consumption by other PDFluent crates. The public API surface is documented in `src/lib.rs`. See the `pdfluent` umbrella crate for the supported high-level entry points.

## License

MIT — see [`LICENSE`](./LICENSE).
