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

# HARD_FAIL_GB raised 5 -> 10: a 5 GB floor let cold-cache Rust builds run the
# root partition out of space mid-compile (CARGO_TARGET_DIR alone is ~4 GB warm
# and grows several GB transiently). 10 GB leaves headroom for the build's peak;
# WARN 20 nudges a cleanup before it gets tight. Override via positional args.
HARD_FAIL_GB="${1:-10}"
WARN_GB="${2:-20}"

STORAGEBOX_MOUNT="/mnt/storagebox"
STORAGEBOX_CI_ROOT="${STORAGEBOX_MOUNT}/pdfluent/ci"

echo "===================================================================="
echo "[runner_disk_guard] PDFluent VPS runner pre-flight disk check"
echo "[runner_disk_guard] hard-fail threshold: <${HARD_FAIL_GB} GB"
echo "[runner_disk_guard] warn threshold:      <${WARN_GB} GB"
echo "===================================================================="

# Root filesystem.
#
# On WSL the root filesystem is a sparse virtual disk (ext4.vhdx) that lives on
# the Windows volume and grows on demand. `df /` therefore reports the vhdx's
# MAXIMUM size — measured 945 GB "free" while the host volume had only 102 GB
# left. Guarding on that number is worse than not guarding: it reports healthy
# right up to the moment the host volume fills and WSL, Windows and the build
# all fail together.
#
# So on WSL the binding constraint is the Windows volume, and that is what we
# check. Elsewhere (the VPS, any normal Linux host) nothing changes.
GUARD_FS="/"
GUARD_LABEL="root (/)"
if [[ -r /proc/version ]] && grep -qiE "microsoft|wsl" /proc/version 2>/dev/null; then
    for host_mount in /mnt/c /mnt/d; do
        if [[ -d "${host_mount}" ]] && df -BG "${host_mount}" >/dev/null 2>&1; then
            GUARD_FS="${host_mount}"
            GUARD_LABEL="Windows-volume (${host_mount}) — WSL root is a sparse vhdx on it"
            break
        fi
    done
    if [[ "${GUARD_FS}" == "/" ]]; then
        echo "[runner_disk_guard] WARNING: WSL detected but no host volume found;"
        echo "[runner_disk_guard]   falling back to / , which over-reports free space"
    fi
fi

root_avail_gb=$(df -BG --output=avail "${GUARD_FS}" | tail -1 | tr -dc '0-9')
echo "[runner_disk_guard] ${GUARD_LABEL} free: ${root_avail_gb} GB"
df -h "${GUARD_FS}" | sed 's/^/[runner_disk_guard]   /'
echo "[runner_disk_guard] cache env: CARGO_HOME=${CARGO_HOME:-unset} CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-unset} TMPDIR=${TMPDIR:-unset}"

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
elif [[ "${CARGO_TARGET_DIR:-}" == "${STORAGEBOX_MOUNT}"/* ]]; then
    # Caches are configured to live on the storagebox but the layout is not
    # there. That is a genuine fault, and it is worth being explicit about.
    echo "[runner_disk_guard] WARNING: ${STORAGEBOX_CI_ROOT} missing while CARGO_TARGET_DIR points at the storagebox"
    echo "[runner_disk_guard]   runner storage layout not initialised"
else
    # Not a fault: the WSL desktop runner keeps its caches on the internal SSD
    # on purpose (cargo's SQLite locking breaks on external/network storage),
    # so it has no storagebox CI layout at all. Warning about a layout this
    # host deliberately does not use is noise, and noise is what makes real
    # warnings easy to miss.
    echo "[runner_disk_guard] storagebox CI layout not in use on this host (caches: ${CARGO_TARGET_DIR:-unset})"
fi

# Observability (warn-only): surface a concurrent corpus-archival `tar`.
# The CI builds_dir lives on the CIFS storagebox; a corpus archive writing to the
# same mount starves get_sources / working-tree checkout and has caused branch
# pipelines to time out and be cancelled (see D13A pipeline-failure diagnosis:
# benchmarks/runs/xfa_enterprise_plan/d13a_pipeline_failure_fix_and_cli_next_step).
# This NEVER changes the verdict — it only makes the cause visible to a triager.
archive_procs="$(pgrep -fl 'tar.*xfa-corpus' 2>/dev/null || true)"
if [[ -n "${archive_procs}" ]]; then
    echo ""
    echo "[runner_disk_guard] WARN: a corpus-archival tar appears to be running:"
    echo "${archive_procs}" | sed 's/^/  /'
    echo "[runner_disk_guard]   CI builds_dir is on CIFS (${STORAGEBOX_MOUNT}); a concurrent"
    echo "[runner_disk_guard]   archive write to the same mount can slow get_sources/checkout"
    echo "[runner_disk_guard]   and cause pipeline timeouts/cancellations. Serialise them."
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

# Surface anything the periodic monitor saw between pipelines.
#
# check-disk.sh runs from cron every 30 minutes and drops this marker when a
# volume crosses its threshold. Printing it here is the point: the old monitor
# wrote warnings to a logfile nobody opened, so a filling disk stayed invisible
# until a build died on it. Job output is a place we actually look.
DISK_MARKER="${DISK_MARKER_DIR:-/var/tmp/xfa-disk-monitor}/breach"
if [[ -f "${DISK_MARKER}" ]]; then
    echo "[runner_disk_guard] ATTENTION: the periodic disk monitor recorded a breach"
    sed 's/^/[runner_disk_guard]   /' "${DISK_MARKER}"
    echo "[runner_disk_guard]   (marker clears itself once usage drops back under threshold)"
    echo ""
fi

# Decide verdict
if (( root_avail_gb < HARD_FAIL_GB )); then
    echo "[runner_disk_guard] HARD FAIL: ${GUARD_LABEL} has only ${root_avail_gb} GB free (<${HARD_FAIL_GB} GB)"
    echo "[runner_disk_guard]   run: bash scripts/ci/runner_cleanup_safe.sh --apply"
    exit 2
fi

if (( root_avail_gb < WARN_GB )); then
    echo "[runner_disk_guard] WARN: ${GUARD_LABEL} has ${root_avail_gb} GB free (<${WARN_GB} GB)"
    echo "[runner_disk_guard]   consider: bash scripts/ci/runner_cleanup_safe.sh --apply"
    # Warning only; do not exit non-zero.
fi

echo "[runner_disk_guard] PASS — proceeding."
exit 0
