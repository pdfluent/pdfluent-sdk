#!/usr/bin/env bash
set -euo pipefail
# With no arguments: the whole workspace, which is what CI runs on master.
# With crate names: only those, which is what the landing lane runs -- see
# scripts/ci/touched_crates.py. The flags live here either way, so the two
# shapes cannot drift apart.
if [ "$#" -eq 0 ]; then pkgs=(--workspace --exclude pdf-desktop --exclude xfa-wasm)
else pkgs=(); for crate in "$@"; do pkgs+=(-p "$crate"); done; fi
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo check "${pkgs[@]}"
