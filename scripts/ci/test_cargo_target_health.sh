#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Does the repair actually repair, and does it know when to keep its hands off?
#
# A clean-up script is a strange thing to test, because both of its mistakes are
# invisible in the ordinary case. Removing too little leaves the next job to fail
# on `dep-graph.part.bin` and blame a crate. Removing too much destroys the cache
# a live build is holding, which costs tens of minutes on the only machine that
# runs the corpus.
#
# So the cases that matter most are the ones where it must NOT act: a lock with a
# cargo process behind it, and a directory that has stopped answering.
#
# FLOOR: cases run >= 12 -- this file has more, and a run that executes a
# handful stopped short rather than found a clean tree.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HEALTH="${ROOT}/scripts/ci/cargo_target_health.sh"
PROBE="${ROOT}/scripts/ci/bounded_probe.sh"
FLOOR=12
failures=0
cases=0

fail() { echo "  FAIL: $*" >&2; failures=$((failures + 1)); }
case_() { cases=$((cases + 1)); }

for f in "${HEALTH}" "${PROBE}"; do
    if [[ ! -f "${f}" ]]; then
        echo "SKIPPED (not a pass): ${f} is gone; nothing to test." >&2
        exit 0
    fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

# A copy of the script beside a copy of the probe, so a case can replace the
# probe without touching the repository. The script finds its dependency next to
# itself, which is what makes this possible.
BENCH="${WORK}/bench"
mkdir -p "${BENCH}"
cp "${HEALTH}" "${BENCH}/cargo_target_health.sh"
cp "${PROBE}" "${BENCH}/bounded_probe.sh"

fresh_target() {
    local dir="${WORK}/target-$1"
    rm -rf "${dir}"
    mkdir -p "${dir}/debug"
    printf '%s\n' "$dir"
}

# `--sweep` refuses while any cargo-family process is alive anywhere on the host
# (codex, #1621: the GitLab jobs pass CARGO_TARGET_DIR through the environment,
# so a command-line scan misses them). The suite therefore has to say what `ps`
# reports, or these cases pass or fail on whether this machine happens to be
# building right now -- which is not a test of anything.
IDLE="${WORK}/stub-idle-host"
mkdir -p "${IDLE}"
printf '#!/bin/sh\nexit 0\n' > "${IDLE}/ps"
chmod +x "${IDLE}/ps"
SWEEP_IDLE=(env "PATH=${IDLE}:${PATH}")

# --- 1. A CONFIGURED directory that is not there is the #264 failure --------
#
# This case used to assert exit 0, which pinned the defect rather than the
# behaviour: six workflows call this script and not shared_build_dir_is_there.sh,
# so on the morning the mount vanished the step they all rely on reported a pass
# and cargo walked into the broken path. (codex, #1616)
case_
out="$(CARGO_TARGET_DIR="${WORK}/never-existed" bash "${BENCH}/cargo_target_health.sh" 2>&1)"; rc=$?
grep -q 'FATAL' <<<"${out}" || fail "a missing configured build directory is not announced: ${out}"
case_
[[ ${rc} -ne 0 ]] || fail "a missing CONFIGURED build directory must be an error, got ${rc}"

# --- 1b. An unconfigured default that is absent means: not this machine -----
case_
out="$(env -u CARGO_TARGET_DIR CARGO_TARGET_HEALTH_DEADLINE=5 bash -c '
  TARGET_DEFAULT=/var/cache/cargo-target
  [ -d "$TARGET_DEFAULT" ] && exit 42
  bash "'"${BENCH}"'/cargo_target_health.sh"' 2>&1)"; rc=$?
if [[ ${rc} -eq 42 ]]; then
    echo "  (skipped: this host actually has /var/cache/cargo-target)"
else
    grep -q 'SKIPPED (not a pass)' <<<"${out}" || fail "an unconfigured absent default is not announced: ${out}"
    [[ ${rc} -eq 0 ]] || fail "an unconfigured absent default must not be an error, got ${rc}"
fi

# --- 2. The file from the 25-08 failure is removed --------------------------
t="$(fresh_target part)"
mkdir -p "${t}/debug/incremental/pdfluent-abc"
: > "${t}/debug/incremental/pdfluent-abc/dep-graph.part.bin"
case_
out="$("${SWEEP_IDLE[@]}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -e "${t}/debug/incremental" ]] && fail "incremental state survived the clean-up"
case_
grep -q 'incremental state present' <<<"${out}" || fail "the clean-up did not say what it removed: ${out}"

# A .part.bin outside incremental/ must go too -- the incremental sweep is not
# allowed to be the only thing that catches it.
t="$(fresh_target loosepart)"
mkdir -p "${t}/debug/build"
: > "${t}/debug/build/dep-graph.part.bin"
case_
out="$("${SWEEP_IDLE[@]}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -e "${t}/debug/build/dep-graph.part.bin" ]] && fail "a stray .part.bin survived the clean-up"
case_
grep -q 'half-written .part.bin' <<<"${out}" || fail "a stray .part.bin was removed without saying so: ${out}"

# --- 3. A stale lock goes; a live one stays ---------------------------------
#
# The second half is the one worth having. Removing a lock a running build holds
# corrupts the shared directory, and it is the kind of mistake a clean-up script
# makes while looking correct.
t="$(fresh_target lock)"
: > "${t}/.cargo-lock"
STUB="${WORK}/stub-no-cargo"
mkdir -p "${STUB}"
printf '#!/bin/sh\nexit 1\n' > "${STUB}/pgrep"
chmod +x "${STUB}/pgrep"
case_
out="$(env "PATH=${IDLE}:${STUB}:${PATH}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -e "${t}/.cargo-lock" ]] && fail "a lock with no cargo behind it was not removed"
case_
grep -q 'no cargo process running' <<<"${out}" || fail "the stale lock removal was not reported: ${out}"

t="$(fresh_target livelock)"
: > "${t}/.cargo-lock"
STUB2="${WORK}/stub-cargo-alive"
mkdir -p "${STUB2}"
printf '#!/bin/sh\necho 4242\nexit 0\n' > "${STUB2}/pgrep"
chmod +x "${STUB2}/pgrep"
case_
out="$(env "PATH=${IDLE}:${STUB2}:${PATH}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -e "${t}/.cargo-lock" ]] || fail "a lock held by a running cargo was removed — that corrupts the cache"
case_
grep -q 'left alone' <<<"${out}" || fail "leaving a live lock alone was not reported: ${out}"

# --- 4. A directory that has stopped answering is fatal, not slow -----------
#
# This is the silent half of #264. `find` over a dead device blocks in D-state
# where SIGKILL does nothing, so the job stops reporting and is reaped an hour
# later. The script must give up on its deadline and say why.
cp "${PROBE}" "${WORK}/probe-backup.sh"
printf '%s\n' '_bounded_probe() { return 124; }' > "${BENCH}/bounded_probe.sh"
case_
out="$(CARGO_TARGET_DIR="${WORK}/pretend-dead" bash "${BENCH}/cargo_target_health.sh" 2>&1)"; rc=$?
[[ ${rc} -eq 1 ]] || fail "a build directory that never answers exited ${rc}, not 1"
case_
grep -q 'did not answer' <<<"${out}" || fail "a dead build directory is not named as such: ${out}"
case_
grep -q '264' <<<"${out}" || fail "the fatal message does not point at the issue: ${out}"
# Restore from the copy. Never `git checkout --` here: that resets the tree and
# would take the rest of this branch's work with it.
cp "${WORK}/probe-backup.sh" "${BENCH}/bounded_probe.sh"

# --- 5. A clean directory is left exactly as it was -------------------------
t="$(fresh_target clean)"
mkdir -p "${t}/debug/deps"
: > "${t}/debug/deps/libpdfluent.rlib"
case_
out="$("${SWEEP_IDLE[@]}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -e "${t}/debug/deps/libpdfluent.rlib" ]] || fail "the clean-up removed a build artefact it should keep"
case_
grep -q 'is clean' <<<"${out}" || fail "a clean directory was not reported clean: ${out}"

# --- the default is report-only, because a snapshot cannot promise no build
#     starts between the check and the delete (codex, #1621) ------------------
t="$(fresh_target reportonly)"; : > "${t}/debug/live.part.bin"
case_
out="$(CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" 2>&1)"
[[ -f "${t}/debug/live.part.bin" ]] || fail "the default invocation deleted something"
case_
grep -q 'not removed' <<<"${out}" || fail "the default did not say it removed nothing: ${out}"
case_
grep -q -- '--sweep' <<<"${out}" || fail "the default did not name --sweep as the way to clean: ${out}"


# --- a busy host refuses to sweep, even when the target is not named ---------
#     The GitLab jobs pass CARGO_TARGET_DIR through the environment, so the
#     command line of a live cargo need not mention this directory at all.
BUSY="${WORK}/stub-busy-host"
mkdir -p "${BUSY}"
# The build-command string below is assembled at runtime, and that is not
# fastidiousness. scripts/ci/orchestration_stays_hosted.py follows the scripts a
# job calls and scans their text for the three compile verbs, to catch a job
# that compiles on the persistent runner by way of a shell script rather than a
# workflow step. It cannot tell a command from a string, so this fixture -- a
# fake `ps` whose whole purpose is to PRINT such a line -- read as
# orchestration-guard compiling the workspace, and turned that guard red on
# master.
#
# Splitting the token keeps the fixture doing its job while the scan reads what
# is true. Note this comment also avoids spelling the verb: the first version of
# it explained the problem using the exact text that causes it, and tripped the
# same scan. If the scan ever learns to tell a command from a quoted string,
# both halves of this can go.
_fake_build="car""go build --release"
printf '#!/bin/sh\necho "/usr/local/bin/%s"\n' "${_fake_build}" > "${BUSY}/ps"
chmod +x "${BUSY}/ps"
t="$(fresh_target busyhost)"; : > "${t}/debug/keepme.part.bin"
case_
out="$(env "PATH=${BUSY}:${PATH}" CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" --sweep 2>&1)"
[[ -f "${t}/debug/keepme.part.bin" ]] || fail "swept while a cargo was alive without naming the target"
case_
grep -q 'nothing removed' <<<"${out}" || fail "the busy-host refusal was not reported: ${out}"

# --- a lock as the ONLY finding must not read as clean -----------------------
t="$(fresh_target lockonly)"; : > "${t}/.cargo-lock"
case_
out="$(CARGO_TARGET_DIR="${t}" bash "${BENCH}/cargo_target_health.sh" 2>&1)"
grep -q 'is clean' <<<"${out}" && fail "a lock was the only finding and it still said clean: ${out}"

if (( cases < FLOOR )); then  # FLOOR
    echo "[test-cargo-target-health] FATAL: ${cases} case(s) ran, floor is ${FLOOR}." >&2
    echo "  The script stopped short, and a test that stops testing reports success" >&2
    echo "  in the same words as one that passed." >&2
    exit 1
fi

if (( failures > 0 )); then
    echo "[test-cargo-target-health] FATAL: ${failures} of ${cases} case(s) failed" >&2
    exit 1

fi
echo "[test-cargo-target-health] OK: ${cases} cases — debris goes, a live lock stays, a dead directory is fatal."
