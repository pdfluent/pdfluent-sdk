#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Touch a filesystem that may be dead, without joining it.
#
# WHY `timeout` IS NOT ENOUGH
#
# A process blocked on uninterruptible I/O (D-state) cannot be killed, not even
# with SIGKILL. `timeout 10 ls /mnt/storagebox` then hangs for the full length of
# the outage itself, because `timeout` waits for a child that will not die. Every
# caller behind it hangs too.
#
# So the child does the touching and the PARENT keeps the clock. The child may
# stay stuck until the disk returns or the machine reboots; the caller gets its
# answer within the deadline either way.
#
# Measured on the build machine 30-08-2026: `/mnt/storagebox` was mounted
# (ext4 on /dev/sde1, present in /proc/self/mountinfo) while every read failed --
# `hv_storvsc ... cmd 0x28 status: scsi 0x2 srb 0x4 host 0xc0000001`, 1160 lines
# and 47903 suppressed callbacks. Eight `ls` processes sat in D-state
# (`folio_wait_bit_common`, `wait_on_buffer`) and none of them ever returned.
# That is the state #264 keeps landing in, and nothing in the pipeline could
# tell it apart from a slow build.
#
# corpus_preflight.sh proved this pattern on the 21-08-2026 outage. This file is
# that idea on its own so the mount verdict and the disk guard can use it too.
# Sourced, never executed: it defines one function.
#
# Usage:
#   . scripts/ci/bounded_probe.sh
#   if _bounded_probe 10 ls -A /mnt/storagebox; then ...; fi
#
# Returns the command's own exit code, or 124 when the deadline passed. The
# command's stdout is forwarded; its stderr is dropped, because a probe is asked
# whether the answer arrives, not what it says.

_bounded_probe() {
    local _secs="$1"; shift
    local _uit _kind _tienden _i=0 _code
    _uit="$(mktemp 2>/dev/null)" || return 125

    "$@" >"${_uit}" 2>/dev/null &
    _kind=$!

    # Tenths, so a one-second deadline is still ten chances to notice rather
    # than one. `sleep 0.1` is coreutils everywhere we run.
    _tienden=$(( _secs * 10 ))
    while [ "${_i}" -lt "${_tienden}" ]; do
        kill -0 "${_kind}" 2>/dev/null || break
        sleep 0.1
        _i=$(( _i + 1 ))
    done

    if kill -0 "${_kind}" 2>/dev/null; then
        # Best effort. It does nothing to a D-state child, and that is exactly
        # why the parent is the one holding the clock.
        kill -9 "${_kind}" 2>/dev/null || true
        rm -f "${_uit}"
        return 124
    fi

    wait "${_kind}" 2>/dev/null
    _code=$?
    cat "${_uit}"
    rm -f "${_uit}"
    return "${_code}"
}
