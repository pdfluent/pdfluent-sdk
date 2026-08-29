#!/usr/bin/env python3
"""Two files claim to list our forks. They must not disagree.

docs/release/canonical_licenses.toml records forks for licence obligations;
docs/UPSTREAM_FORKS.toml records them so we notice when upstream moves ahead.
Both are hand-maintained, both are easy to forget, and a fork missing from
either one is invisible in a different way -- unattributed, or silently
falling behind.

Neither list is authoritative over the other. What matters is that adding a
fork to one and not the other fails here rather than being discovered later.
"""
from __future__ import annotations

import pathlib
import sys
import tomllib

WORTEL = pathlib.Path(__file__).resolve().parents[2]
LICENTIES = WORTEL / "docs/release/canonical_licenses.toml"
UPSTREAM = WORTEL / "docs/UPSTREAM_FORKS.toml"

# Recorded as a fork for licence purposes but deliberately not tracked against
# an upstream release, with the reason. Keep this short and argued.
GEEN_UPSTREAM_SPOOR: dict[str, str] = {}


def uit_licenties() -> set[str]:
    doc = tomllib.load(LICENTIES.open("rb"))
    gevonden: set[str] = set()

    def loop(x) -> None:
        if isinstance(x, dict):
            erf = str(x.get("heritage", ""))
            if "ork of" in erf:
                naam = x.get("workspace_dir") or x.get("name")
                if naam:
                    gevonden.add(str(naam))
            for v in x.values():
                loop(v)
        elif isinstance(x, list):
            for v in x:
                loop(v)

    loop(doc)
    return gevonden


def main() -> int:
    for pad in (LICENTIES, UPSTREAM):
        if not pad.exists():
            print(f"FAIL: {pad.relative_to(WORTEL)} is missing; the two fork lists "
                  "cannot be compared.", file=sys.stderr)
            return 1

    licentie = uit_licenties()
    gevolgd = {f["onze_crate"] for f in tomllib.load(UPSTREAM.open("rb"))["fork"]}

    ontbreekt = licentie - gevolgd - set(GEEN_UPSTREAM_SPOOR)
    if ontbreekt:
        print(f"FAIL: {len(ontbreekt)} crate(s) are recorded as forks for licensing but "
              "are not tracked against upstream, so nothing will notice when they fall "
              f"behind: {', '.join(sorted(ontbreekt))}", file=sys.stderr)
        return 1

    print(f"[fork-lists] OK: {len(licentie)} licensed fork(s), {len(gevolgd)} tracked; "
          "none recorded in one list and missing from the other.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
