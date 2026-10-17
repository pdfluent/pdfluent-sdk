# xfa-js-sandboxed

Sandboxed FormCalc / XFA-JS interpreter used by `pdf-xfa` and the PDFluent runtime. Source: <https://github.com/pdfluent/pdfluent-sdk>. Source: <https://github.com/pdfluent/pdfluent-sdk>.

This crate exposes a minimal, hardened JavaScript-like sandbox for evaluating XFA form expressions (per XFA 3.3 §13) and FormCalc scripts (per XFA 3.3 §25). Designed for embedding into PDF processing pipelines that need to evaluate user-supplied form scripts without giving them filesystem or network access.

## Usage

This crate is intended for internal consumption by other PDFluent crates. The public API surface is documented in `src/lib.rs`. See the `pdfluent` umbrella crate for the supported high-level entry points.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.
