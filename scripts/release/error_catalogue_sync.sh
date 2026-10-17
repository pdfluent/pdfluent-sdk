#!/usr/bin/env bash
# Verify that the error catalogue document is up to date with Rust source.
# Exit non-zero if any code in error.rs is missing from docs/error_catalogue.md,
# or if the catalogue's Node column names an error class the Node package does
# not export.
#
# Usage:
#   bash scripts/release/error_catalogue_sync.sh
#
# Exit codes:
#   0 — all codes present, no duplicates
#   1 — one or more codes missing from the catalogue, duplicate codes found,
#       or a Node-column class the package does not export
#
# CI integration: add a lane that runs this script on every MR.
# See benchmarks/runs/ga_hardening_plan/c/CI_NOTE.md for the suggested config.
#
# Compatible with bash 3.2+ (macOS default) and bash 4+/5+ (Linux CI).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ERROR_RS="${REPO_ROOT}/crates/pdfluent/src/error.rs"
CATALOGUE="${REPO_ROOT}/docs/error_catalogue.md"
NODE_DTS="${REPO_ROOT}/crates/pdf-node/index.d.ts"
NODE_LAYER="${REPO_ROOT}/crates/pdf-node/scripts/build/typed-error-layer.js"

# ---------------------------------------------------------------------------
# 1. Verify required files exist
# ---------------------------------------------------------------------------
if [[ ! -f "${ERROR_RS}" ]]; then
    echo "SYNC FAILED: ${ERROR_RS} not found" >&2
    exit 1
fi
if [[ ! -f "${CATALOGUE}" ]]; then
    echo "SYNC FAILED: ${CATALOGUE} not found" >&2
    echo "Run: python3 scripts/docs/generate_error_catalogue.py" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 2. Extract E-* codes from the code() match arms only.
#
# Strategy: extract the block between "pub const fn code(" and the closing
# brace of its match expression using Python (more reliable than awk on
# bash 3.2). Fall back to all E-* strings if Python is unavailable.
# ---------------------------------------------------------------------------
extract_code_arms() {
    python3 - "${ERROR_RS}" <<'PYEOF'
import re, sys

text = open(sys.argv[1]).read()

# Locate the code() method body
m = re.search(r'pub const fn code\(&self\)[^{]*\{(.+?)^\s*\}', text, re.DOTALL | re.MULTILINE)
if not m:
    # Fallback: grab all E-* strings in the file
    for c in re.findall(r'"(E-[A-Z0-9-]+)"', text):
        print(c)
else:
    for c in re.findall(r'"(E-[A-Z0-9-]+)"', m.group(1)):
        print(c)
PYEOF
}

RAW_CODES="$(extract_code_arms)"

if [[ -z "${RAW_CODES}" ]]; then
    echo "SYNC FAILED: no error codes found in the code() method of ${ERROR_RS}" >&2
    exit 1
fi

RS_CODES_SORTED="$(echo "${RAW_CODES}" | sort -u)"
RS_CODE_COUNT="$(echo "${RS_CODES_SORTED}" | wc -l | tr -d ' ')"

echo "Found ${RS_CODE_COUNT} distinct error code(s) in error.rs code() arms:"
while IFS= read -r code; do
    echo "  ${code}"
done <<< "${RS_CODES_SORTED}"
echo

# ---------------------------------------------------------------------------
# 3. Check for duplicate codes within the code() match arms
# ---------------------------------------------------------------------------
RAW_COUNT="$(echo "${RAW_CODES}" | wc -l | tr -d ' ')"
UNIQUE_COUNT="$(echo "${RAW_CODES}" | sort -u | wc -l | tr -d ' ')"

if [[ "${RAW_COUNT}" -ne "${UNIQUE_COUNT}" ]]; then
    echo "SYNC FAILED: duplicate error codes in the code() match arms of ${ERROR_RS}" >&2
    echo "${RAW_CODES}" | sort | uniq -d | while IFS= read -r dup; do
        echo "  duplicate: ${dup}" >&2
    done
    exit 1
fi

echo "Duplicate check passed (${UNIQUE_COUNT} unique code(s) in code() arms)."
echo

# ---------------------------------------------------------------------------
# 4. Verify every Rust code appears in the catalogue
# ---------------------------------------------------------------------------
# The code must have a TABLE ROW of its own, not merely occur somewhere in the
# file. A bare substring match passed `E-UNSUPPORTED` for years because
# `E-PARSE-UNSUPPORTED-VERSION` contains it, so a code with no row read as
# present; matching a backtick-quoted code anywhere would have passed it too,
# on its mention in the per-binding gap summary (#426).
MISSING_LIST=""
while IFS= read -r code; do
    if ! grep -qE "^\| \`${code}\` \|" "${CATALOGUE}"; then
        MISSING_LIST="${MISSING_LIST}  ${code}"$'\n'
    fi
done <<< "${RS_CODES_SORTED}"

if [[ -n "${MISSING_LIST}" ]]; then
    echo "SYNC FAILED: the following code(s) are in error.rs but not in ${CATALOGUE}:" >&2
    echo "${MISSING_LIST}" >&2
    echo "Run: python3 scripts/docs/generate_error_catalogue.py" >&2
    echo "Then commit docs/error_catalogue.md before releasing." >&2
    exit 1
fi

echo "Catalogue sync OK — all ${RS_CODE_COUNT} code(s) present in ${CATALOGUE}."

# ---------------------------------------------------------------------------
# 5. Every class the Node column names must be one the package exports
# ---------------------------------------------------------------------------
# The Node column used to name PdfluentIoError, PdfluentParseError,
# PdfluentPasswordError, PdfluentValueError and PdfluentOperationError. None of
# them ever existed; the strict-ts example imported six phantom names off this
# table (#587). The column is prose, so nothing checked it. Now every
# `Pdfluent*` name in it has to be declared in index.d.ts and exported by the
# typed-error layer that postbuild appends to index.js.
for f in "${NODE_DTS}" "${NODE_LAYER}"; do
    if [[ ! -f "${f}" ]]; then
        echo "SYNC FAILED: ${f} not found" >&2
        exit 1
    fi
done

NODE_CLASSES="$(python3 - "${CATALOGUE}" <<'PYEOF'
import re, sys

header = None
for line in open(sys.argv[1], encoding="utf-8"):
    if not line.startswith("|"):
        continue
    cells = [c.strip() for c in line.strip().strip("|").split(" | ")]
    if header is None:
        if "Node" in cells:
            header = cells
        continue
    if not cells[0].startswith("`E-"):
        continue
    node = cells[header.index("Node")]
    for name in re.findall(r"\bPdfluent[A-Za-z]*\b", node):
        print(cells[0].strip("`") + " " + name)
PYEOF
)"

PHANTOM_LIST=""
while IFS=' ' read -r code name; do
    [[ -z "${name}" ]] && continue
    if ! grep -qE "^export declare class ${name}( |\{|$)" "${NODE_DTS}" \
        || ! grep -qE "^module\.exports\.${name} = " "${NODE_LAYER}"; then
        PHANTOM_LIST="${PHANTOM_LIST}  ${code}: ${name}"$'\n'
    fi
done <<< "${NODE_CLASSES}"

if [[ -n "${PHANTOM_LIST}" ]]; then
    echo "SYNC FAILED: the Node column of ${CATALOGUE} names classes the Node package does not export:" >&2
    echo "${PHANTOM_LIST}" >&2
    echo "Fix BINDING_STATUS in scripts/docs/generate_error_catalogue.py, or export the class" >&2
    echo "from crates/pdf-node/scripts/build/typed-error-layer.js and its .d.ts." >&2
    exit 1
fi

echo "Node column OK — every class it names is exported by @pdfluent/node."
