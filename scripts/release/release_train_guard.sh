#!/usr/bin/env bash
# release_train_guard.sh — audit-first orchestrator for a publish train.
#
# Given a list of Rust crates (in publish order), this runs the prepublish
# audit for each one and emits the publish commands that would be executed.
# By default it is **audit-only**: it never publishes. Pass --execute to
# actually call `cargo publish` for crates that passed their audit.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md.
#
# Usage:
#   scripts/release/release_train_guard.sh <crate-1> <crate-2> ...
#   scripts/release/release_train_guard.sh --execute <crate-1> <crate-2> ...
#
# Behaviour:
#   - For each crate, runs scripts/release/prepublish_crate_audit.sh.
#   - Aborts the whole train on the first audit failure.
#   - Prints the publish command(s) that would run.
#   - With --execute: runs `cargo publish -p <crate>` for crates that passed.
#     Stops at the first non-zero exit from cargo publish.
#   - With --dry-run-publish: runs `cargo publish --dry-run -p <crate>` after
#     each successful audit (useful for whole-train rehearsal).
#
# Channel scope: this wrapper covers crates.io only. For multi-channel trains,
# call this for the crates.io segment and the per-channel checklists for the
# rest.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
AUDIT="${SCRIPT_DIR}/prepublish_crate_audit.sh"

if [[ ! -x "${AUDIT}" && ! -r "${AUDIT}" ]]; then
    echo "error: audit script not found at ${AUDIT}" >&2
    exit 2
fi

EXECUTE=0
DRY_RUN_PUBLISH=0
CRATES=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --execute)
            EXECUTE=1
            shift
            ;;
        --dry-run-publish)
            DRY_RUN_PUBLISH=1
            shift
            ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        --)
            shift
            while [[ $# -gt 0 ]]; do
                CRATES+=("$1")
                shift
            done
            ;;
        -*)
            echo "error: unknown flag '$1'" >&2
            exit 2
            ;;
        *)
            CRATES+=("$1")
            shift
            ;;
    esac
done

if [[ ${#CRATES[@]} -eq 0 ]]; then
    echo "usage: $(basename "$0") [--execute|--dry-run-publish] <crate-1> <crate-2> ..." >&2
    exit 2
fi

cd "${REPO_ROOT}"

# Clean-tree precondition (the audit script also checks, but a fail-fast at
# the train level is friendlier).
if [[ -n "$(git status --porcelain || true)" ]]; then
    echo "error: working tree dirty. Commit/stash before running the train guard." >&2
    git status --short >&2
    exit 1
fi

# Summary table.
echo "Release-train guard"
echo "==================="
echo "Mode:        $( [[ ${EXECUTE} -eq 1 ]] && echo EXECUTE || echo AUDIT-ONLY )$( [[ ${DRY_RUN_PUBLISH} -eq 1 ]] && echo " + dry-run-publish" || echo "" )"
echo "Crates:      ${CRATES[*]}"
echo "Repo:        ${REPO_ROOT}"
echo ""

PASSED=()
FAILED=()

for CRATE in "${CRATES[@]}"; do
    echo "--- audit: ${CRATE} ---"
    if "${AUDIT}" "${CRATE}"; then
        PASSED+=("${CRATE}")
    else
        FAILED+=("${CRATE}")
        echo "FAILED audit for ${CRATE}; halting train." >&2
        break
    fi

    if [[ ${DRY_RUN_PUBLISH} -eq 1 ]]; then
        echo "--- dry-run publish: ${CRATE} ---"
        if ! cargo publish -p "${CRATE}" --dry-run; then
            FAILED+=("${CRATE} (dry-run)")
            echo "FAILED dry-run publish for ${CRATE}; halting train." >&2
            break
        fi
    fi

    echo ""
done

echo ""
echo "Audit summary"
echo "-------------"
echo "Passed: ${PASSED[*]:-(none)}"
echo "Failed: ${FAILED[*]:-(none)}"
echo ""

if [[ ${#FAILED[@]} -gt 0 ]]; then
    echo "Halting: not running any publish step because at least one crate failed."
    exit 1
fi

# In audit-only mode, print the commands but stop.
if [[ ${EXECUTE} -ne 1 ]]; then
    echo "Audit-only mode. Commands that WOULD run (in order):"
    for CRATE in "${PASSED[@]}"; do
        echo "    cargo publish -p ${CRATE}"
    done
    echo ""
    echo "Re-run with --execute to actually publish."
    exit 0
fi

# Execute mode: publish each crate that passed. Cargo's own `note: waiting for`
# message blocks on index propagation; rely on that instead of a sleep.
echo "EXECUTE mode: publishing crates in order."
for CRATE in "${PASSED[@]}"; do
    echo "--- publish: ${CRATE} ---"
    if ! cargo publish -p "${CRATE}"; then
        echo "FAILED publish for ${CRATE}; halting train. Re-run audit + publish for downstream crates manually." >&2
        exit 1
    fi
done

echo "All passed-audit crates published."
