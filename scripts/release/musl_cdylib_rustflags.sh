#!/usr/bin/env bash
# musl_cdylib_rustflags.sh — source before a musl CDYLIB build, never a CLI one.
#
# rustc cannot produce a `cdylib` for a musl target under the default static
# CRT ("target does not support these crate types"), so every musl `.so`/`.node`
# — Python wheels, C API libs, the JNI lib, napi binaries — links musl libc
# dynamically. That is also what the musllinux wheel policy expects.
#
# The flag lives HERE rather than in `.cargo/config.toml` because that file
# cannot tell a cdylib from a binary: with the flag on the whole musl target
# the musl CLI came out dynamically linked, and the Docker image (Debian, no
# ld-musl) plus every "runs anywhere" user broke. The CLI builds must stay
# static, so they must never source this.
#
# Usage (from the repository root; in a subshell so later builds are unaffected):
#   ( source scripts/release/musl_cdylib_rustflags.sh \
#     && maturin build --zig --target x86_64-unknown-linux-musl ... )
#
# Env RUSTFLAGS replace the config's rustflags outright, so the remap block is
# sourced from release_rustflags.sh beside this file: sourcing this must never
# drop the private-path scrubbing the publish constitution requires. Held by
# test_musl_crt_split.py.
# shellcheck disable=SC1091 # resolved beside this file at source time.
source "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)/release_rustflags.sh" ""
# shellcheck disable=SC2086
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-feature=-crt-static"
