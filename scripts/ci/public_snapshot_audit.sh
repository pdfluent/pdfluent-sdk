#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the published tree and check it (#222).
#
# The gate line needs one command, and producing the tree is a separate step from
# checking it. This does both and nothing else: assemble the snapshot the way
# simulate_public_tree does, then hand it to public_snapshot_check.sh.
#
# TWO MODES, BECAUSE THE TWO HALVES COST TWO ORDERS OF MAGNITUDE APART
#
#   (default)     the path audit. 154 seconds, measured: assemble and grep. Runs
#                 in the pre-push gate on every push.
#   --with-build  the audit AND the clean-clone build with an empty HOME. Minutes
#                 and gigabytes, so it runs on a push to master, on our own
#                 runner, and nowhere else.
#
# The build half existed and ran in no job at all. `public_snapshot_check.sh`
# explains why -- minutes and gigabytes on every push -- and that is an argument
# for a master-push job, not for running nowhere: #222's own gate is "somebody
# who clones the repository can build the SDK and run the smoke tests", and until
# 05-09-2026 the only thing that had ever answered it was a person running it by
# hand.
#
# Not a pass when the tree cannot be built: a check that could not look is not a
# check that succeeded.
set -euo pipefail

MODUS="--audit-only"
if [ "${1-}" = "--with-build" ]; then
    MODUS=""
    shift
fi

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
# shellcheck disable=SC2086  # MODUS is one optional flag or nothing at all.
bash scripts/release/public_snapshot_check.sh $MODUS "$BOOM"
