#!/usr/bin/env bash
set -euo pipefail
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo check --workspace --exclude pdf-desktop --exclude xfa-wasm

# `cargo check` does not build `#[cfg(test)]`, so a test module that does not
# compile passes this gate and fails in CI. That happened twice on 29-08: a
# renamed helper whose definition was dropped in a merge, and two tests
# asserting a URL form that had just changed. Both were caught by the runner,
# fifteen minutes later, on a machine somebody was waiting for.
#
# `--no-run` compiles the test targets without executing them, which is the
# cheap half: the running is what takes minutes, the compiling is what catches
# this.
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo test --no-run --workspace --exclude pdf-desktop --exclude xfa-wasm
