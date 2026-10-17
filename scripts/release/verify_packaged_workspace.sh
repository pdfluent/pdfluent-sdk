#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# verify_packaged_workspace.sh — every publishable crate builds against its
# packaged siblings, not the registry and not the source tree.
#
# What it does:
#   1. `cargo package --workspace --locked --no-verify` (publish = false
#      members excluded), so every publishable crate produces its .crate;
#   2. unpack every target/package/<name>-<version>.crate into a scratch dir;
#   3. write a scratch Cargo.toml workspace whose [patch.crates-io] maps every
#      published-workspace crate name to its UNPACKED directory. Each crate's
#      `=1.1.0` dependency then resolves to the packaged sibling, never to
#      crates.io and never to the source tree;
#   4. `cargo build --locked` (or --offline) for every unpacked crate, in
#      topological order (topological_publish_order.py);
#   5. report per crate (ok/fail), and exit non-zero on any failure.
#
# WHY --no-verify PLUS A STANDALONE BUILD
#
# cargo 1.94's multi-crate verify ICEs on its own temporary registry:
# `no hash listed for xfa-dom-resolver v1.1.0` and the like, because the
# tmp-registry it verifies against carries no hashes. The verifying
# `cargo package --workspace --locked` is therefore unusable until cargo
# fixes the ICE; this helper packs with --no-verify and then does the build
# cargo's own verify would have done, over the unpacked .crates. When the
# ICE is fixed the route can return to plain `cargo package --workspace`.
#
# Callers: scripts/release/prepublish_crate_audit.sh (workspace route) and
# scripts/release/registry_dry_run.sh (crates channel). Both call this one
# helper so the audit and the dry run cannot drift.
#
# Usage:
#   scripts/release/verify_packaged_workspace.sh [--root DIR] [--scratch DIR]
#       [--report FILE] [--allow-dirty] [--offline] [--no-package]
#       [--keep-scratch] [--order-script PATH]
#
#   --root DIR         workspace to package (default: this repository)
#   --scratch DIR      unpack + build here (default: a fresh mktemp dir)
#   --report FILE      write the per-crate report here as well as stdout
#   --allow-dirty      pass --allow-dirty to cargo package (dry-run only;
#                      the publish path refuses a dirty tree)
#   --offline          pass --offline to the scratch builds (and lockfile);
#                      use when every third-party dependency is in the cache
#   --no-package       skip step 1 and build from the .crates already in
#                      target/package (lets a test corrupt a package and prove
#                      the build reads the package, not the tree)
#   --keep-scratch     keep an auto-created scratch dir (a --scratch dir is
#                      always kept)
#   --order-script PATH  topological order script (default: the sibling
#                      topological_publish_order.py, run with cwd --root so a
#                      fixture workspace orders its own crates)
#
# Exit codes:
#   0  every publishable crate packaged and built
#   1  at least one did not (the report says which)

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_ORDER_SCRIPT="${SCRIPT_DIR}/topological_publish_order.py"

ROOT=""
SCRATCH_ARG=""
REPORT=""
ALLOW_DIRTY=false
OFFLINE=false
NO_PACKAGE=false
KEEP_SCRATCH=false
ORDER_SCRIPT="${DEFAULT_ORDER_SCRIPT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --root) ROOT="$2"; shift 2 ;;
        --scratch) SCRATCH_ARG="$2"; shift 2 ;;
        --report) REPORT="$2"; shift 2 ;;
        --allow-dirty) ALLOW_DIRTY=true; shift ;;
        --offline) OFFLINE=true; shift ;;
        --no-package|--skip-package) NO_PACKAGE=true; shift ;;
        --keep-scratch) KEEP_SCRATCH=true; shift ;;
        --order-script) ORDER_SCRIPT="$2"; shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "$0" | head -60; exit 0 ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
done

if [[ -z "${ROOT}" ]]; then
    ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
fi
# Absolute paths throughout: the per-crate builds `cd` into SCRATCH, so a
# relative SCRATCH/REPORT would resolve from the wrong directory.
case "${ROOT}" in
    /*) ;;
    *) ROOT="$(pwd)/${ROOT}" ;;
esac
if [[ ! -d "${ROOT}" ]]; then
    echo "error: workspace root not found: ${ROOT}" >&2
    exit 2
fi
ROOT="$(cd -- "${ROOT}" && pwd)"
case "${SCRATCH_ARG}" in
    ""|/*) ;;
    *) SCRATCH_ARG="$(pwd)/${SCRATCH_ARG}" ;;
esac
case "${REPORT}" in
    ""|/*) ;;
    *) REPORT="$(pwd)/${REPORT}" ;;
esac
# The order script runs with cwd ROOT (so a fixture orders its own crates),
# but the path itself is resolved here.
case "${ORDER_SCRIPT}" in
    /*) ;;
    *) ORDER_SCRIPT="$(cd -- "$(dirname -- "${ORDER_SCRIPT}")" 2>/dev/null && pwd)/$(basename -- "${ORDER_SCRIPT}")"
        if [[ ! -f "${ORDER_SCRIPT}" ]]; then
            ORDER_SCRIPT="${DEFAULT_ORDER_SCRIPT}"
        fi
        ;;
esac

# The disk is tight: line tables only, unless the caller already chose.
: "${CARGO_PROFILE_DEV_DEBUG:=line-tables-only}"
: "${CARGO_PROFILE_TEST_DEBUG:=line-tables-only}"
export CARGO_PROFILE_DEV_DEBUG CARGO_PROFILE_TEST_DEBUG

# Facts about the workspace from one cargo metadata call: where cargo writes
# its .crates (honours CARGO_TARGET_DIR, which the shared build cache sets),
# and which members must be excluded because they are never published.
TARGET_DIR="$(cd "${ROOT}" && cargo metadata --no-deps --format-version 1 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin).get('target_directory',''))")"
if [[ -z "${TARGET_DIR}" ]]; then
    echo "error: cargo metadata reported no target directory for ${ROOT}" >&2
    exit 1
fi
PKGDIR="${TARGET_DIR}/package"

EXCLUDES=()
while IFS= read -r line; do
    case "${line}" in
        --exclude\ *) EXCLUDES+=("--exclude" "${line#--exclude }") ;;
    esac
done < <(cd "${ROOT}" && cargo metadata --no-deps --format-version 1 2>/dev/null \
    | python3 -c "
import json, sys
for p in json.load(sys.stdin)['packages']:
    if p.get('publish') == []:
        print('--exclude ' + p['name'])
")

ORDER="$(cd "${ROOT}" && python3 "${ORDER_SCRIPT}")"
if [[ -z "${ORDER}" ]]; then
    echo "error: the publish list is empty; nothing to verify" >&2
    exit 1
fi
TOTAL="$(printf '%s\n' "${ORDER}" | grep -c . || true)"

if ! ${NO_PACKAGE}; then
    # cargo does not truncate the file it packages into (#591): a smaller
    # crate over an earlier, larger one leaves the old tail behind the gzip
    # stream. The files are this run's to make.
    while read -r n v; do
        [[ -n "${n:-}" ]] || continue
        rm -f "${PKGDIR}/${n}-${v}.crate"
    done <<<"${ORDER}"
    PACK_ARGS=(--workspace --locked --no-verify "${EXCLUDES[@]}")
    if ${ALLOW_DIRTY}; then
        PACK_ARGS+=(--allow-dirty)
    fi
    PACKLOG="$(mktemp -t verify_packaged_workspace_pack.XXXXXX)"
    # The verifying `cargo package --workspace --locked` ICEs on cargo 1.94
    # ("no hash listed for xfa-dom-resolver v1.1.0" in its hashless
    # tmp-registry), so this is --no-verify plus the standalone build below.
    if ! (cd "${ROOT}" && cargo package "${PACK_ARGS[@]}" >"${PACKLOG}" 2>&1); then
        echo "error: workspace packaging failed" >&2
        tail -30 "${PACKLOG}" >&2
        rm -f "${PACKLOG}"
        exit 1
    fi
    rm -f "${PACKLOG}"
    MISSING=0
    while read -r n v; do
        [[ -n "${n:-}" ]] || continue
        if [[ ! -f "${PKGDIR}/${n}-${v}.crate" ]]; then
            echo "error: workspace packaging produced no ${n}-${v}.crate" >&2
            MISSING=1
        fi
    done <<<"${ORDER}"
    if [[ "${MISSING}" -ne 0 ]]; then
        exit 1
    fi
else
    MISSING=0
    while read -r n v; do
        [[ -n "${n:-}" ]] || continue
        if [[ ! -f "${PKGDIR}/${n}-${v}.crate" ]]; then
            echo "error: --no-package but no ${n}-${v}.crate in ${PKGDIR}" >&2
            MISSING=1
        fi
    done <<<"${ORDER}"
    if [[ "${MISSING}" -ne 0 ]]; then
        exit 1
    fi
fi

# The scratch workspace: every .crate unpacked, then one Cargo.toml whose
# [patch.crates-io] points each name at its UNPACKED directory. A packaged
# manifest carries version-only dependencies (cargo rewrote the path deps at
# package time), so the patch is what turns each `=1.1.0` pin into the
# packaged sibling -- never the registry's 1.0.0, never the source tree.
SCRATCH=""
SCRATCH_OWNED=false
if [[ -n "${SCRATCH_ARG}" ]]; then
    SCRATCH="${SCRATCH_ARG}"
    rm -rf "${SCRATCH}"
    mkdir -p "${SCRATCH}"
else
    SCRATCH="$(mktemp -d -t verify_packaged_workspace.XXXXXX)"
    SCRATCH_OWNED=true
    if ! ${KEEP_SCRATCH}; then
        trap 'rm -rf "${SCRATCH}"' EXIT
    fi
fi

while read -r n v; do
    [[ -n "${n:-}" ]] || continue
    TOP="${SCRATCH}/${n}-${v}"
    mkdir -p "${TOP}"
    tar -xzf "${PKGDIR}/${n}-${v}.crate" -C "${TOP}" --strip-components=1
done <<<"${ORDER}"

python3 - "${SCRATCH}" <<'PYEOF' >"${SCRATCH}/Cargo.toml"
import re, sys
from pathlib import Path
synth = Path(sys.argv[1])
entries = []
for member in sorted(p for p in synth.iterdir() if p.is_dir()):
    text = (member / "Cargo.toml").read_text(encoding="utf-8")
    pkg = text.split("[package]", 1)[1]
    name = re.search(r'^name\s*=\s*"([^"]+)"', pkg, re.M).group(1)
    entries.append((member.name, name))
print("[workspace]")
print('resolver = "2"')
print("members = [")
for dirname, _ in entries:
    print(f'  "{dirname}",')
print("]")
print()
print("[patch.crates-io]")
for dirname, name in entries:
    print(f'{name} = {{ path = "{dirname}" }}')
PYEOF

# Third-party versions come from the source lock, not a fresh resolve. A
# fresh `cargo generate-lockfile` picks the latest compatible versions
# (libc 0.2.190 over the locked 0.2.186), which are not in the local cache
# and need not even build with this tree. Copying the source lock and
# building --offline keeps the locked versions: the first offline build
# trims the workspace members to the 36 publishable crates and leaves the
# third-party pins alone.
USE_OFFLINE=${OFFLINE}
if [[ -f "${ROOT}/Cargo.lock" ]]; then
    cp "${ROOT}/Cargo.lock" "${SCRATCH}/Cargo.lock"
    USE_OFFLINE=true
else
    GEN_FLAGS=()
    if ${OFFLINE}; then
        GEN_FLAGS+=(--offline)
    fi
    GENLOG="$(mktemp -t verify_packaged_workspace_lock.XXXXXX)"
    if ! (cd "${SCRATCH}" && cargo generate-lockfile "${GEN_FLAGS[@]}" >"${GENLOG}" 2>&1); then
        echo "error: scratch lockfile failed" >&2
        tail -30 "${GENLOG}" >&2
        rm -f "${GENLOG}"
        exit 1
    fi
    rm -f "${GENLOG}"
fi
BUILD_FLAGS=()
if ${USE_OFFLINE}; then
    BUILD_FLAGS+=(--offline)
else
    BUILD_FLAGS+=(--locked)
fi

# Every unpacked crate in topological order, one build each, so the report
# names the crate that fails rather than the workspace that contains it.
LOGDIR="${SCRATCH}/logs"
mkdir -p "${LOGDIR}"
PASS=0
FAIL=0
FAILED=()
REPORT_LINES=()
while read -r n v; do
    [[ -n "${n:-}" ]] || continue
    BUILDLOG="${LOGDIR}/${n}.log"
    if (cd "${SCRATCH}" && cargo build "${BUILD_FLAGS[@]}" -p "${n}" >"${BUILDLOG}" 2>&1); then
        PASS=$((PASS + 1))
        REPORT_LINES+=("ok ${n} ${v}")
    else
        FAIL=$((FAIL + 1))
        FAILED+=("${n}")
        REPORT_LINES+=("FAIL ${n} ${v}")
    fi
done <<<"${ORDER}"

{
    echo "# Packaged workspace build -- ${TOTAL} crates"
    echo "# root: ${ROOT}"
    echo "# scratch: ${SCRATCH}"
    echo ""
    echo "| crate | version | result |"
    echo "|---|---|---|"
    while read -r n v; do
        [[ -n "${n:-}" ]] || continue
        if printf '%s\n' "${FAILED[@]}" | grep -qx "${n}" 2>/dev/null; then
            echo "| ${n} | ${v} | FAIL |"
        else
            echo "| ${n} | ${v} | ok |"
        fi
    done <<<"${ORDER}"
    echo ""
    echo "${PASS} ok, ${FAIL} failed"
    if [[ "${FAIL}" -gt 0 ]]; then
        echo "failed crates: ${FAILED[*]}"
    fi
} >"${SCRATCH}/report.txt"
cat "${SCRATCH}/report.txt"
if [[ -n "${REPORT}" ]]; then
    mkdir -p "$(dirname "${REPORT}")"
    cp "${SCRATCH}/report.txt" "${REPORT}"
fi

if [[ "${FAIL}" -gt 0 ]]; then
    for n in "${FAILED[@]}"; do
        echo "--- ${n} (tail) ---" >&2
        tail -20 "${LOGDIR}/${n}.log" >&2
    done
    exit 1
fi
exit 0
