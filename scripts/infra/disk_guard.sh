#!/usr/bin/env bash
# disk_guard.sh — fail-fast if any monitored mount is below threshold.
#
# Used as a pre-flight check in CI pipelines and a standalone alarm.
# Exits 0 if all mounts are above threshold, 1 if any is below.
#
# Usage:
#   disk_guard.sh                 # use default thresholds, current host
#   disk_guard.sh --threshold-root 15  # override root threshold (GB)
#   disk_guard.sh --json          # emit JSON instead of text
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$HERE/_lib.sh"

THRESHOLD_ROOT_GB=15
THRESHOLD_STORAGEBOX_GB=500
THRESHOLD_MAC_DATA_GB=50
JSON=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --threshold-root) shift; THRESHOLD_ROOT_GB="$1" ;;
    --threshold-storagebox) shift; THRESHOLD_STORAGEBOX_GB="$1" ;;
    --threshold-mac-data) shift; THRESHOLD_MAC_DATA_GB="$1" ;;
    --json) JSON=1 ;;
    -h|--help)
      echo "Usage: $0 [--threshold-root GB] [--threshold-storagebox GB] [--threshold-mac-data GB] [--json]"
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

# Get available GB for a mount. Works on macOS and Linux.
mount_avail_gb() {
  local mount="$1"
  if [[ ! -d "$mount" ]]; then echo ""; return; fi
  df -k "$mount" 2>/dev/null | awk 'NR==2 { printf "%d", $4/1024/1024 }'
}

declare -a FAILURES=()
declare -a RESULTS=()

check_mount() {
  local label="$1" path="$2" threshold="$3"
  local avail
  avail=$(mount_avail_gb "$path")
  if [[ -z "$avail" ]]; then
    RESULTS+=("$label|$path|N/A|skipped")
    return
  fi
  local status="ok"
  if (( avail < threshold )); then
    status="LOW"
    FAILURES+=("$label avail=${avail}G < threshold=${threshold}G ($path)")
  fi
  RESULTS+=("$label|$path|${avail}G|$status (threshold=${threshold}G)")
}

case "$(uname -s)" in
  Linux)
    check_mount root          "/"               "$THRESHOLD_ROOT_GB"
    check_mount storagebox    "/mnt/storagebox" "$THRESHOLD_STORAGEBOX_GB"
    ;;
  Darwin)
    check_mount mac_data      "/System/Volumes/Data" "$THRESHOLD_MAC_DATA_GB"
    ;;
esac

if (( JSON )); then
  printf '{"host":"%s","results":[' "$(hostname -s)"
  first=1
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r lbl pth av st <<<"$r"
    (( first )) || printf ','
    first=0
    printf '{"mount":"%s","path":"%s","avail":"%s","status":"%s"}' "$lbl" "$pth" "$av" "$st"
  done
  printf '],"failures":%d}\n' "${#FAILURES[@]}"
else
  echo "=== disk_guard $(hostname -s) ==="
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r lbl pth av st <<<"$r"
    printf "  %-12s  %-30s  avail=%-6s  %s\n" "$lbl" "$pth" "$av" "$st"
  done
  if (( ${#FAILURES[@]} > 0 )); then
    echo
    echo "FAIL:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
  fi
fi

exit $(( ${#FAILURES[@]} > 0 ? 1 : 0 ))
