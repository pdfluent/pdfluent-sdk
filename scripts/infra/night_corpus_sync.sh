#!/usr/bin/env bash
# night_corpus_sync.sh — pull the remaining corpus data from the VPS, but only
# during the night window, so daytime internet stays usable.
#
# Cron on the WSL desktop runner:
#   0 23 * * * root /usr/local/sbin/night_corpus_sync.sh >> /var/log/night-sync.log 2>&1
#
# The window is enforced by this script, not by a second cron entry that kills
# it: computing the deadline here means an overrun cannot outlive the window
# even if the killer job fails to fire.
#
# WHY RSYNC AND NOT A TAR STREAM
#
# A single tar stream per directory is roughly an order of magnitude faster for
# hundreds of thousands of small files, and that is what the initial migration
# used. It is also not resumable: an interrupted stream leaves a half-extracted
# tree with no way to continue. This job spans several nights by design (~720 GB
# at a measured ~7.9 MB/s), so it will be interrupted every single night, and
# resumability outranks throughput. rsync skips what it already has, so each
# night continues where the last one stopped.
#
# NEVER DELETES AT THE DESTINATION. --delete would prune a partial mirror down
# to whatever the current source listing happens to be; on an interrupted
# multi-night copy that destroys work rather than syncing it.
set -uo pipefail

VPS="${VPS_HOST:-root@46.225.223.175}"
SRC_ROOT="/mnt/storagebox"
DST_ROOT="/mnt/storagebox"
STATE_DIR="/var/lib/night-corpus-sync"
LOCK="${STATE_DIR}/lock"
STOP_HOUR="${STOP_HOUR:-7}"
MIN_FREE_GB="${MIN_FREE_GB:-50}"

# Highest value first, so an unfinished run still leaves the most important
# data present:
#   corpus     — the measuring stick; curated selections our baselines rest on
#   safedocs   — public DARPA set, re-fetchable in principle, but our baselines
#                were taken against THIS snapshot, so a later re-download is a
#                different set and silently invalidates them
#   pdfluent   — a May tar backup of the corpus; genuinely duplicate, so last
SOURCES=(corpus safedocs pdfluent)

log() { echo "$(date -Iseconds) $*"; }

# A job that cannot set up its own state must say so and stop. Failing soft
# here would mean the transfer silently never runs while the log reads normal --
# and nobody would notice for weeks, by which time the VPS may be gone.
if ! mkdir -p "${STATE_DIR}" 2>/dev/null; then
    log "ABORT: cannot create ${STATE_DIR} (need root?)"
    exit 1
fi
if ! command -v flock >/dev/null 2>&1; then
    log "ABORT: flock not available — refusing to run without an overlap guard"
    exit 1
fi

# One run at a time. Cron firing while last night's run somehow survives would
# otherwise have two rsyncs competing for the same files and the same uplink.
exec 9>"${LOCK}" || { log "ABORT: cannot open lock file ${LOCK}"; exit 1; }
if ! flock -n 9; then
    log "SKIP: another run holds the lock (still going from a previous window?)"
    exit 0
fi

# Seconds until the stop hour. Runs started inside the window get the remainder;
# a manual run in the afternoon gets nothing and says so.
now_h=$(date +%-H)
now_m=$(date +%-M)
if (( now_h >= STOP_HOUR && now_h < 23 )); then
    log "SKIP: outside the night window (now ${now_h}:${now_m}, window 23:00-0${STOP_HOUR}:00)"
    log "  override with STOP_HOUR=<hour> if you really want to run in daytime"
    exit 0
fi
if (( now_h >= 23 )); then
    minutes_left=$(( (24 - now_h + STOP_HOUR) * 60 - now_m ))
else
    minutes_left=$(( (STOP_HOUR - now_h) * 60 - now_m ))
fi
budget=$(( minutes_left * 60 ))
log "night window: ${minutes_left} minutes left until 0${STOP_HOUR}:00"

if ! timeout 30 ssh -o ConnectTimeout=20 -o BatchMode=yes "${VPS}" true 2>/dev/null; then
    log "ABORT: cannot reach ${VPS}"
    exit 1
fi

deadline=$(( $(date +%s) + budget ))
transferred_any=0

for src in "${SOURCES[@]}"; do
    remaining=$(( deadline - $(date +%s) ))
    if (( remaining < 120 )); then
        log "window closed — stopping before ${src}"
        break
    fi

    if [[ -f "${STATE_DIR}/done.${src}" ]]; then
        log "${src}: already complete (marker present), skipping"
        continue
    fi

    free_gb=$(df -BG --output=avail "${DST_ROOT}" | tail -1 | tr -dc '0-9')
    if (( free_gb < MIN_FREE_GB )); then
        log "ABORT: only ${free_gb} GB free on ${DST_ROOT} (need >${MIN_FREE_GB})"
        exit 1
    fi

    log "${src}: starting, ${remaining}s of window left, ${free_gb} GB free"
    # -a preserves times so the next night can tell what it already has.
    # --partial keeps half-copied large files (that May tar is 231 GB; without
    # it, every night would restart it from zero and it would never finish).
    timeout "${remaining}" rsync -a --partial --info=progress2 --no-inc-recursive \
        -e "ssh -o ConnectTimeout=20 -o BatchMode=yes" \
        "${VPS}:${SRC_ROOT}/${src}/" "${DST_ROOT}/${src}/"
    rc=$?
    transferred_any=1

    case "$rc" in
        0)
            log "${src}: COMPLETE"
            date -Iseconds > "${STATE_DIR}/done.${src}"
            ;;
        124)
            log "${src}: window closed mid-transfer — will continue next night"
            break
            ;;
        *)
            # 23/24 are partial-transfer codes (vanished or unreadable files),
            # normal on a live tree and not a reason to stop the whole run.
            log "${src}: rsync exit ${rc} — not marking complete, will retry next night"
            ;;
    esac
done

if (( transferred_any == 0 )); then
    log "nothing left to do — all sources carry a completion marker"
fi

log "--- state ---"
for src in "${SOURCES[@]}"; do
    if [[ -f "${STATE_DIR}/done.${src}" ]]; then
        log "  ${src}: complete since $(cat "${STATE_DIR}/done.${src}")"
    else
        log "  ${src}: $(du -sh "${DST_ROOT}/${src}" 2>/dev/null | cut -f1) local, incomplete"
    fi
done
log "destination free: $(df -h "${DST_ROOT}" | tail -1 | awk '{print $4}')"
