//! Integration-test entry point for `tests/web_examples`.
//!
//! Cargo discovers each `tests/*.rs` file as an integration-test binary. We
//! point it at `tests/web_examples/mod.rs` which declares the per-snippet
//! modules. See issue #1240.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[path = "web_examples/mod.rs"]
mod web_examples;
