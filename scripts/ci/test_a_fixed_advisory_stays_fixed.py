#!/usr/bin/env python3
"""The advisory-floor guard, exercised against what it must refuse.

The guard's good day and a guard that reads nothing look identical from the
outside, and this repository has shipped the second twice (#294, #361). These
cases make the difference visible.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from a_fixed_advisory_stays_fixed import FLOORS, as_number, below_floors  # noqa: E402

FLOOR = {"RUSTSEC-2026-0185": ("quinn-proto", "0.11.15", "test row")}

CASES = [
    ("the exact floor version passes", [{"name": "quinn-proto", "version": "0.11.15"}], 0),
    ("a later patch passes", [{"name": "quinn-proto", "version": "0.11.17"}], 0),
    ("a later minor passes", [{"name": "quinn-proto", "version": "0.12.0"}], 0),
    ("one patch below the floor is refused", [{"name": "quinn-proto", "version": "0.11.14"}], 1),
    ("a much older version is refused", [{"name": "quinn-proto", "version": "0.9.0"}], 1),
    ("the crate being absent passes", [{"name": "serde", "version": "1.0.0"}], 0),
    (
        "two copies are both judged",
        [
            {"name": "quinn-proto", "version": "0.11.17"},
            {"name": "quinn-proto", "version": "0.11.14"},
        ],
        1,
    ),
]

failures = 0

for name, packages, expected in CASES:
    found = len(below_floors(packages, FLOOR))
    if found != expected:
        print(f"  FAIL: {name}: expected {expected} problem(s), got {found}", file=sys.stderr)
        failures += 1
    else:
        print(f"  ok: {name}")

# Version ordering is the whole guard; a lexicographic comparison would pass
# "0.11.9" as newer than "0.11.15" and let the advisory straight back in.
if as_number("0.11.9") >= as_number("0.11.15"):
    print("  FAIL: versions are being compared as text, not as numbers", file=sys.stderr)
    failures += 1
else:
    print("  ok: 0.11.9 sorts below 0.11.15")

# Every floor must name a crate and a version; a row with an empty crate name
# would silently match nothing.
for advisory, (crate, floor, why) in FLOORS.items():
    if not crate or not as_number(floor) or len(why) < 20:
        print(f"  FAIL: {advisory} has an unusable row: {crate!r} {floor!r}", file=sys.stderr)
        failures += 1

# And the real thing, against the real lockfile.
guard = pathlib.Path(__file__).resolve().parent / "a_fixed_advisory_stays_fixed.py"
run = subprocess.run([sys.executable, str(guard)], capture_output=True, text=True)
if run.returncode != 0:
    print(f"  FAIL: the guard fails on this repository:\n{run.stdout}{run.stderr}", file=sys.stderr)
    failures += 1
else:
    print("  ok: the guard passes on this repository")

if failures:
    print(f"\n{failures} case(s) failed", file=sys.stderr)
    sys.exit(1)
print(f"[advisory-floors] {len(CASES) + 2} case(s) pass")
