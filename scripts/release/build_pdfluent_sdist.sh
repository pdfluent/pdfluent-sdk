#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# build_pdfluent_sdist.sh — build the pdfluent sdist, refusing old maturin.
#
# maturin below 1.15 is refused on this layout: crates/pdf-python is its own
# workspace nested under crates/, with path dependencies in the parent
# workspace, and the sdist root maturin computes (crates/) sits below the
# parent manifest it then strips. 1.12.6 panics with StripPrefixError there;
# 1.15.0 is the version measured to build (worktree and plain clone alike,
# same bytes), and 1.13/1.14 were never measured, so the floor is 1.15 and
# the message names 1.15.0. An older maturin fails here naming its version,
# instead of panicking halfway through the release. (It is the layout, not
# the checkout: a linked worktree and a plain clone panic identically on
# 1.12 and both build on 1.15.0.)
#
# Usage:
#   scripts/release/build_pdfluent_sdist.sh [--out DIR]
# OUT defaults to dist. Writes pdfluent-<version>.tar.gz there.
set -Eeuo pipefail

cd "$(dirname "$0")/../.."

OUT="dist"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -20; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

command -v maturin >/dev/null 2>&1 || { echo "missing on PATH: maturin" >&2; exit 1; }

HAVE="$(maturin --version 2>/dev/null || true)"
# "maturin 1.15.0" -> "1 15". Anything else unparseable refuses: a floor
# that cannot read the version cannot promise the fix is in.
SET="$(printf '%s' "$HAVE" | sed -n 's/^maturin \([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1 \2/p')"
if [[ -z "$SET" ]]; then
  echo "cannot read the maturin version (saw: ${HAVE:-<none>}); need maturin>=1.15 for the sdist (1.15.0 measured)" >&2
  exit 1
fi
set -- $SET
if [[ "$1" -lt 1 || ( "$1" -eq 1 && "$2" -lt 15 ) ]]; then
  echo "need maturin>=1.15 for the sdist (have ${HAVE}); 1.15.0 is the measured version, older releases panic with StripPrefixError on the split workspace" >&2
  exit 1
fi

mkdir -p "$OUT"
maturin sdist -m crates/pdf-python/Cargo.toml --out "$OUT"
