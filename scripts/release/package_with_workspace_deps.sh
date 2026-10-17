#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# package_with_workspace_deps.sh — `cargo package` a crate TOGETHER WITH every
# workspace crate it depends on, in one invocation.
#
# WHY ONE INVOCATION
#
# `cargo package -p pdfluent` resolves pdfluent's workspace dependencies
# (`pdf-annot =1.1.0`, ...) from crates.io, where 1.1.0 does not exist yet
# before publication — so any pre-publication run fails by construction
# (release-smoke run 37451394856: "failed to select a version for the
# requirement pdf-annot"). Cargo 1.94 resolves the unpublished workspace
# versions locally when the crates are selected together (multi-package
# packaging with the local-registry overlay), so this selects the crate under
# test together with its transitive workspace dependencies.
#
# THE SET comes from scripts/release/topological_publish_order.py --for CRATE:
# the crate and its transitive workspace dependencies in publish order, the
# same source the publish uses. `--print-only` prints that selection, one
# crate per line, and packages nothing (for tests and for reading it by hand).
#
# Usage:
#   scripts/release/package_with_workspace_deps.sh <crate> [--print-only]
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

PRINT_ONLY=false
CRATE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --print-only) PRINT_ONLY=true; shift ;;
    -h|--help) sed -n '2,29p' "$0"; exit 0 ;;
    -*) echo "package_with_workspace_deps: unknown flag: $1" >&2; exit 2 ;;
    *)
      if [ -n "${CRATE}" ]; then
        echo "package_with_workspace_deps: one crate, not '${CRATE}' and '$1'" >&2
        exit 2
      fi
      CRATE="$1"; shift ;;
  esac
done
if [ -z "${CRATE}" ]; then
  echo "usage: scripts/release/package_with_workspace_deps.sh <crate> [--print-only]" >&2
  exit 2
fi

# Exits 2 naming the crate when it is unknown or not publishable; that is
# the refusal, and errexit carries it out (a plain assignment, not `local`).
ORDER="$(python3 "${SCRIPT_DIR}/topological_publish_order.py" --for "${CRATE}")"

NAMES=()
while IFS= read -r LINE; do
  NAME="${LINE%% *}"
  if [ -n "${NAME}" ]; then
    NAMES+=("${NAME}")
  fi
done <<<"${ORDER}"
if [ "${#NAMES[@]}" -eq 0 ]; then
  echo "package_with_workspace_deps: ${CRATE} selects nothing — refusing to package the world" >&2
  exit 1
fi

if "${PRINT_ONLY}"; then
  printf '%s\n' "${NAMES[@]}"
  exit 0
fi

ARGS=()
for NAME in "${NAMES[@]}"; do
  ARGS+=(-p "${NAME}")
done
cargo package "${ARGS[@]}" --no-verify --allow-dirty
