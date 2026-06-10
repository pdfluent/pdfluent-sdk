#!/usr/bin/env bash
set -euo pipefail
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo test --workspace --exclude pdf-desktop --exclude xfa-wasm --release \
    --config 'profile.release.panic="unwind"' -- --test-threads=4
