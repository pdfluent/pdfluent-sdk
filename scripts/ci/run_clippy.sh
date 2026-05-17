#!/usr/bin/env bash
set -euo pipefail
# Pre-existing hayro-jbig2 baseline: allow(missing_docs) is declared in
# crates/hayro-jbig2/src/lib.rs and crates/hayro-jpeg2000/src/lib.rs — not suppressed here.
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo clippy --workspace --exclude pdf-desktop --exclude xfa-wasm -- -D warnings
