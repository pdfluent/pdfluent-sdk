#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The crates the workspace build excludes, checked and tested natively when a
# landing reaches them (#623; touched_crates.py CHECKED_WHEN_REACHED).
#
#   bash scripts/ci/run_excluded_dependents.sh <crate>...
#
# `cargo check --tests` first: the cheap question, and the one that fails with a
# compiler's message rather than a test's. Then the crate's own native tests --
# the source-level guards among them, which is where the wasm bundle's std rule
# lives. No arguments: nothing reached, nothing to do, exit 0.
set -uo pipefail
[ "$#" -gt 0 ] || { echo "run_excluded_dependents: no excluded crate reached"; exit 0; }
for crate in "$@"; do
  echo "=== $crate: cargo check --tests"
  CARGO_TERM_COLOR=always cargo check -p "$crate" --tests || exit 1
  echo "=== $crate: cargo test"
  CARGO_TERM_COLOR=always RUST_BACKTRACE=1 cargo test -p "$crate" -- --test-threads=4 || exit 1
done
