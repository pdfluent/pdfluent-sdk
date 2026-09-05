#!/usr/bin/env bash
set -euo pipefail
# Arguments are crate names; without them, the whole workspace. See run_build.sh.
if [ "$#" -eq 0 ]; then pkgs=(--workspace --exclude pdf-desktop --exclude xfa-wasm)
else pkgs=(); for crate in "$@"; do pkgs+=(-p "$crate"); done; fi
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo test "${pkgs[@]}" --release \
    --config 'profile.release.panic="unwind"' -- --test-threads=4
