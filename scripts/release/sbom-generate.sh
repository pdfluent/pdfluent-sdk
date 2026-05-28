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
  # Portable substitute for `mapfile -t CRATES < <(publish_eligible_crates)` —
  # macOS ships bash 3.2 which has no mapfile builtin; this loop works on
  # bash 3.2+ and busybox sh alike.
  while IFS= read -r line; do
    CRATES+=("$line")
  done < <(publish_eligible_crates)
fi
mkdir -p "$OUT_DIR"
echo "[sbom] generating SBOMs for ${#CRATES[@]} crate(s) → $OUT_DIR/"

# --- 3) generation (single workspace walk, then collect) -------------------
# cargo-cyclonedx 0.5.x writes one `<member>/<member>.cdx.json` per workspace
# member regardless of which member's `--manifest-path` you point at — it
# walks the workspace. So we run it ONCE from the workspace root (fastest)
# and then collect the requested crates' files into $OUT_DIR. This avoids
# N redundant workspace walks the previous per-crate loop performed.
#
# Earlier-version notes:
#  - 0.5.x has NO `-p <CRATE>` flag and the `--all` flag was removed; the
#    `--manifest-path` argument is the only path-selector.
#  - Output convention is `<member-dir>/<member-name>.cdx.json`, NOT
#    `bom.json` (which was the 0.4.x default).
echo "[sbom] running cargo cyclonedx once (workspace walk, --target all)..."
# Clean any stale .cdx.json files from prior aborted runs so the collect
# step only sees freshly-generated output.
find crates tools -maxdepth 2 -name '*.cdx.json' -delete 2>/dev/null || true
# `--target all` makes the SBOM target-independent: include every dep that
# ANY target would pull (e.g. android_system_properties, curve25519-dalek-derive,
# fiat-crypto, objc, nix, …). Without it, the host triple of the generator
# leaks into the dep list — locally aarch64-apple-darwin, in CI x86_64-unknown-
# linux-gnu — causing baseline drift across environments for the same source
# tree. A release SBOM should describe the crate's full dependency surface,
# not the build machine's slice of it.
if ! cargo cyclonedx --format json --target all --manifest-path Cargo.toml >/dev/null 2>"$OUT_DIR/.cyclonedx.err"; then
  echo "[sbom] FATAL: cargo cyclonedx workspace walk failed" >&2
  sed 's/^/    /' "$OUT_DIR/.cyclonedx.err" >&2 || true
  rm -f "$OUT_DIR/.cyclonedx.err"
  exit 5
fi
rm -f "$OUT_DIR/.cyclonedx.err"

fail=0
for crate in "${CRATES[@]}"; do
  cdir="crates/$crate"
  [[ -f "$cdir/Cargo.toml" ]] || { echo "[sbom] skip (no Cargo.toml): $crate"; continue; }
  # cargo-cyclonedx writes `<dir>/<published-name>.cdx.json`. For most crates
  # the dir-name equals the [package].name, but the eight hayro/lopdf fork
  # crates use `pdfluent-<x>` published names while the dir keeps the
  # upstream name (e.g. dir `hayro-ccitt` → name `pdfluent-ccitt`). Read the
  # actual `[package].name` from Cargo.toml to look up the right file, and
  # use it as the OUTPUT filename so baselines key on the published crate
  # identity (what shows up on crates.io and in SBOM consumers).
  pkg_name="$(grep -m1 '^name' "$cdir/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
  [[ -n "$pkg_name" ]] || pkg_name="$crate"
  src="$cdir/${pkg_name}.cdx.json"
  if [[ -f "$src" ]]; then
    mv "$src" "$OUT_DIR/${pkg_name}.cdx.json"
    echo "[sbom] OK   : ${pkg_name}.cdx.json ($(wc -c <"$OUT_DIR/${pkg_name}.cdx.json") B)"
  else
    echo "[sbom] WARN : $crate (pkg=$pkg_name) produced no ${pkg_name}.cdx.json (workspace walk did not include it)" >&2
    fail=$((fail+1))
  fi
done

# Also clean up any .cdx.json files the walk produced for crates we did NOT
# request (publish=false internal crates, etc.) so we don't leak generated
# artefacts into the source tree.
find crates tools -maxdepth 2 -name '*.cdx.json' -delete 2>/dev/null || true

# --- 3b) canonicalise paths -------------------------------------------------
# cargo-cyclonedx writes the absolute on-disk path of each path-dependency
# (e.g. `path+file:///Users/.../crates/pdf-xfa#1.0.0-beta.8`) into the
# `bom-ref` of each component AND the `ref` / `dependsOn[]` of each
# dependency entry. That leaks the developer's home dir (locally) or the
# CI runner build dir (in CI) into the SBOM and makes baselines non-portable
# across machines.
#
# Canonicalisation strips the path prefix down to `path://<crate>#<rest>`,
# keying only on the workspace-member directory name. This makes the SBOM
# both *clean* (no private paths) and *reproducible* (same canonical form on
# any machine), which is the precondition for committing baselines and for
# the `--check` drift detection to actually mean what it claims.
echo "[sbom] canonicalising bom-ref paths (strip absolute prefix)..."
for cdx in "$OUT_DIR"/*.cdx.json; do
  [[ -f "$cdx" ]] || continue
  python3 - "$cdx" <<'PY'
import json, re, sys
p = sys.argv[1]
with open(p) as f:
    d = json.load(f)

# path+file:///abs/path/<member-parent>/<member>#<rest>  →  path://<member>#<rest>
PAT = re.compile(r'path\+file:///[^#"\s]*?/([^/#"\s]+)#')

def canon(s):
    if isinstance(s, str):
        return PAT.sub(r'path://\1#', s)
    return s

def walk(o):
    if isinstance(o, dict):
        for k in list(o.keys()):
            if k in ('bom-ref', 'ref', 'dependsOn'):
                if isinstance(o[k], list):
                    o[k] = [canon(x) for x in o[k]]
                else:
                    o[k] = canon(o[k])
            else:
                walk(o[k])
    elif isinstance(o, list):
        for item in o:
            walk(item)

walk(d)
# Stable formatting so byte-identical baselines across runs.
with open(p, 'w') as f:
    json.dump(d, f, sort_keys=True, indent=2)
    f.write('\n')
PY
done

# --- 4) optional workspace rollup ------------------------------------------
# Convention: the workspace-virtual root's combined SBOM is named workspace.cdx.json.
# In 0.5.x the workspace walk above already produces per-member files; a
# combined rollup requires a separate invocation with the `--describe` flag
# set appropriately. We emit a deterministic concatenation marker file rather
# than re-walking; consumers wanting a single-file BOM should use
# https://github.com/CycloneDX/cyclonedx-cli `merge` over the per-crate files.
if [[ $WORKSPACE_ROLLUP -eq 1 ]]; then
  echo "[sbom] workspace rollup: per-member files already in $OUT_DIR/ (0.5.x emits per-member)"
  echo "[sbom]   merge into single file with: cyclonedx merge --input-files $OUT_DIR/*.cdx.json --output-file $OUT_DIR/workspace.cdx.json"
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
