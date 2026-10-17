#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The owner's publish rule of #574, in the one place every publish path can
# call (#575).
#
# WHY IT IS ITS OWN FILE
#
# It was first written into `release_train_guard.sh`, and review found the hole
# that shape has: the hosted crates.io workflow runs `publish_ordered.sh --live`
# directly and never touches the train guard, so the gate sat in a parallel
# orchestrator the live path does not use. A rule that lives in one of two
# publish paths is a rule with a way round it. (codex, #1930)
#
# WHAT IT REFUSES (owner decision 06-10-2026)
#
# The structured extraction ships in 1.1.0 as PREVIEW. A publish is refused
# while the newest master-line round REGRESSED against the one before it --
# that is `structured_parity.py --release`, the ratchet. Parity with the
# reference no longer blocks publication; it is what the "stable" label and
# any parity claim require, and that is `structured_parity.py --stable`,
# which no publish path calls.
#
# There is no "this train does not carry it" flag: `extract_markdown` and
# `extract_blocks` are on master's public surface (#573), so any train off
# master ships them.
set -Eeuo pipefail

HIER="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
WORTEL="$(cd -- "${HIER}/../.." && pwd)"
POORT="${WORTEL}/scripts/ci/loop/structured_parity.py"

if [[ ! -r "${POORT}" ]]; then
    echo "error: ${POORT} is missing, and a publish may not run without the" >&2
    echo "       gate it is required to pass." >&2
    exit 2
fi

echo "[parity] the structured extraction, against the last master round (#574, #575)"
if ! python3 "${POORT}" --release; then
    echo "error: the structured extraction REGRESSED against the last master round; nothing was audited and nothing published." >&2
    exit 1
fi
echo "the structured extraction is not below the last master round (preview; parity is required for stable only)"
