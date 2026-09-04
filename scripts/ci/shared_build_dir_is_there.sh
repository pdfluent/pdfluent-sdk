#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Fail at the top of a job, not forty minutes in.
#
# On 27-08-2026 the shared build directory went missing twice. Four jobs died on
# `Permission denied`, which sent the diagnosis after ownership. Two others were
# worse: they stopped existing, and GitLab only cleaned them up as
# `no_updates_running` almost an hour later, with the pipeline waiting on
# something that was already gone (#264).
#
# This checks the directory a job is about to build into, before it builds, and
# says which of the two went wrong. It changes no variable -- deliberately:
# desktop_env.sh appends a slot suffix to CARGO_TARGET_DIR, and a job that
# already has its own must not get a second one.
#
# Usage:  bash scripts/ci/shared_build_dir_is_there.sh [directory]
#         defaults to $CARGO_TARGET_DIR

set -uo pipefail
# shellcheck source=scripts/ci/why_not_writable.sh
. "$(dirname "${BASH_SOURCE[0]}")/why_not_writable.sh"

doel="${1:-${CARGO_TARGET_DIR:-}}"
if [[ -z "${doel}" ]]; then
    echo "SKIPPED (not a pass): no directory given and CARGO_TARGET_DIR is unset." >&2
    exit 0
fi

# On a deadline, both of them.
#
# `mkdir -p` and `[[ -w ]]` are the two calls that blocked forever on 30-08-2026:
# the disk under /mnt/storagebox was failing every read while the mount stayed in
# the table, and eight processes that touched it sat in D-state and never came
# back. A check meant to fail at the top of a job instead joined the outage, and
# the job went quiet until GitLab reaped it as `no_updates_running` an hour
# later. Failing fast is the entire purpose of this file (#264).
DEADLINE="${SHARED_BUILD_DIR_DEADLINE:-20}"

_bounded_probe "${DEADLINE}" mkdir -p "${doel}" >/dev/null 2>&1
mk=$?
if [ "${mk}" -eq 0 ]; then
    _bounded_probe "${DEADLINE}" test -w "${doel}" >/dev/null 2>&1
    mk=$?
fi

if [ "${mk}" -eq 0 ]; then
    echo "[shared-build-dir] OK: ${doel} is there and writable."
    exit 0
fi

if [ "${mk}" -eq 124 ]; then
    echo "[shared-build-dir] FATAL: ${doel} did not answer within ${DEADLINE}s." >&2
else
    echo "[shared-build-dir] FATAL: cannot use ${doel}" >&2
fi
_waarom_niet "${doel}"
exit 1
