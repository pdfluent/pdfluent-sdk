#!/usr/bin/env python3
"""Provisioning and reaping must take the same lock, or there is no lock.

Neither the Hetzner nor the GitHub API offers a compare-and-swap, so every
check either side of a delete is a point in time (#280). Exactly one machine
provisions and reaps -- this desktop -- so a plain file lock closes that
window. It only closes it if both sides name the same file, and two default
strings in two languages drift the moment somebody edits one.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
KANTEN = {
    "scripts/ci/reuse_an_idle_instance.sh":
        r'PDFLUENT_INSTANCE_LOCK:-([^}"\s]+)',
    "scripts/ci/sweep_idle_instances.py":
        r'"PDFLUENT_INSTANCE_LOCK",\s*"([^"]+)"',
}


def main() -> int:
    gevonden: dict[str, str] = {}
    for pad, patroon in KANTEN.items():
        bestand = WORTEL / pad
        if not bestand.exists():
            print(f"FAIL: {pad} is missing, so the lock cannot be checked", file=sys.stderr)
            return 1
        m = re.search(patroon, bestand.read_text())
        if m is None:
            print(f"FAIL: {pad} no longer takes a lock. Provisioning and reaping race "
                  "the moment one side stops locking.", file=sys.stderr)
            return 1
        gevonden[pad] = m.group(1)

    if len(set(gevonden.values())) != 1:
        print("FAIL: the two sides lock different files, which is the same as not "
              "locking at all:", file=sys.stderr)
        for pad, slot in gevonden.items():
            print(f"  {pad} -> {slot}", file=sys.stderr)
        return 1

    slot = next(iter(gevonden.values()))
    print(f"[one-lock] OK: provisioning and reaping both take {slot}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
