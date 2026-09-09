# formcalc-interpreter

FormCalc expression evaluator — Adobe's domain-specific language for XFA forms. **Experimental — part of the XFA stack.**

This crate is part of [PDFluent](https://pdfluent.com), a pure-Rust PDF SDK. Source: <https://github.com/pdfluent/pdfluent-sdk>.

**AGPL-3.0 or a commercial licence**, at your option — see the Licence section below.

## What it does

Implements the FormCalc language used inside XFA forms for client-side calculations, validation, and reactive bindings. Supports the standard built-in functions, type coercion rules, and SOM-expression resolution against the XFA DOM.

## Status

Experimental — XFA support is under active development. See [`pdf-xfa`](https://crates.io/crates/pdf-xfa) for current status.

## Usage

Most users do not depend on this crate directly. Use the [`pdfluent`](https://crates.io/crates/pdfluent) facade.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this crate.

## Links

- Main crate: <https://crates.io/crates/pdfluent>
- Documentation: <https://pdfluent.com/docs>
