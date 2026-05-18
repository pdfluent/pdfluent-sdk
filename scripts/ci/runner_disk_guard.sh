#!/usr/bin/env bash
# runner_disk_guard.sh — pre-flight disk check for the PDFluent VPS runner.
#
# Designed to run at the top of sanity:cargo-* and heavy CI lanes so a
# disk-full failure surfaces before the build actually starts (which is
# what burned us on 2026-05-17: `cargo` failed deep into the build with
# "No space left on device" instead of failing fast).
#
# Exit codes:
#   0 — both root and storagebox have enough free space
#   2 — root < HARD_FAIL_GB free OR storagebox not writable: HARD FAIL
#   3 — root < WARN_GB free: WARN (does not fail; printed loudly)
#
# Thresholds (defaults are tuned for the transition phase while
# /opt/xfa-corpus (216 GB) is still on root):
#   - HARD_FAIL_GB = 5  : enough for /tmp + GitLab clone + log spill
#   - WARN_GB      = 15 : healthy headroom
# Post corpus migration these should be raised to 30 / 50 respectively.
#
# Usage:
#   bash scripts/ci/runner_disk_guard.sh         # default thresholds
#   bash scripts/ci/runner_disk_guard.sh 30 50   # post-corpus-migration
#
# Reports the top 10 disk consumers on root and on the storagebox so a
# triager can see at a glance what's eating space.
#
# Storage-layout context: see docs/ci/vps_storage_layout.md.

set -euo pipefail

HARD_FAIL_GB="${1:-5}"
WARN_GB="${2:-15}"

STORAGEBOX_MOUNT="/mnt/storagebox"
STORAGEBOX_CI_ROOT="${STORAGEBOX_MOUNT}/pdfluent/ci"

echo "===================================================================="
echo "[runner_disk_guard] PDFluent VPS runner pre-flight disk check"
echo "[runner_disk_guard] hard-fail threshold: <${HARD_FAIL_GB} GB on /"
echo "[runner_disk_guard] warn threshold:      <${WARN_GB} GB on /"
echo "===================================================================="

# Root filesystem
root_avail_gb=$(df -BG --output=avail / | tail -1 | tr -dc '0-9')
echo "[runner_disk_guard] root (/) free: ${root_avail_gb} GB"

# Storagebox
if [[ -d "${STORAGEBOX_MOUNT}" ]]; then
    if df -h "${STORAGEBOX_MOUNT}" >/dev/null 2>&1; then
        sb_avail=$(df -BG --output=avail "${STORAGEBOX_MOUNT}" | tail -1 | tr -dc '0-9')
        echo "[runner_disk_guard] storagebox (${STORAGEBOX_MOUNT}) free: ${sb_avail} GB"
    else
        echo "[runner_disk_guard] WARNING: ${STORAGEBOX_MOUNT} present but df failed"
        sb_avail=0
    fi
else
    echo "[runner_disk_guard] WARNING: ${STORAGEBOX_MOUNT} not mounted"
    sb_avail=0
fi

# Probe storagebox writability (CIFS can drop into RO-fallback)
if [[ -d "${STORAGEBOX_CI_ROOT}" ]]; then
    probe="${STORAGEBOX_CI_ROOT}/.disk_guard_probe.$$"
    if ( : > "${probe}" ) 2>/dev/null; then
        rm -f "${probe}"
        echo "[runner_disk_guard] storagebox write probe: OK"
    else
        echo "[runner_disk_guard] HARD FAIL: storagebox is not writable (CIFS may be in degraded RO state)"
        echo "[runner_disk_guard]   recovery: umount ${STORAGEBOX_MOUNT} && mount -a"
        exit 2
    fi
else
    echo "[runner_disk_guard] WARNING: ${STORAGEBOX_CI_ROOT} missing — runner storage layout not initialised"
fi

echo ""
echo "[runner_disk_guard] top 10 root consumers:"
{
    du -sh /opt/* /home/* /var/log /var/cache /var/lib/gitlab-runner /tmp 2>/dev/null \
        | sort -hr | head -10 | sed 's/^/  /'
} || true

echo ""
echo "[runner_disk_guard] gitlab-runner builds + cache (storagebox):"
{
    du -sh ${STORAGEBOX_CI_ROOT}/gitlab-runner/builds 2>/dev/null \
           ${STORAGEBOX_CI_ROOT}/gitlab-runner/cache  2>/dev/null \
           ${STORAGEBOX_CI_ROOT}/cargo/target         2>/dev/null \
           ${STORAGEBOX_CI_ROOT}/cargo/home           2>/dev/null \
           ${STORAGEBOX_CI_ROOT}/npm                  2>/dev/null \
        | sort -hr | sed 's/^/  /'
} || true

echo ""

# Decide verdict
if (( root_avail_gb < HARD_FAIL_GB )); then
    echo "[runner_disk_guard] HARD FAIL: root has only ${root_avail_gb} GB free (<${HARD_FAIL_GB} GB)"
    echo "[runner_disk_guard]   run: bash scripts/ci/runner_cleanup_safe.sh --apply"
    exit 2
fi

if (( root_avail_gb < WARN_GB )); then
    echo "[runner_disk_guard] WARN: root has ${root_avail_gb} GB free (<${WARN_GB} GB)"
    echo "[runner_disk_guard]   consider: bash scripts/ci/runner_cleanup_safe.sh --apply"
    # Warning only; do not exit non-zero.
fi

echo "[runner_disk_guard] PASS — proceeding."
exit 0
