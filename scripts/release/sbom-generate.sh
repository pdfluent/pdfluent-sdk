#!/usr/bin/env bash
# sbom-generate.sh — CycloneDX SBOM generator for PDFluent publish artifacts.
#
# Governed by: docs/release/sbom_protocol.md
# See also:    docs/release/PUBLISH_PROTOCOL.md
#
# Produces one CycloneDX JSON SBOM per publish-eligible crate (and optionally a
# workspace-rollup SBOM), under `dist/sbom/` by default. The first CI run that
# generates these uploads them as artifacts; subsequent runs may use `--check`
# to detect dependency-graph drift against a committed baseline.
#
# Usage:
#   scripts/release/sbom-generate.sh [OPTIONS] [CRATE...]
#
# Options:
#   --out DIR        Output dir (default: dist/sbom).
#   --workspace      Also generate a workspace-rollup SBOM (workspace.cdx.json).
#   --tool-install   `cargo install --locked --version X.Y.Z cargo-cyclonedx` if missing.
#   --check          Compare each generated SBOM against docs/release/sbom_baselines/
#                    and exit non-zero on drift.
#   -h, --help       Show this help.
#
# CRATE...           Optional positional list of crate names. If omitted, every
#                    publish-eligible crate (publish != false) is processed.
#
# Pinned tool version:  cargo-cyclonedx 0.5.7
# Reason for vendoring: CI must not depend on an unpinned `cargo install`; CVE
# scanners and procurement teams need a deterministic SBOM generator version.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="dist/sbom"
WORKSPACE_ROLLUP=0
TOOL_INSTALL=0
CHECK_BASELINE=0
CYCLONEDX_VERSION="0.5.7"   # pinned; bump only with a baseline refresh

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)            OUT_DIR="$2"; shift 2 ;;
    --workspace)      WORKSPACE_ROLLUP=1; shift ;;
    --tool-install)   TOOL_INSTALL=1; shift ;;
    --check)          CHECK_BASELINE=1; shift ;;
    -h|--help)        sed -n '/^# Usage:/,/^# Pinned tool/p' "$0"; exit 0 ;;
    --*)              echo "unknown option: $1" >&2; exit 2 ;;
    *)                break ;;
  esac
done

# --- 1) tool presence -------------------------------------------------------
if [[ $TOOL_INSTALL -eq 1 ]] && ! command -v cargo-cyclonedx >/dev/null 2>&1; then
  echo "[sbom] installing cargo-cyclonedx v${CYCLONEDX_VERSION} (locked)..."
  cargo install --locked --version "${CYCLONEDX_VERSION}" cargo-cyclonedx
fi
if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
  echo "[sbom] ERROR: cargo-cyclonedx not found." >&2
  echo "[sbom]        re-run with --tool-install or run:" >&2
  echo "[sbom]        cargo install --locked --version ${CYCLONEDX_VERSION} cargo-cyclonedx" >&2
  exit 1
fi
TOOL_VER="$(cargo cyclonedx --version 2>/dev/null | awk '{print $NF}')"
echo "[sbom] cargo-cyclonedx version: ${TOOL_VER:-unknown}"

# --- 2) target crate list ---------------------------------------------------
publish_eligible_crates() {
  for c in crates/*/Cargo.toml; do
    if ! grep -qE '^publish *= *false' "$c"; then
      basename "$(dirname "$c")"
    fi
  done
}
CRATES=("$@")
if [[ ${#CRATES[@]} -eq 0 ]]; then
  mapfile -t CRATES < <(publish_eligible_crates)
fi
mkdir -p "$OUT_DIR"
echo "[sbom] generating SBOMs for ${#CRATES[@]} crate(s) → $OUT_DIR/"

# --- 3) per-crate generation -----------------------------------------------
# cargo-cyclonedx writes `bom.json` next to each crate's Cargo.toml; we collect
# them under $OUT_DIR with the crate name prefixed.
fail=0
for crate in "${CRATES[@]}"; do
  cdir="crates/$crate"
  [[ -f "$cdir/Cargo.toml" ]] || { echo "[sbom] skip (no Cargo.toml): $crate"; continue; }
  rm -f "$cdir/bom.json"
  if ! cargo cyclonedx --format json -p "$crate" >/dev/null 2>"$OUT_DIR/.${crate}.err"; then
    echo "[sbom] FAIL: $crate" >&2
    sed 's/^/    /' "$OUT_DIR/.${crate}.err" >&2 || true
    fail=$((fail+1))
    continue
  fi
  if [[ -f "$cdir/bom.json" ]]; then
    mv "$cdir/bom.json" "$OUT_DIR/${crate}.cdx.json"
    echo "[sbom] OK   : ${crate}.cdx.json ($(wc -c <"$OUT_DIR/${crate}.cdx.json") B)"
  else
    echo "[sbom] WARN : $crate produced no bom.json"
    fail=$((fail+1))
  fi
done
rm -f "$OUT_DIR"/.*.err 2>/dev/null || true

# --- 4) optional workspace rollup ------------------------------------------
if [[ $WORKSPACE_ROLLUP -eq 1 ]]; then
  echo "[sbom] workspace rollup → $OUT_DIR/workspace.cdx.json"
  rm -f bom.json
  cargo cyclonedx --all --format json >/dev/null 2>&1 || true
  [[ -f bom.json ]] && mv bom.json "$OUT_DIR/workspace.cdx.json"
fi

# --- 5) optional drift check against committed baselines -------------------
if [[ $CHECK_BASELINE -eq 1 ]]; then
  BASELINE_DIR="docs/release/sbom_baselines"
  drift=0
  for cdx in "$OUT_DIR"/*.cdx.json; do
    nm="$(basename "$cdx")"
    base="$BASELINE_DIR/$nm"
    if [[ ! -f "$base" ]]; then
      echo "[sbom] NEW (no baseline): $nm"
      drift=$((drift+1)); continue
    fi
    # Compare dependency lists semantically (ignore timestamp/serialNumber).
    if ! python3 -c "
import json, sys
a=json.load(open('$base')); b=json.load(open('$cdx'))
for k in ('serialNumber','metadata'):
    a.pop(k,None); b.pop(k,None)
sys.exit(0 if a==b else 3)
" 2>/dev/null; then
      echo "[sbom] DRIFT: $nm differs from baseline"
      drift=$((drift+1))
    fi
  done
  if [[ $drift -gt 0 ]]; then
    echo "[sbom] $drift SBOM(s) differ from baseline." >&2
    echo "[sbom] If intentional: cp dist/sbom/<name>.cdx.json docs/release/sbom_baselines/" >&2
    exit 3
  fi
  echo "[sbom] baseline check PASS."
fi

echo "[sbom] DONE: $(ls -1 "$OUT_DIR"/*.cdx.json 2>/dev/null | wc -l | tr -d ' ') SBOM(s) in $OUT_DIR/"
[[ $fail -eq 0 ]] || exit 4
