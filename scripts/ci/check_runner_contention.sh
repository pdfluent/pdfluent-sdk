#!/usr/bin/env bash
# check_runner_contention.sh — read-only pre-flight that warns about CI runner
# storage contention BEFORE a job's get_sources/build runs.
#
# Warns (does NOT fail by default) if:
#   - an XFA corpus archive tar is active (competes for the CIFS mount);
#   - root free space is below a threshold;
#   - the storagebox CIFS mount is present but not writable;
#   - the GitLab runner builds_dir appears to be on the CIFS mount.
#
# Safe on any machine: no secrets are read, no root required, missing inputs are
# treated as "unknown/skip", and absent tools degrade gracefully. It NEVER reads
# /etc/gitlab-runner/config.toml token lines.
#
# Exit codes:
#   0 — OK, or warnings only (default warn-only mode)
#   3 — contention detected AND fail-mode opted in via RUNNER_CONTENTION_FAIL=1
#
# Usage:
#   bash scripts/ci/check_runner_contention.sh            # warn-only
#   RUNNER_CONTENTION_FAIL=1 bash scripts/ci/check_runner_contention.sh   # fail on contention
#   ROOT_MIN_GB=30 bash scripts/ci/check_runner_contention.sh             # custom threshold

set -uo pipefail

ROOT_MIN_GB="${ROOT_MIN_GB:-15}"
STORAGEBOX_MOUNT="${STORAGEBOX_MOUNT:-/mnt/storagebox}"
FAIL_MODE="${RUNNER_CONTENTION_FAIL:-0}"

warnings=0
note() { echo "[check_runner_contention] $*"; }
warn() { echo "[check_runner_contention] WARN: $*" >&2; warnings=$((warnings + 1)); }

note "read-only CI runner contention pre-flight (warn-only unless RUNNER_CONTENTION_FAIL=1)"

# 1. Active corpus archive tar?
if command -v pgrep >/dev/null 2>&1; then
    if pgrep -fa 'tar .*xfa-corpus|xfa-corpus-[0-9].*\.tar' >/dev/null 2>&1; then
        warn "an XFA corpus archive tar appears to be active — it competes with CI for the CIFS mount; avoid running CI now"
    else
        note "no active corpus archive tar detected"
    fi
else
    note "pgrep unavailable — skipping corpus-tar check"
fi

# 2. Root free space.
if df -BG / >/dev/null 2>&1; then
    root_free=$(df -BG --output=avail / 2>/dev/null | tail -1 | tr -dc '0-9')
    if [ -n "${root_free:-}" ]; then
        if [ "$root_free" -lt "$ROOT_MIN_GB" ]; then
            warn "root has ${root_free} GB free (< ${ROOT_MIN_GB} GB) — risk of mid-build disk exhaustion"
        else
            note "root free: ${root_free} GB (>= ${ROOT_MIN_GB} GB)"
        fi
    fi
else
    note "df unavailable — skipping root-space check"
fi

# 3. Storagebox present but not writable?
if [ -d "$STORAGEBOX_MOUNT" ]; then
    probe="${STORAGEBOX_MOUNT}/.contention_probe.$$"
    if ( : >"$probe" ) 2>/dev/null; then
        rm -f "$probe" 2>/dev/null
        note "storagebox writable"
    else
        warn "storagebox ${STORAGEBOX_MOUNT} present but not writable (CIFS may be in degraded RO state)"
    fi
else
    note "storagebox ${STORAGEBOX_MOUNT} not mounted here — skipping (likely not the VPS runner)"
fi

# 4. builds_dir on CIFS? (read only the builds_dir line; never token/secret lines)
cfg="/etc/gitlab-runner/config.toml"
if [ -r "$cfg" ]; then
    if grep -E '^\s*builds_dir' "$cfg" 2>/dev/null | grep -q "$STORAGEBOX_MOUNT"; then
        warn "a runner builds_dir is on the CIFS mount (${STORAGEBOX_MOUNT}) — see CI_RUNNER_BUILDS_DIR_MIGRATION_RUNBOOK.md"
    else
        note "no builds_dir on CIFS detected"
    fi
else
    note "runner config not readable here — skipping builds_dir check"
fi

if [ "$warnings" -eq 0 ]; then
    note "OK — no contention signals detected"
    exit 0
fi

note "${warnings} contention signal(s) detected"
if [ "$FAIL_MODE" = "1" ]; then
    note "RUNNER_CONTENTION_FAIL=1 → exiting 3"
    exit 3
fi
note "warn-only mode → exiting 0 (set RUNNER_CONTENTION_FAIL=1 to fail)"
exit 0
