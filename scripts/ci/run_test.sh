#!/usr/bin/env bash
set -euo pipefail
# Arguments are crate names; without them, the whole workspace. See run_build.sh.
# THE PROFILE FOLLOWS THE SHAPE, AND THE TWO SHAPES HAVE DIFFERENT CALLERS.
#
# With no arguments this is the whole workspace, and that is the shape
# `.gitlab-ci.yml` runs as `quality:cargo-test` -- required on a release tag,
# and the gate the publishing jobs depend on. That one keeps the shipping
# profile: a regression that only shows up under `lto = "fat"` and
# `opt-level = "z"` must not be able to pass the tag pipeline. (codex, #1879)
#
# With crate names this is the landing lane, and there it uses the profile the
# rest of the lane uses -- see below.
if [ "$#" -eq 0 ]; then
    pkgs=(--workspace --exclude pdf-desktop --exclude xfa-wasm)
    profile=(--release --config 'profile.release.panic="unwind"')
else
    pkgs=(); for crate in "$@"; do pkgs+=(-p "$crate"); done
    profile=()
fi
# THE LANE'S OWN PROFILE, NOT A THIRD ONE (#523).
#
# The landing lane used to run `--release --config
# 'profile.release.panic="unwind"'` as well, and that is three separate builds
# of every dependency in one landing: the dev profile for `run_build.sh` and
# `run_clippy.sh`, plain `--release` for the visual-regression gate, and this
# one -- which the `--config` override fingerprints apart from plain release, so
# it shares nothing with either.
#
# What it was paying for is in Cargo.toml: the release profile of this workspace
# is the SHIPPING profile -- `lto = "fat"`, `codegen-units = 1`,
# `opt-level = "z"` -- tuned for the size of what we publish, which is the most
# expensive way there is to compile a test. Measured on a pdf-manip landing:
# 1369 s of a 38-minute landing.
#
# The wave already tests in the dev profile (`cargo test --workspace` in
# ci-ephemeral.yml), so this is not a lower standard, it is the same one; and
# the panic override goes with it, since `panic = "abort"` is a release setting
# and the dev profile unwinds.
CARGO_TERM_COLOR=always RUST_BACKTRACE=1 \
  cargo test "${pkgs[@]}" "${profile[@]}" -- --test-threads=4
