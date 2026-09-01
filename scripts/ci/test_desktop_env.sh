#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
#
# Does the shared build directory say which of the three went wrong?
#
# Twice on 27-08-2026 the shared build directory was simply not mounted, and the
# error that reached the log was `Permission denied (os error 13)`. That points
# at ownership while the cause is a mount, and the diagnosis started from the
# wrong end both times (#264).
#
# On 30-08-2026 the same path failed a third way, and it was the expensive one:
# the mount was in /proc/self/mountinfo, the device answered nothing, and every
# process that touched it went into D-state and stayed there. That is what makes
# a job stop reporting until GitLab reaps it an hour later.
#
# These cases build all three situations and check the verdict names the right
# one. They also check the fatal path is fatal, and that the probe which asks
# the question never joins the outage it is asking about.
#
# FLOOR: cases run >= 10 -- this file has more, and a run that executes a
# handful has lost its way through the script rather than found a clean tree.
# A test that quietly stops testing reports success in the same words as one
# that passed.

set -uo pipefail
WORTEL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BRON="${WORTEL}/scripts/ci/desktop_env.sh"
VERDICT="${WORTEL}/scripts/ci/why_not_writable.sh"
PROBE="${WORTEL}/scripts/ci/bounded_probe.sh"
CHECKER="${WORTEL}/scripts/ci/shared_build_dir_is_there.sh"
VLOER=10
fouten=0
gevallen=0

klaag() { echo "  FAIL: $*" >&2; fouten=$((fouten + 1)); }
geval() { gevallen=$((gevallen + 1)); }

for bestand in "$VERDICT" "$PROBE" "$CHECKER"; do
    if [[ ! -f "$bestand" ]]; then
        echo "SKIPPED (not a pass): ${bestand} is gone; nothing to test." >&2
        exit 0
    fi
done

# A workbench with a mount table we control. Without one the cases would depend
# on what happens to be mounted on the machine running them, and the situation
# that matters most -- a storage root that exists on disk and is NOT mounted --
# cannot be built under /mnt without root.
WERK="$(mktemp -d)"
trap 'chmod -R u+rwX "$WERK" 2>/dev/null; rm -rf "$WERK"' EXIT
MNT="${WERK}/mnt"
mkdir -p "${MNT}/storagebox"

printf '%s\n' \
    "1 0 8:1 / / rw - ext4 /dev/sda1 rw" \
    "2 1 0:2 / ${MNT} rw - tmpfs none rw" > "${WERK}/zonder"
printf '%s\n' \
    "1 0 8:1 / / rw - ext4 /dev/sda1 rw" \
    "2 1 0:2 / ${MNT} rw - tmpfs none rw" \
    "3 1 8:65 / ${MNT}/storagebox rw - ext4 /dev/sde1 rw" > "${WERK}/met"

# `voor <mountinfo> <extra shell> <doel>`: run the verdict in a shell of its own
# with the mount table injected. The extra shell runs after sourcing, which is
# how the "does not answer" case stubs the probe -- shell functions are looked
# up when they are called, so a later definition wins.
voor() {
    local info="$1" extra="$2" doel="$3"
    ( WNW_MOUNTINFO="$info" WNW_OPSLAG_PREFIX="$MNT" WNW_DEADLINE=3 \
      bash -c ". '${VERDICT}'; ${extra}; _waarom_niet '${doel}'" ) 2>&1
}

# 1. A storage root that is not in the mount table, and not on disk either.
geval
uit="$(voor "${WERK}/zonder" ":" "${MNT}/erisgeenstorage/cargo-slots/target-0")"
grep -q 'NOT MOUNTED' <<<"$uit" || klaag "an absent storage root is not called a mount problem: ${uit}"
grep -q 'not a permissions problem' <<<"$uit" || klaag "it does not say permissions are ruled out"

# 2. The regression that made the previous attempt at this wrong. The storage
#    root EXISTS on disk and holds files -- an unmounted mount point is a
#    root-owned directory like any other, and anything may have written into it
#    since. Deciding by "is it empty" calls this ownership. The mount table
#    calls it what it is.
geval
mkdir -p "${MNT}/storagebox/restant"
echo rommel > "${MNT}/storagebox/restant/achtergebleven.txt"
uit="$(voor "${WERK}/zonder" ":" "${MNT}/storagebox/cargo-slots/target-0")"
grep -q 'NOT MOUNTED' <<<"$uit" || klaag "a non-empty leftover mount point is not called unmounted: ${uit}"
geval
grep -q 'not writable by' <<<"$uit" && klaag "a missing mount is blamed on ownership: ${uit}"

# 3. Mounted, answering, and genuinely not writable: this one IS permissions.
verboden="${MNT}/storagebox/verboden"
mkdir -p "$verboden"; chmod 500 "$verboden"
if [[ $(id -u) -eq 0 ]]; then
    echo "SKIPPED (not a pass): running as root, chmod 500 does not keep root out." >&2
else
    geval
    uit="$(voor "${WERK}/met" ":" "${verboden}/kind")"
    grep -q 'not writable by' <<<"$uit" || klaag "a genuine permissions problem is not named: ${uit}"
    geval
    grep -q 'NOT MOUNTED' <<<"$uit" && klaag "a permissions problem is called a mount problem: ${uit}"
fi
chmod 700 "$verboden" 2>/dev/null

# 4. Mounted, and the device answers nothing. Neither of the other two verdicts
#    is true of this, and it is the one that costs an hour: the job does not
#    fail, it stops existing.
geval
uit="$(voor "${WERK}/met" "_bounded_probe() { return 124; }" "${MNT}/storagebox/cargo-slots/target-0")"
grep -q 'does NOT ANSWER' <<<"$uit" || klaag "a mounted but dead device is not named: ${uit}"
geval
grep -q 'not writable by' <<<"$uit" && klaag "a dead device is blamed on ownership: ${uit}"
geval
grep -q 'NOT MOUNTED' <<<"$uit" && klaag "a dead device is called an absent mount: ${uit}"

# 5. The probe returns on time even when its child does not. This is the whole
#    reason the parent holds the clock: a D-state child cannot be killed, so
#    `timeout` waits out the outage and everything behind it waits too.
geval
start=$(date +%s)
( . "$PROBE"; _bounded_probe 1 sleep 10 ) >/dev/null 2>&1
rc=$?
duur=$(( $(date +%s) - start ))
[[ $rc -eq 124 ]] || klaag "a probe that outran its deadline returned ${rc}, not 124"
geval
[[ $duur -le 4 ]] || klaag "the probe waited ${duur}s on a 1s deadline — the parent joined the child"

# 6. And it is still an ordinary command runner: output and exit code survive.
geval
uit="$( . "$PROBE"; _bounded_probe 5 echo hallo )"
[[ "$uit" == "hallo" ]] || klaag "the probe swallowed its command's output: '${uit}'"
geval
( . "$PROBE"; _bounded_probe 5 false ) >/dev/null 2>&1
[[ $? -eq 1 ]] || klaag "the probe did not pass through a non-zero exit code"

# 7. No mount table, no verdict about mounts. Guessing "not mounted" from no
#    evidence points the next person down a different dead end, which is the
#    same defect in the other direction.
geval
uit="$(voor "${WERK}/bestaat-niet" ":" "${MNT}/storagebox/cargo-slots/target-0")"
grep -q 'SKIPPED (not a pass)' <<<"$uit" || klaag "an unreadable mount table is not announced: ${uit}"

# 8. The fatal path is still fatal under CI, and carries its verdict. Without
#    it the log says "cannot create <path>" and the next person starts at the
#    wrong end again, which is the whole of #264.
geval
uit="$(CI=1 CARGO_TARGET_DIR=/mnt/erisgeenstorage/cargo-slots/target-0 \
       bash -c ". '${BRON}'" 2>&1)"
rc=$?
[[ $rc -ne 0 ]] || klaag "sourcing with an uncreatable CARGO_TARGET_DIR under CI exits 0"
geval
grep -q 'FATAL' <<<"$uit" || klaag "the failure is not announced as FATAL: ${uit}"
geval
grep -q 'VERDICT\|SKIPPED (not a pass)' <<<"$uit" || klaag "the fatal path reaches no verdict: ${uit}"

# 9. The standalone checker fails on a directory it cannot create and passes on
#    one it can.
geval
uit="$(bash "$CHECKER" /mnt/erisgeenstorage/target-0 2>&1)"; rc=$?
[[ $rc -ne 0 ]] || klaag "the checker exits 0 on a directory it cannot create"
geval
goed="$(mktemp -d)/wel"
bash "$CHECKER" "$goed" >/dev/null 2>&1 || klaag "the checker fails on a directory it can create"

# 10. Both of the checker's filesystem calls go through the deadline.
#
#     This reads the source rather than staging an outage, and it is worth being
#     honest about what that does and does not prove: it cannot show the bound
#     works -- case 5 does that -- only that the checker still uses it. That is
#     the regression worth catching, because a bare `mkdir -p` looks completely
#     ordinary in a diff and reintroduces the hour-long silence.
geval
if grep -qE '^[[:space:]]*(if )?mkdir -p "\$\{doel\}"' "$CHECKER"; then
    klaag "the checker calls mkdir without a deadline again; a dead mount will hang it"
fi
geval
[[ $(grep -c '_bounded_probe' "$CHECKER") -ge 2 ]] || \
    klaag "the checker no longer bounds both of its filesystem calls"

if (( gevallen < VLOER )); then  # FLOOR
    echo "[test-desktop-env] FATAL: ${gevallen} case(s) ran, floor is ${VLOER}. The" >&2
    echo "  script stopped short, and a test that stops testing reports success in" >&2
    echo "  the same words as one that passed." >&2
    exit 1
fi

if [[ $fouten -gt 0 ]]; then
    echo "[test-desktop-env] FATAL: ${fouten} of ${gevallen} case(s) failed" >&2
    exit 1
fi
echo "[test-desktop-env] OK: ${gevallen} cases — mount / dead device / permission are told apart, the probe outlives its child, and the fatal path is fatal."
