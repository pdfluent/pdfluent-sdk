#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Does the SDK still compile for the browser? (#483)
#
# WHY A LANE NEEDS THIS AT ALL
#
# Nothing in either local lane compiled anything for wasm32 until this file. The
# fast lane and the landing lane build for the host, and the only wasm compile in
# the repository was a step of the two-hourly wave -- so a change that breaks the
# browser build passed every gate, landed, and turned master red up to two hours
# later, with the author gone. That is what #383 did: it made
# `pdf-substitute-fonts` a `cfg(not(target_arch = "wasm32"))` dependency, which
# left `FontSource::BundledSubstitute` unconstructible on wasm32, and a dead
# variant under `-D warnings` is a compile error for the whole crate.
#
# WHY THE WAVE'S COMMAND, VERBATIM
#
# `cargo check -p pdf-manip --target wasm32-unknown-unknown` -- the narrow check
# the ticket first asked for -- does not compile at all, for an unrelated reason:
# `getrandom` 0.4 needs a backend chosen for wasm32, and what supplies it is
# feature unification across the `pdfluent` graph. Alone, pdf-manip fails with
# four E0425s out of getrandom before it ever reaches our own code, which is a
# gate that is red about the wrong thing on the first day.
#
# So this runs what .github/workflows/ci-ephemeral.yml runs, argument for
# argument. Two spellings of one question drift; one does not.
#
# WHY RUSTFLAGS
#
# The wave sets `RUSTFLAGS: -D warnings` for the whole job, and that is the half
# that turns a dead variant into a red. Without it this check is green on exactly
# the code that broke master. Setting RUSTFLAGS here REPLACES the target section
# of .cargo/config.toml (cargo does not merge the two), so this compiles without
# `+simd128` and without the path remapping -- which is what the wave does too,
# for the same reason, and neither matters to a `check` that produces no
# artefact.
#
# The target is spelled out at both places it appears rather than held in a
# variable: scripts/ci/test_the_lane_compiles_for_the_browser.py compares this
# command with the wave's word for word, and a variable is a word it cannot
# resolve.
set -uo pipefail

# NOT a plain `rustup target add`: that lands on the DEFAULT toolchain, while
# rust-toolchain.toml pins the one cargo will actually use. A target on the wrong
# toolchain answers "can't find crate for `core`" -- the trap of #1539/#1547, and
# the wave's own step carries the same two lines for it.
if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown \
      --toolchain "$(rustup show active-toolchain | cut -d' ' -f1)" || {
    echo "run_wasm_check: could not add wasm32-unknown-unknown to the active toolchain." >&2
    echo "  NOTHING WAS CHECKED about the browser build." >&2
    exit 2
  }
else
  echo "run_wasm_check: no rustup on this machine, so the wasm32-unknown-unknown" >&2
  echo "  std cannot be installed or verified." >&2
  echo "  NOTHING WAS CHECKED about the browser build." >&2
  exit 2
fi

CARGO_TERM_COLOR=always RUSTFLAGS="-D warnings" \
  cargo check --target wasm32-unknown-unknown -p pdfluent --tests
