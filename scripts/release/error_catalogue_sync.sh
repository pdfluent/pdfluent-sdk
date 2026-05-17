#!/usr/bin/env bash
# Verify that the error catalogue document is up to date with Rust source.
# Exit non-zero if any code in error.rs is missing from docs/error_catalogue.md.
#
# Usage:
#   bash scripts/release/error_catalogue_sync.sh
#
# Exit codes:
#   0 — all codes present, no duplicates
#   1 — one or more codes missing from the catalogue, or duplicate codes found
#
# CI integration: add a lane that runs this script on every MR.
# See benchmarks/runs/ga_hardening_plan/c/CI_NOTE.md for the suggested config.
#
# Compatible with bash 3.2+ (macOS default) and bash 4+/5+ (Linux CI).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ERROR_RS="${REPO_ROOT}/crates/pdfluent/src/error.rs"
CATALOGUE="${REPO_ROOT}/docs/error_catalogue.md"

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
MISSING_LIST=""
while IFS= read -r code; do
    if ! grep -qF "${code}" "${CATALOGUE}"; then
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
