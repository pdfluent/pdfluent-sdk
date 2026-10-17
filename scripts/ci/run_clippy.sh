#!/usr/bin/env bash
set -euo pipefail
# Pre-existing hayro-jbig2 baseline: allow(missing_docs) is declared in
# crates/hayro-jbig2/src/lib.rs and crates/hayro-jpeg2000/src/lib.rs — not suppressed here.
# Arguments are crate names; without them, the whole workspace. See run_build.sh.
#
# --all-targets, because without it this lane reads library code only and every
# `#[cfg(test)]` module and `tests/` file in the workspace goes unlinted. The
# wave on the bought machine compiles those under `RUSTFLAGS: -D warnings`, so
# a warning in test code is an error there and nothing at all here: that gap is
# how `formcalc-interpreter` and `pdfluent-jbig2` stopped compiling as tests and
# stayed red for months, invisible while the suites queued (#361).
if [ "$#" -eq 0 ]; then pkgs=(--workspace --exclude pdf-desktop --exclude xfa-wasm)
else pkgs=(); for crate in "$@"; do pkgs+=(-p "$crate"); done; fi
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo clippy "${pkgs[@]}" --all-targets -- -D warnings
