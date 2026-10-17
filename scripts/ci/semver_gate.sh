#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# semver_gate.sh — refuse a 1.x tree that breaks semver against v1.0.0 (#574).
#
# WHY
#
# `EngineError` grew a variant after 1.0.0 on an exhaustive enum, which is a
# major-version change hiding in a minor: every downstream exhaustive match
# breaks. The fix (OptionsError, carried in EngineError::Io) restored the
# letter of semver; this gate keeps it. It runs `cargo semver-checks
# check-release --baseline-rev v1.0.0` for every workspace crate published at
# 1.0.0 — the publish set from scripts/release/topological_publish_order.py,
# which is the same order scripts/release/publish_ordered.sh ships in,
# narrowed to the crates whose Cargo.toml at tag v1.0.0 was publishable at
# version 1.0.0 (scripts/ci/_semver_gate_select.sh). Selection is by the TAG,
# never by the current version: selecting on "is 1.0.0 today" checks nothing
# once the workspace moves to 1.1.0.
#
# A MISSING TOOL IS NOT A PASS. Without cargo-semver-checks installed this
# script claims nothing: it prints SKIPPED on stderr and exits 2, never 0.
# (The shape of every SKIPPED-reported-as-a-failure in this repository, with
# the sign held the right way round.)
#
# After the check-release loop, the gate also refuses a public enum that is
# NEW since v1.0.0 without #[non_exhaustive]: check-release compares
# against v1.0.0 and cannot see it, so it would ship in 1.1.0 and break
# on its first new variant (scripts/ci/check_new_public_enums.py).
#
# Default features are not the whole public API. Every crate with a line
# in scripts/ci/semver_features.txt gets a second check-release pass with
# defaults plus those features, and the enum half builds its rustdoc JSON
# with the same features; a feature the baseline never had is enabled on
# the current side only. Never --all-features: the list is explicit.
#
# Wired into the release pipeline (.github/workflows/publish-crates.yml),
# before any publish step. Not in per-PR CI.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/ci/_semver_gate_select.sh
. scripts/ci/_semver_gate_select.sh

if ! command -v cargo-semver-checks >/dev/null 2>&1; then
    echo "SKIPPED (not a pass): cargo-semver-checks not installed" >&2
    exit 2
fi

if ! command git rev-parse --verify --quiet v1.0.0 >/dev/null; then
    echo "semver_gate: baseline tag v1.0.0 is not in this checkout" >&2
    exit 2
fi

ORDER_FILE="$(mktemp)"
MAP_FILE="$(mktemp)"
SELECTED="$(mktemp)"
trap 'rm -f "$ORDER_FILE" "$MAP_FILE" "$SELECTED"' EXIT
if ! python3 scripts/release/topological_publish_order.py >"$ORDER_FILE"; then
    echo "semver_gate: topological_publish_order.py failed; claiming nothing" >&2
    exit 2
fi
if ! cargo metadata --no-deps --format-version 1 | python3 -c '
import json, os, sys
root = os.getcwd()
for p in json.load(sys.stdin)["packages"]:
    name = p["name"]
    rel = os.path.relpath(p["manifest_path"], root)
    print(name + "=" + rel)
' >"$MAP_FILE"; then
    echo "semver_gate: cargo metadata failed; claiming nothing" >&2
    exit 2
fi
export SEMVER_GATE_MANIFEST_MAP="$MAP_FILE"
if ! semver_gate_select "$ORDER_FILE" >"$SELECTED"; then
    echo "semver_gate: crate selection failed; claiming nothing" >&2
    exit 2
fi

# The feature pass reads scripts/ci/semver_features.txt. A missing or
# malformed file is not "no feature pass": every non-blank non-comment
# line must be `<known crate>: <features>`, exactly once per crate, or the
# gate claims nothing. (The enum half re-validates what it reads the same
# way; the two never disagree silently.)
FEATURES_FILE="scripts/ci/semver_features.txt"
export SEMVER_GATE_FEATURES_FILE="$FEATURES_FILE"
if [ ! -f "$FEATURES_FILE" ]; then
    echo "semver_gate: $FEATURES_FILE is missing; claiming nothing" >&2
    exit 2
fi
if ! python3 - "$FEATURES_FILE" "$MAP_FILE" <<'EOF'
import re
import sys
known = set()
with open(sys.argv[2], encoding="utf-8") as fh:
    for line in fh:
        if "=" in line:
            known.add(line.split("=", 1)[0].strip())
seen = set()
with open(sys.argv[1], encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = re.fullmatch(r"([A-Za-z0-9_-]+)\s*:\s*(.+)", line)
        if not m:
            print(f"semver_gate: malformed {sys.argv[1]}:{lineno}; claiming nothing",
                  file=sys.stderr)
            sys.exit(1)
        crate, feats = m.group(1), m.group(2)
        if crate not in known:
            print(f"semver_gate: {sys.argv[1]}:{lineno} names unknown crate "
                  f"'{crate}'; claiming nothing", file=sys.stderr)
            sys.exit(1)
        if crate in seen:
            print(f"semver_gate: {sys.argv[1]}:{lineno} repeats crate "
                  f"'{crate}'; claiming nothing", file=sys.stderr)
            sys.exit(1)
        seen.add(crate)
        for feat in feats.split():
            if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.\-/:+]*", feat):
                print(f"semver_gate: {sys.argv[1]}:{lineno} has a bad feature "
                      f"name '{feat}'; claiming nothing", file=sys.stderr)
                sys.exit(1)
EOF
then
    exit 2
fi

failed=()
checked=0
while read -r name version; do
    [ -n "$name" ] || continue
    checked=$((checked + 1))
    echo "[semver] $name ($version): check-release against v1.0.0"
    if cargo semver-checks check-release --baseline-rev v1.0.0 -p "$name"; then
        echo "[semver] $name: OK"
    else
        failed+=("$name")
    fi
    feats="$(semver_gate_features_for "$name")"
    if [ -n "$feats" ]; then
        echo "[semver] $name ($version): check-release against v1.0.0, features: $feats"
        args=()
        for feat in $feats; do
            args+=(--features "$feat")
        done
        # --default-features pins the pass to defaults plus the listed
        # features: without it the all-features heuristic would also enable
        # what the list deliberately excludes (system libraries, downloads).
        # A feature the baseline never had is enabled on the current side
        # only (cargo-semver-checks warns and carries on).
        if cargo semver-checks check-release --baseline-rev v1.0.0 -p "$name" \
                --default-features "${args[@]}"; then
            echo "[semver] $name (features): OK"
        else
            failed+=("$name (features)")
        fi
    fi
done <"$SELECTED"

if [ "$checked" -eq 0 ]; then
    echo "semver_gate: no crates published at 1.0.0 in the publish order; claiming nothing" >&2
    exit 2
fi

if [ "${#failed[@]}" -ne 0 ]; then
    echo "[semver] FAIL: ${#failed[@]} crate(s) break semver against v1.0.0:" >&2
    for name in "${failed[@]}"; do
        echo "  $name" >&2
    done
fi

# New public enums, over the same selection: cargo-semver-checks cannot
# see an exhaustive enum that is new since v1.0.0, so a second check
# refuses it before it ships (scripts/ci/check_new_public_enums.py).
# Runs even when the loop above failed, so one gate run reports both.
# The enum half reads the same feature list: a crate with a line gets its
# rustdoc JSON built with defaults plus those features, on both sides.
enums_status=0
python3 scripts/ci/check_new_public_enums.py --crates-file "$SELECTED" \
    --features-file "$FEATURES_FILE" || enums_status=$?

if [ "${#failed[@]}" -ne 0 ] || [ "$enums_status" -eq 1 ]; then
    exit 1
fi

if [ "$enums_status" -eq 2 ]; then
    exit 2
fi

echo "[semver] OK: $checked crate(s) keep semver against v1.0.0"
