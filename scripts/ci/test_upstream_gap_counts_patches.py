#!/usr/bin/env python3
"""A fork behind on patch releases is behind (#262).

The gap was measured as major*100 + minor, so twenty patch releases behind
measured zero and read as current. The same file's `volledig()` says in as many
words that patch releases are where fixes land -- the CCITT buffer fix arrived
in one.

Pure arithmetic, so this runs anywhere and needs no network.
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys

HIER = pathlib.Path(__file__).resolve().parent


def laad():
    spec = importlib.util.spec_from_file_location(
        "upstream", HIER / "upstream_has_not_moved_on.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main() -> int:
    m = laad()
    stuk = []

    if m.volledig("0.3.7") <= m.volledig("0.3.0"):
        stuk.append("0.3.7 does not compare as ahead of 0.3.0")
    if m.volledig("0.3.0") > m.volledig("0.3.0"):
        stuk.append("a version compares as ahead of itself")

    # The threshold has to be a small number, not a licence to drift: a fork
    # ten patches behind should not read as current.
    if m.PATCHES_TOEGESTAAN >= 10:
        stuk.append(f"PATCHES_TOEGESTAAN is {m.PATCHES_TOEGESTAAN}, which is wide "
                    "enough that a fork can drift a long way and stay green")
    if m.PATCHES_TOEGESTAAN < 1:
        stuk.append("PATCHES_TOEGESTAAN below 1 fails on the first upstream patch, "
                    "which makes the check noise rather than signal")

    # And the ordering must be total: major beats minor beats patch.
    volgorde = ["0.2.9", "0.3.0", "0.3.1", "1.0.0"]
    gesorteerd = sorted(volgorde, key=m.volledig)
    if gesorteerd != volgorde:
        stuk.append(f"version ordering is wrong: {gesorteerd}")

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}", file=sys.stderr)
        return 1
    print("[upstream-patches] OK: patch releases count, and the ordering is total.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
