#!/usr/bin/env bash
# Unit-test the version-aware Gate E logic without running the full dry-run.
# Sources the relevant variables and applies the same comparison logic.

set -euo pipefail

A2_WASM_SHA256="25be29415573afb2df67970bef33992660973559a6e8a7f8938def3c991ec698"
A2_LEDGER_VERSION="1.0.0-beta.11"

check() {
    local label="$1"
    local pkg_version="$2"
    local wasm_sha="$3"
    local expected="$4"

    local actual
    if [[ "$wasm_sha" == "$A2_WASM_SHA256" ]]; then
        actual="MATCH"
    elif [[ "$pkg_version" == "$A2_LEDGER_VERSION" ]]; then
        actual="STRICT_FAIL"
    else
        actual="INFORMATIONAL_DIVERGE"
    fi

    if [[ "$actual" == "$expected" ]]; then
        echo "  ✓ $label → $actual"
    else
        echo "  ✗ $label → got $actual, expected $expected"
        exit 1
    fi
}

echo "=== Gate E version-aware unit tests ==="
check "equal version + equal SHA"     "1.0.0-beta.11" "$A2_WASM_SHA256"                                                                  "MATCH"
check "equal version + diff SHA"      "1.0.0-beta.11" "0000000000000000000000000000000000000000000000000000000000000000"                  "STRICT_FAIL"
check "diff version + diff SHA"       "1.0.0-beta.3"  "444558a2d820997c114bade7fda7133af1025b323c5955e507bef8342dd7cfb3"                  "INFORMATIONAL_DIVERGE"
check "newer version + diff SHA"      "1.0.0-beta.12" "1111111111111111111111111111111111111111111111111111111111111111"                  "INFORMATIONAL_DIVERGE"
check "older version + matching SHA"  "1.0.0-beta.10" "$A2_WASM_SHA256"                                                                  "MATCH"
echo "All Gate E logic tests pass."
