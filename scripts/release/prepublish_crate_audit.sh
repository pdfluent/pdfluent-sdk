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
#   4. produce the actual .crate via `cargo package -p <crate>`
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
# Step 1 — working tree must be clean.
# ----------------------------------------------------------------------
cd "${REPO_ROOT}"
DIRTY=$(git status --porcelain || true)
if [[ -n "${DIRTY}" ]]; then
    echo "error: working tree is dirty. Commit or stash before running the audit." >&2
    echo "${DIRTY}" >&2
    exit 1
fi

# ----------------------------------------------------------------------
# Step 2/3 — capture the file list and the resolved version.
# ----------------------------------------------------------------------
echo "[1/8] cargo package --list -p ${CRATE_NAME}"
TMP_LIST=$(mktemp -t prepublish_crate_audit_list.XXXXXX)
trap 'rm -f "${TMP_LIST}"' EXIT

if ! cargo package --list -p "${CRATE_NAME}" >"${TMP_LIST}" 2>&1; then
    echo "error: cargo package --list failed for ${CRATE_NAME}" >&2
    cat "${TMP_LIST}" >&2
    exit 1
fi

# Resolve the crate version from cargo metadata. Robust against rename:
# `cargo pkgid -p <pkg>` returns the package URI including the version.
PKGID=$(cargo pkgid -p "${CRATE_NAME}" 2>/dev/null || true)
if [[ -z "${PKGID}" ]]; then
    echo "error: cargo pkgid -p ${CRATE_NAME} returned nothing; is the package name correct?" >&2
    exit 1
fi
# pkgid form examples:
#   path+file:///…/crates/pdf-forms#pdfluent-forms@1.0.0-beta.6
#   registry+https://github.com/rust-lang/crates.io-index#serde@1.0.218
CRATE_VERSION=$(printf '%s\n' "${PKGID}" | sed -E 's/.*@([^#]+)$/\1/')
if [[ -z "${CRATE_VERSION}" || "${CRATE_VERSION}" == "${PKGID}" ]]; then
    echo "error: could not parse version from pkgid: ${PKGID}" >&2
    exit 1
fi
echo "    resolved version: ${CRATE_VERSION}"

# ----------------------------------------------------------------------
# Step 4 — produce the actual .crate.
# ----------------------------------------------------------------------
echo "[2/8] cargo package -p ${CRATE_NAME}"
if ! cargo package -p "${CRATE_NAME}" 1>/dev/null 2>&1; then
    # Re-run without redirection to surface the error.
    cargo package -p "${CRATE_NAME}" >&2
    echo "error: cargo package -p ${CRATE_NAME} failed" >&2
    exit 1
fi

CRATE_FILE="${REPO_ROOT}/target/package/${CRATE_NAME}-${CRATE_VERSION}.crate"
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

CLEAN_TREE_NOTE="git status --porcelain empty at audit start"
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
