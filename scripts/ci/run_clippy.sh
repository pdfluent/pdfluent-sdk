#!/usr/bin/env bash
set -euo pipefail
# Pre-existing hayro-jbig2 baseline: allow(missing_docs) is declared in
# crates/hayro-jbig2/src/lib.rs and crates/hayro-jpeg2000/src/lib.rs — not suppressed here.
# Arguments are crate names; without them, the whole workspace. See run_build.sh.
if [ "$#" -eq 0 ]; then pkgs=(--workspace --exclude pdf-desktop --exclude xfa-wasm)
else pkgs=(); for crate in "$@"; do pkgs+=(-p "$crate"); done; fi
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo clippy "${pkgs[@]}" -- -D warnings
