#!/usr/bin/env bash
# prepublish_crate_audit.sh — mandatory pre-publish audit for a Rust crate.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md.
#
# Usage: scripts/release/prepublish_crate_audit.sh <crate-name>
#
# The argument is the cargo package name (the value of [package].name in
# Cargo.toml — for renamed crates, that is the *published* name, e.g.
# "pdfluent-forms", not the path-key "pdf-forms").
#
# What it does:
#   1. fail if the working tree is dirty
#   2. fail if cargo can't build the package
#   3. run `cargo package --list -p <crate>` and capture the file list
#   4. produce the actual .crate: per-crate `cargo package -p <crate>` first,
#      and when that fails resolving the release's own unpublished
#      dependencies, the workspace route through
#      scripts/release/verify_packaged_workspace.sh: ONE
#      `cargo package --workspace --locked --no-verify` plus a standalone
#      build over the unpacked .crates (cargo 1.94's local overlay).
#   5. unpack the .crate into a temp dir
#   6. inspect manifest licence metadata + verify the LICENSE / LICENSE-APACHE /
#      LICENSE-MIT / NOTICE files actually live inside the tarball
#   7. invoke audit_package_tree.py to scan for leakage patterns
#   8. write a markdown report to benchmarks/runs/prepublish_audits/
#   9. exit non-zero on any blocker
#
# This script never publishes anything. It is the gate that must pass before
# the operator runs `cargo publish`.

set -Eeuo pipefail

# ----------------------------------------------------------------------
# Resolve repo root and helper paths.
# ----------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
HELPER="${SCRIPT_DIR}/audit_package_tree.py"
REPORTS_DIR="${REPO_ROOT}/benchmarks/runs/prepublish_audits"

if [[ ! -x "${HELPER}" && ! -r "${HELPER}" ]]; then
    echo "error: audit helper missing at ${HELPER}" >&2
    exit 2
fi

# ----------------------------------------------------------------------
# Args.
# ----------------------------------------------------------------------
if [[ $# -ne 1 ]]; then
    echo "usage: $(basename "$0") <crate-name>" >&2
    echo "  <crate-name> is the published package name (e.g. pdfluent-forms, pdf-engine, pdfluent-lopdf)" >&2
    exit 2
fi
CRATE_NAME="$1"

# ----------------------------------------------------------------------
# Step-4 helpers: the workspace fallback and its freshness manifest.
# ----------------------------------------------------------------------
# Set by workspace_package_facts; the lock dir by the lock holder only.
TARGET_DIR=""
PKGDIR=""
EXCLUDES=()
AUDIT_PACK_LOCKDIR=""

# Facts about this workspace from one cargo metadata call: where cargo
# writes its .crates (honours CARGO_TARGET_DIR, which the shared build
# cache sets), and which workspace members must be excluded from a
# workspace packaging because they are never published.
workspace_package_facts() {
    local facts
    facts=$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "
import json, sys
m = json.load(sys.stdin)
print(m.get('target_directory', ''))
for p in m['packages']:
    if p.get('publish') == []:
        print('--exclude ' + p['name'])
") || {
        echo "error: cargo metadata failed" >&2
        exit 1
    }
    TARGET_DIR="$(printf '%s\n' "${facts}" | sed -n '1p')"
    if [[ -z "${TARGET_DIR}" ]]; then
        echo "error: cargo metadata reported no target directory" >&2
        exit 1
    fi
    PKGDIR="${TARGET_DIR}/package"
    EXCLUDES=()
    local line
    while IFS= read -r line; do
        case "${line}" in
            --exclude\ *)
                EXCLUDES+=("--exclude" "${line#--exclude }") ;;
        esac
    done <<<"$(printf '%s\n' "${facts}" | sed -n '2,$p')"
}

# The freshness-manifest key: what the packaged bytes are a function of.
# The tree is clean (step 1 ran), so HEAD names the sources; the cargo
# version and the checkout path name the toolchain and the bytes' mtimes.
audit_pack_key() {
    python3 - "$(git -C "${REPO_ROOT}" rev-parse HEAD)" "$(cargo --version)" \
        "${REPO_ROOT}" <<'PYEOF'
import hashlib, sys
print(hashlib.sha1("\n".join(sys.argv[1:]).encode()).hexdigest()[:16])
PYEOF
}

audit_pack_manifest() {
    printf '%s/audit-pack/%s/crates.json' "${TARGET_DIR}" "$(audit_pack_key)"
}

# Print the recorded packaging route for a crate whose .crate on disk
# still matches the recorded sha256; exit 1 otherwise (no manifest, no
# entry, missing file, or a stale .crate planted over the audited one).
manifest_route_if_fresh() {
    python3 - "$1" "$2" "$3" <<'PYEOF' || return 1
import hashlib, json, sys
name, crate_file, manifest = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with open(manifest, encoding="utf-8") as fh:
        found = json.load(fh)
except (OSError, ValueError):
    sys.exit(1)
entry = (found.get("crates") or {}).get(name)
if not entry:
    sys.exit(1)
digest = hashlib.sha256()
try:
    with open(crate_file, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            digest.update(chunk)
except OSError:
    sys.exit(1)
if digest.hexdigest() != entry.get("sha"):
    sys.exit(1)
print(entry.get("route", ""))
PYEOF
}

# Record one crate's .crate hash and route, keeping the rest of the
# manifest (the fast route lands here without the standalone build).
manifest_record_crate() {
    python3 - "$1" "$2" "$3" "$4" <<'PYEOF'
import hashlib, json, os, sys
manifest, name, crate_file, route = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
os.makedirs(os.path.dirname(manifest), exist_ok=True)
try:
    with open(manifest, encoding="utf-8") as fh:
        found = json.load(fh)
except (OSError, ValueError):
    found = {"build_ok": False, "crates": {}}
digest = hashlib.sha256()
with open(crate_file, "rb") as fh:
    for chunk in iter(lambda: fh.read(65536), b""):
        digest.update(chunk)
found.setdefault("crates", {})[name] = {"sha": digest.hexdigest(), "route": route}
tmp = manifest + ".new"
with open(tmp, "w", encoding="utf-8") as fh:
    json.dump(found, fh, indent=2, sort_keys=True)
    fh.write("\n")
os.replace(tmp, manifest)
PYEOF
}

# Rewrite the manifest wholesale after a workspace packaging plus a green
# standalone build: every .crate is fresh and every route is the workspace.
manifest_write_all() {
    local manifest="$1" pkgdir="$2"
    shift 2
    python3 - "${manifest}" "${pkgdir}" "$@" <<'PYEOF'
import hashlib, json, os, sys
manifest, pkgdir, pairs = sys.argv[1], sys.argv[2], sys.argv[3:]
found = {"build_ok": True, "crates": {}}
for pair in pairs:
    name, _, version = pair.partition("=")
    crate_file = os.path.join(pkgdir, f"{name}-{version}.crate")
    digest = hashlib.sha256()
    with open(crate_file, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            digest.update(chunk)
    found["crates"][name] = {
        "sha": digest.hexdigest(),
        "route": "workspace overlay + standalone build",
    }
os.makedirs(os.path.dirname(manifest), exist_ok=True)
tmp = manifest + ".new"
with open(tmp, "w", encoding="utf-8") as fh:
    json.dump(found, fh, indent=2, sort_keys=True)
    fh.write("\n")
os.replace(tmp, manifest)
PYEOF
}

# Only one process packages a key at a time: the .crates it writes are
# shared with every session on this machine. Waiters sleep on the lock
# dir; a holder that died (kill -9 skips the trap) leaves a lock whose
# pid is gone, and anything older than an hour is stale no matter what.
audit_pack_maybe_break_stale() {
    local lockdir="$1" holder pid stamp now
    [[ -f "${lockdir}/holder" ]] || return 1
    holder=$(cat "${lockdir}/holder" 2>/dev/null || echo "")
    pid="${holder%% *}"
    stamp="${holder##* }"
    if [[ -n "${pid}" ]] && ! kill -0 "${pid}" 2>/dev/null; then
        rm -rf "${lockdir}"
        return 0
    fi
    now=$(date +%s)
    if [[ "${stamp}" =~ ^[0-9]+$ ]] && [[ $((now - stamp)) -gt 3600 ]]; then
        rm -rf "${lockdir}"
        return 0
    fi
    return 1
}

audit_pack_acquire_lock() {
    local lockdir="$1" waited=0
    mkdir -p "$(dirname "${lockdir}")"
    while true; do
        if mkdir "${lockdir}" 2>/dev/null; then
            echo "$$ $(date +%s)" >"${lockdir}/holder" 2>/dev/null || true
            AUDIT_PACK_LOCKDIR="${lockdir}"
            return 0
        fi
        if audit_pack_maybe_break_stale "${lockdir}"; then
            continue
        fi
        if [[ "${waited}" -ge 180 ]]; then
            echo "error: timed out waiting for ${lockdir}" >&2
            exit 1
        fi
        sleep 10
        waited=$((waited + 1))
    done
}

audit_pack_release_lock() {
    if [[ -n "${AUDIT_PACK_LOCKDIR}" ]]; then
        rm -rf "${AUDIT_PACK_LOCKDIR}"
        AUDIT_PACK_LOCKDIR=""
    fi
}

# The disk is tight and keys accumulate (one per HEAD per checkout): keep
# the two newest, drop the rest. Only complete keys count -- a build in
# progress has no manifest yet and is never pruned.
audit_pack_prune() {
    local base="${TARGET_DIR}/audit-pack" count=0
    [[ -d "${base}" ]] || return 0
    local d
    while IFS= read -r d; do
        [[ -z "${d}" || "${d}" == *.lock ]] && continue
        [[ -f "${base}/${d}/crates.json" ]] || continue
        count=$((count + 1))
        if [[ "${count}" -gt 2 ]]; then
            rm -rf "${base:?}/${d}"
        fi
    done < <(ls -t "${base}" 2>/dev/null || true)
}

# The workspace fallback: ONE `cargo package --workspace --locked --no-verify`
# for every publishable crate, then a standalone build over the unpacked
# .crates -- both inside scripts/release/verify_packaged_workspace.sh, which
# the dry run's crates channel also calls, so the two cannot drift.
# cargo's own workspace verify cannot do this itself: it ICEs on its
# hashless tmp-registry ("no hash listed for xfa-dom-resolver v1.1.0",
# cargo 1.94.0) -- hence --no-verify plus the standalone build. When cargo
# fixes the ICE the route can return to plain `cargo package --workspace`.
# Sets PACK_ROUTE. Exits 1 on any failure.
workspace_fallback_package() {
    local key packdir manifest lockdir synthdir
    key="$(audit_pack_key)"
    packdir="${TARGET_DIR}/audit-pack/${key}"
    manifest="${packdir}/crates.json"
    lockdir="${TARGET_DIR}/audit-pack/${key}.lock"
    audit_pack_acquire_lock "${lockdir}"
    # Re-validate under the lock: another process may have finished first.
    local route
    if route="$(manifest_route_if_fresh "${CRATE_NAME}" "${CRATE_FILE}" "${manifest}")"; then
        PACK_ROUTE="${route}"
        echo "    reusing audited .crate (${PACK_ROUTE})"
        audit_pack_release_lock
        return 0
    fi
    echo "    workspace packaging + standalone build (verify_packaged_workspace.sh) ..."
    synthdir="${packdir}/synth"
    local helperlog
    helperlog=$(mktemp -t prepublish_workspace_helper.XXXXXX)
    # Pass no `--allow-dirty`: step 1 refused a dirty tree, and the
    # freshness manifest keys the packaged bytes by HEAD.
    if ! bash "${SCRIPT_DIR}/verify_packaged_workspace.sh" \
            --root "${REPO_ROOT}" --scratch "${synthdir}" \
            --report "${packdir}/report.txt" >"${helperlog}" 2>&1; then
        echo "error: the packaged crates do not build standalone" >&2
        tail -30 "${helperlog}" >&2
        rm -f "${helperlog}"
        audit_pack_release_lock
        exit 1
    fi
    cat "${helperlog}"
    rm -f "${helperlog}"
    local names
    names="$(python3 "${SCRIPT_DIR}/topological_publish_order.py")"
    local pairs=()
    local n v
    while read -r n v; do
        [[ -n "${n:-}" ]] || continue
        pairs+=("${n}=${v}")
    done <<<"${names}"
    manifest_write_all "${manifest}" "${PKGDIR}" "${pairs[@]}"
    audit_pack_prune
    PACK_ROUTE="workspace overlay + standalone build"
    echo "    workspace route complete (${PACK_ROUTE})"
    audit_pack_release_lock
}

# ----------------------------------------------------------------------
# Step 0 — internal-only crates are never published.
# A crate declared `publish = false` has no crates.io tarball, so the
# published-tarball licence-file gate does not apply to it. `cargo metadata`
# reports `publish = false` as an empty array ([]) and a publishable crate as
# null. Short-circuit internal crates cleanly (before the clean-tree gate, as
# `cargo metadata` does not need a clean tree) so they cannot register as a
# false licence blocker. If such a crate is later flipped to publishable, the
# field becomes null and the full audit below runs again.
# ----------------------------------------------------------------------
cd "${REPO_ROOT}"
PY_PREFLIGHT=$(command -v python3 || command -v python || true)
if [[ -n "${PY_PREFLIGHT}" ]]; then
    IS_INTERNAL=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
        | "${PY_PREFLIGHT}" -c "import sys,json; m=json.load(sys.stdin); p=[k.get('publish') for k in m['packages'] if k['name']=='${CRATE_NAME}']; print('yes' if (p and p[0]==[]) else 'no')" 2>/dev/null || true)
    if [[ "${IS_INTERNAL}" == "yes" ]]; then
        echo "SKIP: ${CRATE_NAME} is declared 'publish = false' (internal-only)."
        echo "      Internal crates are never published to crates.io, so the"
        echo "      published-tarball licence-file audit does not apply. Nothing to audit."
        exit 0
    fi
fi

# ----------------------------------------------------------------------
# The version, resolved before the clean-tree gate.
# ----------------------------------------------------------------------
# Step 1 has to know which three report files THIS run is going to overwrite,
# and their names carry the version. `cargo pkgid` reads the manifests and does
# not care whether the tree is clean, the same reason step 0 runs where it does.
# Resolve the crate version from cargo metadata. Robust against rename:
# `cargo pkgid -p <pkg>` returns the package URI including the version.
PKGID=$(cargo pkgid -p "${CRATE_NAME}" 2>/dev/null || true)
if [[ -z "${PKGID}" ]]; then
    echo "error: cargo pkgid -p ${CRATE_NAME} returned nothing; is the package name correct?" >&2
    exit 1
fi
# pkgid forms cargo emits:
#   path+file:///…/crates/pdf-forms#pdfluent-forms@1.0.0-beta.6   (renamed)
#   path+file:///…/crates/pdf-manip#1.0.0-beta.5                  (non-renamed)
#   registry+https://github.com/rust-lang/crates.io-index#serde@1.0.218
if [[ "${PKGID}" == *@* ]]; then
    CRATE_VERSION="${PKGID##*@}"
else
    CRATE_VERSION="${PKGID##*#}"
fi
if [[ -z "${CRATE_VERSION}" || "${CRATE_VERSION}" == "${PKGID}" ]]; then
    echo "error: could not parse version from pkgid: ${PKGID}" >&2
    exit 1
fi
echo "    resolved version: ${CRATE_VERSION}"

# ----------------------------------------------------------------------
# Step 1 — working tree must be clean.
# ----------------------------------------------------------------------
# THE AUDIT'S OWN REPORTS ARE NOT DIRT (#358).
#
# Step 8 writes three files into benchmarks/runs/prepublish_audits/, and this
# check ran before them without excluding them -- so the audit of crate N left
# exactly the dirt that made the audit of crate N+1 refuse, and a publish of 32
# crates could never get past the first one it audited. Measured on the 1.0.0
# publish of 16-09-2026: it stopped at crate 5, listing the three files its own
# previous audit had written.
#
# What is dropped is UNTRACKED output in that one directory, not the directory.
# A pathspec exclusion would also hide a tracked report that was edited, staged
# or deleted -- 107 of these reports are under version control, and a change to
# one of them is a change to the evidence of an earlier release, which
# PUBLISH_PROTOCOL.md 7 says the audit must refuse. Only `?? ` lines under that
# prefix are dropped; ` M`, `A `, ` D` and the rest still stop the audit.
# (codex, #1916)
#
# The reports are not gitignored, because the evidence of a release is worth
# keeping under version control; it is this check that has to know the
# difference, not .gitignore.
#
# `git -C` because this runs from wherever the caller stands, and the paths
# git prints are relative to the repository root.
#
# And the three files this run is about to overwrite are not dirt either, even
# when they are tracked and modified. Once a release's reports are committed --
# which PUBLISH_PROTOCOL.md 4 asks for -- re-auditing that same version rewrites
# them with a fresh temporary path inside, which shows up as ` M` and would stop
# the next crate in the train. Only these three, by exact name: a tracked report
# of another crate or another version that changed is still dirt.
# (codex, #1916)
# The rule itself is in tree_dirt.sh, because it was in two files and only one
# of them was ever taught it (#358).
# shellcheck source=scripts/release/tree_dirt.sh
source "${SCRIPT_DIR}/tree_dirt.sh"
DIRTY=$(pdfluent_tree_dirt "${REPO_ROOT}" "${CRATE_NAME}" "${CRATE_VERSION}")
if [[ -n "${DIRTY}" ]]; then
    echo "error: working tree is dirty. Commit or stash before running the audit." >&2
    echo "${DIRTY}" >&2
    exit 1
fi

# ----------------------------------------------------------------------
# Step 2/3 — capture the file list (the version is resolved above).
# ----------------------------------------------------------------------
echo "[1/8] cargo package --list -p ${CRATE_NAME}"
TMP_LIST=$(mktemp -t prepublish_crate_audit_list.XXXXXX)
TMP_PACKLOG=""
trap 'rm -f "${TMP_LIST}" ${TMP_PACKLOG:-}; audit_pack_release_lock 2>/dev/null || true' EXIT

if ! cargo package --list -p "${CRATE_NAME}" >"${TMP_LIST}" 2>&1; then
    echo "error: cargo package --list failed for ${CRATE_NAME}" >&2
    cat "${TMP_LIST}" >&2
    exit 1
fi

# ----------------------------------------------------------------------
# Step 4 — produce the actual .crate.
# ----------------------------------------------------------------------
# Two routes to the same bytes. The fast route is the historical one: a
# per-crate `cargo package -p`, verified by cargo against the registry.
# While the release's own dependencies are unpublished (`=1.1.0` pinned,
# only 1.0.0 on the index) that fails resolving them, and the audit falls
# back to the workspace route through verify_packaged_workspace.sh: ONE
# `cargo package --workspace --locked --no-verify` (cargo 1.94's local
# overlay resolves each crate against the packaged versions of the others,
# in the same invocation) plus a standalone build over the unpacked .crates,
# which is what cargo's own verify would have done. Anything that is NOT a
# resolution failure -- a bad manifest, a build break -- stays loud on the
# fast route: the fallback must never launder a real defect into a green.
echo "[2/8] produce the .crate for ${CRATE_NAME}"
workspace_package_facts
CRATE_FILE="${PKGDIR}/${CRATE_NAME}-${CRATE_VERSION}.crate"
PACK_ROUTE=""
FALLBACK_REASON=""
MANIFEST="$(audit_pack_manifest)"
if PACK_ROUTE="$(manifest_route_if_fresh "${CRATE_NAME}" "${CRATE_FILE}" "${MANIFEST}")"; then
    echo "    reusing audited .crate (${PACK_ROUTE})"
else
    PACK_ROUTE=""
    # cargo writes the .crate into an existing file without truncating it
    # (#591). A smaller crate over an earlier, larger one leaves the old
    # tail behind the gzip stream: GNU tar then stops at step 3 with
    # "trailing garbage", and a gate instance reused from the pool -- whose
    # target/ keeps the last run's packages -- failed this audit on every
    # re-use. The file is this run's to make.
    rm -f "${CRATE_FILE}"
    TMP_PACKLOG=$(mktemp -t prepublish_crate_audit_packlog.XXXXXX)
    if cargo package -p "${CRATE_NAME}" >"${TMP_PACKLOG}" 2>&1; then
        PACK_ROUTE="per-crate cargo verify"
        manifest_record_crate "${MANIFEST}" "${CRATE_NAME}" "${CRATE_FILE}" "${PACK_ROUTE}"
        echo "    packaged per-crate (cargo verify)"
    elif grep -qE "failed to select a version for the requirement|no matching package named" "${TMP_PACKLOG}"; then
        FALLBACK_REASON="$(grep -m1 -E "failed to select a version for the requirement|no matching package named" "${TMP_PACKLOG}" | sed 's/^ *//')"
        echo "    per-crate verify is registry-blocked; falling back to the workspace route"
        echo "    blocked on: ${FALLBACK_REASON}"
        workspace_fallback_package
    else
        echo "error: cargo package -p ${CRATE_NAME} failed" >&2
        cat "${TMP_PACKLOG}" >&2
        rm -f "${TMP_PACKLOG}"
        exit 1
    fi
    rm -f "${TMP_PACKLOG}"
fi

if [[ ! -f "${CRATE_FILE}" ]]; then
    echo "error: expected ${CRATE_FILE} but it was not produced" >&2
    exit 1
fi

CRATE_SHA=$(shasum -a 256 "${CRATE_FILE}" | awk '{print $1}')
CRATE_SIZE=$(wc -c <"${CRATE_FILE}" | tr -d ' ')
echo "    .crate produced: ${CRATE_FILE} (${CRATE_SIZE} bytes, sha256 ${CRATE_SHA})"

# ----------------------------------------------------------------------
# Step 5 — unpack into a temp dir.
# ----------------------------------------------------------------------
echo "[3/8] unpack .crate"
WORK_DIR=$(mktemp -d -t prepublish_crate_audit.XXXXXX)
# Keep the work dir on success too, for forensic inspection; clean only on
# unrelated trap exit paths.
UNPACK_DIR="${WORK_DIR}/unpack"
mkdir -p "${UNPACK_DIR}"
tar -xzf "${CRATE_FILE}" -C "${UNPACK_DIR}"

EXPECTED_TOPDIR="${UNPACK_DIR}/${CRATE_NAME}-${CRATE_VERSION}"
if [[ ! -d "${EXPECTED_TOPDIR}" ]]; then
    echo "error: tarball top-level dir not found at ${EXPECTED_TOPDIR}" >&2
    echo "    found:" >&2
    ls -la "${UNPACK_DIR}" >&2
    exit 1
fi
echo "    unpacked to: ${EXPECTED_TOPDIR}"

# ----------------------------------------------------------------------
# Step 6 — inspect manifest licence metadata + LICENSE files.
# ----------------------------------------------------------------------
echo "[4/8] inspect manifest licence metadata"

PACKAGED_MANIFEST="${EXPECTED_TOPDIR}/Cargo.toml"
LICENCE_LINE=$(awk '
    /^\[package\]/        { in_pkg = 1; next }
    /^\[/                 { in_pkg = 0 }
    in_pkg && /^license[[:space:]]*=/      { print "license="$0;       found = 1 }
    in_pkg && /^license-file[[:space:]]*=/ { print "license-file="$0;  found = 1 }
    END { if (!found) exit 3 }
' "${PACKAGED_MANIFEST}") || {
    echo "error: Cargo.toml inside the tarball has neither 'license' nor 'license-file'" >&2
    exit 1
}

echo "    manifest licence metadata: ${LICENCE_LINE}"

# Determine required licence files based on what the manifest declares.
NEEDS_FILES=()
case "${LICENCE_LINE}" in
    *"\"MIT OR Apache-2.0\""*|*"\"Apache-2.0 OR MIT\""*)
        NEEDS_FILES+=("LICENSE-APACHE" "LICENSE-MIT")
        ;;
    *"\"MIT\""*)
        NEEDS_FILES+=("LICENSE")
        ;;
    *"\"Apache-2.0\""*)
        NEEDS_FILES+=("LICENSE" "NOTICE")
        ;;
    *"\"BSD-3-Clause\""*|*"\"BSD-2-Clause\""*|*"\"Zlib\""*)
        NEEDS_FILES+=("LICENSE")
        ;;
    license-file=*)
        # Pull the filename out of `license-file = "X"`.
        FNAME=$(printf '%s\n' "${LICENCE_LINE}" | sed -E 's/.*license-file[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')
        NEEDS_FILES+=("${FNAME}")
        ;;
    *)
        echo "warning: unrecognised licence expression in manifest: ${LICENCE_LINE}" >&2
        NEEDS_FILES+=("LICENSE")
        ;;
esac

MISSING_LICENCE=0
for f in "${NEEDS_FILES[@]}"; do
    if [[ -f "${EXPECTED_TOPDIR}/${f}" ]]; then
        FSIZE=$(wc -c <"${EXPECTED_TOPDIR}/${f}" | tr -d ' ')
        FSHA=$(shasum -a 256 "${EXPECTED_TOPDIR}/${f}" | awk '{print $1}')
        echo "    licence file present: ${f} (${FSIZE} bytes, sha256 ${FSHA:0:16}…)"
    else
        echo "    MISSING licence file: ${f}" >&2
        MISSING_LICENCE=1
    fi
done

if [[ "${MISSING_LICENCE}" -ne 0 ]]; then
    echo "error: one or more required licence files are missing from the published tarball" >&2
    exit 1
fi

# ----------------------------------------------------------------------
# Step 7 — invoke the generic tree audit.
# ----------------------------------------------------------------------
echo "[5/8] audit_package_tree.py scan"
mkdir -p "${REPORTS_DIR}"
PYTHON_BIN=$(command -v python3 || command -v python)
if [[ -z "${PYTHON_BIN}" ]]; then
    echo "error: python3 not found on PATH" >&2
    exit 2
fi

TREE_AUDIT_RC=0
"${PYTHON_BIN}" "${HELPER}" \
    --tree "${EXPECTED_TOPDIR}" \
    --out "${REPORTS_DIR}" \
    --package-name "${CRATE_NAME}" \
    --package-version "${CRATE_VERSION}" \
    --channel crates_io \
    || TREE_AUDIT_RC=$?

# ----------------------------------------------------------------------
# Step 8 — write the orchestrating audit report.
# ----------------------------------------------------------------------
echo "[6/8] write orchestrating audit report"
REPORT="${REPORTS_DIR}/${CRATE_NAME}-${CRATE_VERSION}.md"
TREE_REPORT="${REPORTS_DIR}/${CRATE_NAME}-${CRATE_VERSION}.audit.md"
TREE_JSON="${REPORTS_DIR}/${CRATE_NAME}-${CRATE_VERSION}.audit.json"

# What the report says the audit checked has to be what it checked: since
# #358 the rule drops this run's own report files, and a note claiming an
# empty `git status` would be evidence of something that did not happen.
CLEAN_TREE_NOTE="tree clean at audit start (scripts/release/tree_dirt.sh: this run's own audit reports excepted)"
DRY_RUN_NOTE="not run by this script; run \`cargo publish -p ${CRATE_NAME} --dry-run\` separately before publishing"

{
    printf '# Prepublish audit — `%s %s` (crates.io)\n\n' "${CRATE_NAME}" "${CRATE_VERSION}"
    printf 'Generated by `scripts/release/prepublish_crate_audit.sh`.\n\n'
    printf '## Inputs\n\n'
    printf -- '- Repo: `%s`\n' "${REPO_ROOT}"
    printf -- '- Crate (cargo package name): `%s`\n' "${CRATE_NAME}"
    printf -- '- Version resolved via `cargo pkgid`: `%s`\n' "${CRATE_VERSION}"
    printf -- '- Tarball: `%s`\n' "${CRATE_FILE}"
    printf -- '- Tarball size: %s bytes\n' "${CRATE_SIZE}"
    printf -- '- Tarball sha256: `%s`\n' "${CRATE_SHA}"
    printf -- '- Packaging route: `%s`\n' "${PACK_ROUTE}"
    if [[ -n "${FALLBACK_REASON}" ]]; then
        printf -- '- Per-crate verify was registry-blocked: `%s`\n' "${FALLBACK_REASON}"
    fi
    printf -- '- Working tree state: %s\n' "${CLEAN_TREE_NOTE}"
    printf -- '- Dry-run: %s\n\n' "${DRY_RUN_NOTE}"

    printf '## Manifest licence metadata\n\n'
    printf -- '- Detected line: `%s`\n' "${LICENCE_LINE}"
    printf -- '- Required licence file(s):\n'
    for f in "${NEEDS_FILES[@]}"; do
        printf -- '  - `%s`\n' "${f}"
    done
    printf '\n## Required licence files present in tarball\n\n'
    for f in "${NEEDS_FILES[@]}"; do
        if [[ -f "${EXPECTED_TOPDIR}/${f}" ]]; then
            FSIZE=$(wc -c <"${EXPECTED_TOPDIR}/${f}" | tr -d ' ')
            FSHA=$(shasum -a 256 "${EXPECTED_TOPDIR}/${f}" | awk '{print $1}')
            printf -- '- `%s` — %s bytes, sha256 `%s`\n' "${f}" "${FSIZE}" "${FSHA}"
        else
            printf -- '- `%s` — **MISSING** ❌\n' "${f}"
        fi
    done

    printf '\n## Package-tree scan\n\n'
    printf -- '- Detailed report: `%s`\n' "${TREE_REPORT#"${REPO_ROOT}"/}"
    printf -- '- JSON: `%s`\n' "${TREE_JSON#"${REPO_ROOT}"/}"
    printf -- '- Helper exit code: `%s` (0 = clean, 1 = blockers, 2 = usage)\n\n' "${TREE_AUDIT_RC}"

    printf '## Package contents (cargo package --list)\n\n'
    printf -- '```\n'
    cat "${TMP_LIST}"
    printf -- '```\n\n'

    if [[ "${TREE_AUDIT_RC}" -ne 0 ]]; then
        printf '## Verdict\n\n'
        printf -- '`AUDIT_BLOCKED` — tree scan returned exit %s. Inspect `%s` for blocker list. Do NOT publish.\n' "${TREE_AUDIT_RC}" "${TREE_REPORT#"${REPO_ROOT}"/}"
    else
        printf '## Verdict\n\n'
        printf -- '`AUDIT_PASS` — required licence files present in the published tarball, tree scan returned 0. Operator may proceed to `cargo publish --dry-run` and then `cargo publish` (clean tree required, no `--allow-dirty`).\n'
    fi

    printf '\n---\n'
    printf 'Governed by `docs/release/PUBLISH_PROTOCOL.md`.\n'
} > "${REPORT}"

echo "[7/8] audit report written: ${REPORT}"
echo "[8/8] done."

# ----------------------------------------------------------------------
# Exit policy: non-zero if the tree audit returned non-zero.
# ----------------------------------------------------------------------
exit "${TREE_AUDIT_RC}"
