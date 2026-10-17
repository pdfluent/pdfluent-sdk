#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# apt-get on a machine that has just booted, where something else holds the
# lock. Waits for it; does not paper over anything else.
#
# THE FAILURE THIS COMES FROM. Wave 34163246411 bought a ccx43, and three steps
# of it died within seventy seconds of each other on the same two lines:
#
#     E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by
#        process 42140 (unattended-upgr)
#
# Ubuntu starts `unattended-upgrades` on boot. A throwaway instance is at its
# first minutes of uptime for the whole of a wave, so that is exactly when our
# apt-get calls arrive -- and two jobs of the same wave run on the one instance,
# so they also collide with each other. `Install veraPDF and mutool` fell over,
# the PDF/A gate behind it then found no validator, and the extraction step fell
# over on its own apt-get. Three red steps, one lock.
#
# Retrying is right for this failure and wrong for every other one, so this
# retries the lock and nothing else: an install that fails because the package
# does not exist fails here on the first attempt, with its own message. A blanket
# retry would turn a real failure into ten minutes of grinding followed by the
# same red, with the cause pushed off the top of the log.
#
# Usage:  bash scripts/ci/apt_get.sh update -qq
#         bash scripts/ci/apt_get.sh install -y -qq mupdf-tools unzip
#
# Environment:
#   APT_LOCK_DEADLINE_S  how long to keep waiting for the lock (default 600)
#   APT_LOCK_INTERVAL_S  seconds between attempts (default 5)
set -euo pipefail

DEADLINE_S="${APT_LOCK_DEADLINE_S:-600}"
INTERVAL_S="${APT_LOCK_INTERVAL_S:-5}"

if [ "$#" -eq 0 ]; then
    echo "[apt] usage: apt_get.sh <apt-get arguments...>" >&2
    exit 2
fi

# The runner user on the ephemeral image may already be root, in which case
# there is no sudo to call and asking for one fails for the wrong reason.
SUDO=""
[ "$(id -u)" -eq 0 ] || SUDO="sudo"

# What apt says when it is the lock, in the wordings apt and dpkg use for it.
# Matched against the whole output, because `apt-get update` prints it on
# stdout and `apt-get install` on stderr.
LOCK_SAYS='Could not get lock|Unable to acquire the dpkg frontend lock|dpkg frontend lock|Waiting for cache lock|is another process using it'

output="$(mktemp)"
trap 'rm -f "${output}"' EXIT

started=$SECONDS
attempt=0
while :; do
    attempt=$((attempt + 1))
    rc=0
    $SUDO apt-get "$@" >"${output}" 2>&1 || rc=$?

    if [ "${rc}" -eq 0 ]; then
        cat "${output}"
        [ "${attempt}" -eq 1 ] || echo "[apt] the lock came free; apt-get succeeded on attempt ${attempt}"
        exit 0
    fi

    if ! grep -Eq "${LOCK_SAYS}" "${output}"; then
        cat "${output}" >&2
        echo "[apt] FAILED with exit ${rc}, and not on the lock -- so this is the" >&2
        echo "[apt] answer, not a wait. Read the apt output above." >&2
        exit "${rc}"
    fi

    waited=$((SECONDS - started))
    if [ "${waited}" -ge "${DEADLINE_S}" ]; then
        cat "${output}" >&2
        echo "[apt] FATAL: the dpkg lock was still held after ${waited}s over" >&2
        echo "[apt] ${attempt} attempt(s). Something is holding it for longer than a" >&2
        echo "[apt] boot-time unattended-upgrades run, and waiting more would only" >&2
        echo "[apt] move the failure to the job timeout." >&2
        exit "${rc}"
    fi

    echo "[apt] the dpkg lock is held by another process (attempt ${attempt}, ${waited}s of ${DEADLINE_S}s); retrying in ${INTERVAL_S}s"
    sleep "${INTERVAL_S}"
done
