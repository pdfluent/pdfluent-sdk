#!/usr/bin/env bash
set -euo pipefail
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo check --workspace --exclude pdf-desktop --exclude xfa-wasm
