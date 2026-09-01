#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
#
# Recognise a damaged shared build directory and repair it, instead of letting
# the next job fail on a message that points at the wrong thing.
#
# WHY THIS EXISTS
#
# On 25-08-2026 `sanity:cookbook-examples` failed with:
#
#   error: failed to move dependency graph from
#     `<shared>/debug/incremental/pdfluent-.../dep-graph.part.bin`
#     to `.../dep-graph.bin`: No such file or directory (os error 2)
#
# There was nothing wrong with the code. The shared build directory is used by
# every job on this host. Abort a pipeline and its cargo process keeps grinding;
# kill that process and half-written state stays behind, and the next build
# trips over it and reports the failure as though the crate were broken. That is
# the most expensive kind of red: it points somewhere else.
#
# CARGO_INCREMENTAL=0 removes the cause. This removes the risk that remains,
# because the directory is still a shared resource that concurrent jobs write
# into, and because a job may override the setting.
#
# WHAT IT DOES
#
# Finds and removes three kinds of debris. All three are harmless to delete --
# cargo rebuilds them. What must NOT go is the directory itself: refilling it
# costs tens of minutes of runner time on the one machine that also runs the
# corpus measurements.
#
# ORDER MATTERS. Run `shared_build_dir_is_there.sh` before this, never after.
# Every command below touches the filesystem, and on a device that has stopped
# answering, `find` blocks in D-state where SIGKILL does nothing -- the silent
# half of #264. The probe here bounds each call as well, so this file is safe on
# its own, but a job that checks first gets a verdict instead of a timeout.
set -uo pipefail

# shellcheck source=scripts/ci/bounded_probe.sh
. "$(dirname "${BASH_SOURCE[0]}")/bounded_probe.sh"

TARGET="${CARGO_TARGET_DIR:-/var/cache/cargo-target}"
DEADLINE="${CARGO_TARGET_HEALTH_DEADLINE:-60}"

_bounded_probe "${DEADLINE}" test -d "${TARGET}"
seen=$?
if [ "${seen}" -eq 124 ]; then
    echo "[cargo-target-health] FATAL: ${TARGET} did not answer within ${DEADLINE}s." >&2
    echo "[cargo-target-health]   The device is failing I/O while the mount stays in the" >&2
    echo "[cargo-target-health]   table. Run scripts/ci/shared_build_dir_is_there.sh for the" >&2
    echo "[cargo-target-health]   full verdict. (#264)" >&2
    exit 1
fi
if [ "${seen}" -ne 0 ]; then
    # Exiting 0 here was the fault this whole file exists to prevent, one level
    # up. Six workflows -- bench, crash-guard, enterprise-acceptance, gate-ci,
    # publish-crates, wasm-gate -- call this script and NOT
    # shared_build_dir_is_there.sh, though the note above says to run that
    # first. So on the morning the mount is gone, the step every one of them
    # relies on reported a pass and cargo walked into the broken path. That is
    # #264 reported as green. (codex, #1616)
    #
    # The split that keeps this honest: a directory somebody CONFIGURED and
    # which is not there is the failure. An unconfigured default that is not
    # there means you are not on the shared-build machine at all, and failing a
    # contributor's clone for that helps nobody.
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        echo "[cargo-target-health] FATAL: CARGO_TARGET_DIR=${TARGET} is set and the" >&2
        echo "[cargo-target-health]   directory is not there. That is the missing mount of" >&2
        echo "[cargo-target-health]   #264, not an absent option. Run" >&2
        echo "[cargo-target-health]   scripts/ci/shared_build_dir_is_there.sh for which of the" >&2
        echo "[cargo-target-health]   three it is: gone, dead, or unwritable." >&2
        exit 1
    fi
    echo "SKIPPED (not a pass): ${TARGET} does not exist and CARGO_TARGET_DIR is unset," >&2
    echo "  so this is not the shared-build machine and there is nothing to check." >&2
    exit 0
fi

# P1: nothing below deletes anything while a cargo is running anywhere on this
# host. The reasoning was already written down for `.cargo-lock` -- "a lock
# removed out from under a live build corrupts the directory this file exists to
# protect" -- and then the two deletions above it did not apply it. A live
# `.part.bin` removed before cargo renames it reproduces exactly the 25-08
# failure this script repairs. (codex, #1616)
#
# The concurrency groups do not help: GitHub and GitLab jobs share this machine
# and each other's target directory, and neither knows about the other's lock.
# Scoped to THIS directory, not to the machine. `pgrep -x cargo` was the first
# attempt and it is useless here: this host runs GitHub and GitLab jobs and
# several worktrees at once, so some cargo is nearly always alive and the script
# would never clean anything again. What matters is whether a build is using
# *this* target -- which the rustc command lines say, because cargo passes
# `--out-dir <target>/debug/deps` and `-L dependency=<target>/...`.
_target_in_use() {
    ps -Ao args= 2>/dev/null | grep -F -- "${TARGET}" | grep -qvE '^\s*(grep|ps)\b'
}
# This is a snapshot, not a lock, and it is worth being plain about that: a
# build can start in the moment between this check and the deletions below. The
# finding offered two remedies -- take an exclusive lock, or establish no cargo
# is using the target -- and this is the second. It closes the common case (a
# job running on the shared machine) and not the rare one (a job starting during
# the sweep). A real cross-CI lock is the fix if this ever bites; it has not
# yet, and I would rather leave the limitation written down than implied.
if _target_in_use; then
    echo "[cargo-target-health] a build is using ${TARGET} — nothing removed."
    echo "[cargo-target-health]   Debris is cheap to leave and expensive to delete from"
    echo "[cargo-target-health]   under a live build: a .part.bin removed before cargo"
    echo "[cargo-target-health]   renames it is the 25-08 failure this script repairs."
    echo "[cargo-target-health]   Re-run when this directory is idle."
    exit 0
fi

cleaned=0

# 1. Half-written incremental state. Since CARGO_INCREMENTAL=0 this should not
#    be here at all; if it is, it predates that setting or a job overrode it.
#    It is also the single largest reclaimable item on this host -- 23 GB across
#    four checkouts when it was last measured.
if [ -d "${TARGET}/debug/incremental" ] || [ -d "${TARGET}/release/incremental" ]; then
    n=$(_bounded_probe "${DEADLINE}" find "${TARGET}"/*/incremental -maxdepth 1 -type d 2>/dev/null | wc -l | tr -d ' ')
    echo "[cargo-target-health] incremental state present (${n} directories) while CARGO_INCREMENTAL=0 — removed"
    if ! _bounded_probe "${DEADLINE}" rm -rf "${TARGET}/debug/incremental" "${TARGET}/release/incremental" >/dev/null 2>&1; then
        # 124 is the probe's timeout, anything else is a real rm failure. Either
        # way the debris is still there, and saying "removed" would send the next
        # build into the files this step claims to have cleared. (codex, #1616)
        echo "[cargo-target-health] FATAL: could not remove the incremental state." >&2
        echo "[cargo-target-health]   The subtree is unresponsive or not ours to delete;" >&2
        echo "[cargo-target-health]   the initial probe does not walk into it, so this is the" >&2
        echo "[cargo-target-health]   first place it shows. (#264)" >&2
        exit 1
    fi
    cleaned=$((cleaned + 1))
fi

# 2. Leftover `.part.bin` — exactly the file named in the 25-08 failure.
part=$(_bounded_probe "${DEADLINE}" find "${TARGET}" -name '*.part.bin' -type f 2>/dev/null | head -50)
if [ -n "${part}" ]; then
    n=$(echo "${part}" | wc -l | tr -d ' ')
    echo "[cargo-target-health] ${n} half-written .part.bin — removed"
    if ! echo "${part}" | xargs rm -f 2>/dev/null; then
        echo "[cargo-target-health] FATAL: could not remove ${n} .part.bin file(s)." >&2
        exit 1
    fi
    cleaned=$((cleaned + 1))
fi

# 3. A lock held by a process that no longer exists. An aborted pipeline leaves
#    its cargo running; kill that, and the lock stays behind and the next job
#    waits on it forever.
#
#    `pgrep` is machine-wide on purpose. Any cargo anywhere on this host might
#    be the owner, and a lock removed out from under a live build corrupts the
#    directory this file exists to protect. Leaving a stale lock costs one
#    job; removing a live one costs the cache.
if [ -f "${TARGET}/.cargo-lock" ]; then
    if ! pgrep -x cargo >/dev/null 2>&1; then
        echo "[cargo-target-health] .cargo-lock with no cargo process running — removed"
        rm -f "${TARGET}/.cargo-lock"
        cleaned=$((cleaned + 1))
    else
        echo "[cargo-target-health] .cargo-lock present and cargo is running — left alone"
    fi
fi

if [ "${cleaned}" -eq 0 ]; then
    echo "[cargo-target-health] ${TARGET} is clean"
else
    echo "[cargo-target-health] ${cleaned} kind(s) of debris removed; the next build starts clean"
fi
exit 0
