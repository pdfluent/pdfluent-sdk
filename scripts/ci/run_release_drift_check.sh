#!/usr/bin/env bash
# run_release_drift_check.sh — release-channel drift check for scheduled CI.
#
# Run this in a scheduled GitLab CI job (hourly / daily). It fetches the
# live state of every channel via registry APIs and compares to the
# canonical manifest. Exits non-zero on FAIL findings; that surfaces as
# a pipeline failure + alert without blocking per-push pipelines.
#
# This is INTENTIONALLY not wired into the pre-push hook — registry
# hiccups would create false-positive blockers for individual developers.
# The drift check belongs on a schedule that the operator monitors.
#
# Usage:
#   bash scripts/ci/run_release_drift_check.sh         # human-readable
#   bash scripts/ci/run_release_drift_check.sh --json  # machine-readable
#
# Exit:
#   0  no FAIL findings
#   1  one or more FAIL findings (drift outside threshold)

set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

if [[ "${1:-}" == "--json" ]]; then
  exec python3 scripts/release/release_train.py drift --json
fi

echo "=== Release-channel drift check ==="
echo "Time: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
echo
python3 scripts/release/release_train.py drift
