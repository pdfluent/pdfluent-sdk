#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Which of the three is it: the mount is gone, the disk is dead, or we lack
# permission.
#
# Twice on 27-08-2026 `/mnt/storagebox/cargo-slots` was simply not there, and the
# error that reached the log was `Permission denied (os error 13)` -- which
# points at ownership while the cause is a mount. Four jobs died on it, and the
# diagnosis started from the wrong end both times (#264).
#
# WHY THERE ARE THREE VERDICTS AND NOT TWO
#
# Measured on the build machine 30-08-2026, with no job running:
#
#   /proc/self/mountinfo: 318 82 8:65 / /mnt/storagebox rw,noatime - ext4 /dev/sde1
#   dmesg:                hv_storvsc ... tag#34 cmd 0x28 status: scsi 0x2 srb 0x4
#                         host 0xc0000001   (1160 lines, 47903 suppressed)
#   ps:                   8 x `ls /mnt/storagebox` in D-state, wchan
#                         folio_wait_bit_common / wait_on_buffer, never returning
#
# The mount was PRESENT and the disk was DEAD. Neither of the two verdicts this
# file used to give is true of that state, and it is the state that produces
# #264's worse half: jobs that stop reporting and get reaped as
# `no_updates_running` an hour later. A directory that hangs is not a directory
# that is missing, and it is not a permissions problem either.
#
# WHY NOT "IS THE MOUNT POINT EMPTY"
#
# The previous attempt at this decided the mount was gone when the storage root
# existed but held nothing. That reads the wrong evidence twice over. An
# unmounted `/mnt/storagebox` is a root-owned directory on the root filesystem
# that anything may have written into since, and a live mount can legitimately
# be empty. Worse, `ls -A` on the suspect path is exactly the call that hangs
# when the disk is dead, so the diagnosis joined the outage it was diagnosing.
#
# /proc/self/mountinfo answers the mount question outright, and reading it
# touches no filesystem at all.
#
# Sourced by desktop_env.sh and by shared_build_dir_is_there.sh, so both say the
# same thing. Sourced, never executed: it defines one function.

# shellcheck source=scripts/ci/bounded_probe.sh
. "$(dirname "${BASH_SOURCE[0]}")/bounded_probe.sh"

# Overridable so the test can build all three situations without root and
# without a real outage. Defaults are the real thing.
: "${WNW_MOUNTINFO:=/proc/self/mountinfo}"
: "${WNW_OPSLAG_PREFIX:=/mnt}"
: "${WNW_DEADLINE:=10}"

# Every mount point on this machine, one per line. Field 5 of mountinfo.
# Pure text: this cannot hang, whatever state the disks are in.
_wnw_mountpunten() {
    [ -r "${WNW_MOUNTINFO}" ] || return 1
    awk '{print $5}' "${WNW_MOUNTINFO}" 2>/dev/null
}

# Is this exact path a mount point?
_wnw_is_gekoppeld() {
    local _pad="$1" _mp
    while IFS= read -r _mp; do
        [ "${_mp}" = "${_pad}" ] && return 0
    done <<EOF
$(_wnw_mountpunten)
EOF
    return 1
}

# The longest mount point that contains this path. "/" if nothing else does.
_wnw_dragende_mount() {
    local _pad="$1" _mp _beste="/"
    while IFS= read -r _mp; do
        [ -n "${_mp}" ] || continue
        if [ "${_pad}" = "${_mp}" ] || case "${_pad}" in "${_mp%/}"/*) true ;; *) false ;; esac; then
            [ "${#_mp}" -gt "${#_beste}" ] && _beste="${_mp}"
        fi
    done <<EOF
$(_wnw_mountpunten)
EOF
    printf '%s\n' "${_beste}"
}

# The storage root a path was meant to live under: /mnt/<name> for /mnt/<name>/…
# Empty when the path is not under the storage prefix at all.
_wnw_opslagwortel() {
    local _pad="$1" _rest
    case "${_pad}" in
        "${WNW_OPSLAG_PREFIX%/}"/*) _rest="${_pad#"${WNW_OPSLAG_PREFIX%/}"/}" ;;
        *) return 1 ;;
    esac
    printf '%s/%s\n' "${WNW_OPSLAG_PREFIX%/}" "${_rest%%/*}"
}

_waarom_niet() {
    local _doel="$1" _wortel _drager _probe

    echo "[desktop_env]   running as: $(id -un) ($(id -u)) groups: $(id -Gn 2>/dev/null)" >&2

    # 1. Is it answering at all? Ask the mount that carries the path, with the
    #    parent holding the clock. This comes FIRST because every other question
    #    below touches the filesystem, and on a dead disk that never returns.
    _drager="$(_wnw_dragende_mount "${_doel}")"
    # The exit code is captured on its own line on purpose: inside `if ! cmd`
    # the value of $? is the negation's, always 0, and the branch that tells a
    # dead disk from an empty directory would never be taken. That mistake is
    # already written up in corpus_preflight.sh; it is easy to make twice.
    _bounded_probe "${WNW_DEADLINE}" ls -A "${_drager}" >/dev/null 2>&1
    _probe=$?
    if [ "${_probe}" -eq 124 ]; then
        echo "[desktop_env]   VERDICT: ${_drager} is mounted but does NOT ANSWER." >&2
        echo "[desktop_env]   Reading it did not return within ${WNW_DEADLINE}s. The device is" >&2
        echo "[desktop_env]   failing I/O while the mount stays in the table, so every job" >&2
        echo "[desktop_env]   that touches it blocks in D-state and cannot be killed." >&2
        echo "[desktop_env]   This is neither a missing mount nor permissions." >&2
        echo "[desktop_env]   Check: dmesg | grep hv_storvsc  — recovery: re-attach the disk" >&2
        echo "[desktop_env]   to the VM; a reboot of the VM is what clears the D-state. (#264)" >&2
        return 0
    fi

    # 2. It answers. Was the storage it should be on ever mounted?
    if _wortel="$(_wnw_opslagwortel "${_doel}")"; then
        if [ ! -r "${WNW_MOUNTINFO}" ]; then
            # Say so rather than guess. Without the mount table every path looks
            # unmounted, and answering "not mounted" from no evidence is the
            # same wrong turn as answering "permissions" from no evidence -- it
            # just points the next person down a different dead end.
            echo "SKIPPED (not a pass): ${WNW_MOUNTINFO} is unreadable, so whether" >&2
            echo "  ${_wortel} is mounted was not determined." >&2
        elif _wnw_is_gekoppeld "${_wortel}"; then
            echo "[desktop_env]   ${_wortel} is a mount point in ${WNW_MOUNTINFO}." >&2
        else
            echo "[desktop_env]   VERDICT: ${_wortel} is NOT MOUNTED — no entry for it in" >&2
            echo "[desktop_env]   ${WNW_MOUNTINFO}. What you can see at that path is the bare" >&2
            echo "[desktop_env]   mount point on the root filesystem, root-owned, which is why" >&2
            echo "[desktop_env]   creating anything under it says 'Permission denied'." >&2
            echo "[desktop_env]   This is not a permissions problem. Remount it; the directory" >&2
            echo "[desktop_env]   and its contents come back with it. (#264)" >&2
            return 0
        fi
    fi

    # 3. Mounted, answering, still unusable: now it really is ownership. Only
    #    here is it safe to stat the path, and it is still done on a deadline --
    #    a disk can die between the probe above and this line.
    local _p="${_doel}"
    while [ ! -e "${_p}" ] && [ "${_p}" != "/" ]; do _p="$(dirname "${_p}")"; done
    echo "[desktop_env]   deepest existing ancestor: ${_p}" >&2
    _bounded_probe "${WNW_DEADLINE}" ls -ld "${_p}" | sed 's/^/[desktop_env]   /' >&2
    echo "[desktop_env]   VERDICT: ${_p} is not writable by $(id -un)." >&2
}
