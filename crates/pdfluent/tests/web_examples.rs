//! Integration-test entry point for `tests/web_examples`.
//!
//! Cargo discovers each `tests/*.rs` file as an integration-test binary. We
//! point it at `tests/web_examples/mod.rs` which declares the per-snippet
//! modules. See issue #1240.

#[path = "web_examples/mod.rs"]
mod web_examples;
