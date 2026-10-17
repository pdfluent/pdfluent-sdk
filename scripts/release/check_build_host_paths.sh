#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# check_build_host_paths.sh — run right after a release build, in the same job.
#
# Two things, so a wrong remap fails minutes into the job and the log shows
# why:
#   1. Print the remap block this runner computes (release_rustflags.sh, the
#      helper every release build sources), one pair per line, with whether
#      the source exists and, when it is a symlink or junction, where it
#      really is. A reviewer reads off it which runner paths were covered.
#   2. Run binary_host_paths.py --max 0 over the job's fresh artefacts
#      (.so/.dylib/.dll/.pyd/.node, the CLI executable, or a wheel, whose
#      native member is read). Any build-host path fails the step.
#
# Usage:
#   bash scripts/release/check_build_host_paths.sh FILE [FILE ...]
#   bash scripts/release/check_build_host_paths.sh --remaps-only
# --remaps-only prints the block and checks nothing: for a build that runs
# inside a container (the manylinux wheels), where the block that counts is
# the container's, printed from inside it.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
remaps_only=false
if [ "${1:-}" = "--remaps-only" ]; then
    remaps_only=true
    shift
fi
if ! $remaps_only && [ "$#" -eq 0 ]; then
    echo "usage: check_build_host_paths.sh [--remaps-only] FILE [FILE ...]" >&2
    exit 2
fi

flags="$(
    unset RUSTFLAGS
    # shellcheck disable=SC1091 # resolved beside this file at run time.
    source "$here/release_rustflags.sh"
    printf '%s' "${RUSTFLAGS:-}"
)"
if [ -z "$flags" ]; then
    echo "FAIL: release_rustflags.sh computed no remap block on this runner" >&2
    exit 1
fi
echo "Remap block on this runner (release_rustflags.sh, last match wins):"
# shellcheck disable=SC2086 # the block is space-separated flag/value pairs.
set -- "$@" -- $flags
files=()
while [ "$1" != "--" ]; do files+=("$1"); shift; done
shift
while [ "$#" -gt 0 ]; do
    if [ "$1" = "--remap-path-prefix" ] && [ "$#" -ge 2 ]; then
        src="${2%=*}" dst="${2##*=}"
        if [ -e "$src" ]; then
            real="$(cd "$src" 2>/dev/null && pwd -P || printf '%s' "$src")"
            if [ "$real" != "$src" ]; then
                echo "  $src -> $dst  (exists; real path $real)"
            else
                echo "  $src -> $dst  (exists)"
            fi
        else
            echo "  $src -> $dst  (absent on this runner)"
        fi
        shift 2
    else
        shift
    fi
done
$remaps_only && exit 0

py="$(command -v python3 || command -v python)"
"$py" "$here/binary_host_paths.py" --max 0 "${files[@]}"
