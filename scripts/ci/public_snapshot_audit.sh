#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the published tree and run only the path audit over it (#222).
#
# The gate line needs one command, and producing the tree is a separate step from
# checking it. This does both and nothing else: assemble the snapshot the way
# simulate_public_tree does, then hand it to public_snapshot_check.sh
# --audit-only.
#
# Not a pass when the tree cannot be built: a check that could not look is not a
# check that succeeded.
set -euo pipefail

WORTEL="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$WORTEL"

UIT="$(python3 scripts/ci/simulate_public_tree.py --keep 2>&1)" || {
    printf '%s\n' "$UIT" >&2
    echo "[snapshot-audit] SKIPPED (not a pass): the published tree could not be" >&2
    echo "  assembled, so nothing was audited." >&2
    exit 1
}
printf '%s\n' "$UIT"

BOOM="$(printf '%s\n' "$UIT" | sed -n 's/.*assembled [0-9]* file(s) at //p' | tail -1)"
if [ -z "$BOOM" ] || [ ! -d "$BOOM" ]; then
    echo "[snapshot-audit] SKIPPED (not a pass): could not read the tree's path out" >&2
    echo "  of simulate_public_tree's output, so nothing was audited." >&2
    exit 1
fi

trap 'rm -rf "$BOOM"' EXIT
bash scripts/release/public_snapshot_check.sh --audit-only "$BOOM"
