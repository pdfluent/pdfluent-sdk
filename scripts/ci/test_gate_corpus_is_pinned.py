#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Break the gate corpus manifest on purpose and check the guard notices.

The guard in gate_corpus_is_pinned.py is what stands between "the gates measure
a fixed, licensed, checksummed set" and "the gates measure whatever is lying
around". A guard nobody has tried to defeat is not a guard -- that is how a
green suite survived a `/ToUnicode` doubling, and how four workflows sat in a
queue for months looking like gates (#276).

Each case mutates a copy of the real manifest in a temporary directory and
requires the guard to come back non-zero. The last case is the control: the
unmutated copy must still pass, otherwise these are all passing for the wrong
reason.

# FLOOR: cases >= 11 -- two-way. Fewer means a mutation was dropped and the
# suite got easier while still reporting green; more without editing this
# number means somebody added a case and never checked it into the count.
"""

from __future__ import annotations

import copy
import json
import pathlib
import subprocess
import sys
import tempfile

VLOER = 11
GUARD = pathlib.Path(__file__).with_name("gate_corpus_is_pinned.py").resolve()
ECHT = pathlib.Path("corpus/GATE_CORPUS_MANIFEST.json")


def zonder_een_entry(d):
    d["entries"].pop()
    d["total"] = len(d["entries"])
    return "an entry dropped from the manifest"


def met_een_entry_extra(d):
    extra = copy.deepcopy(d["entries"][0])
    extra["name"] = extra["name"] + ".copy"
    d["entries"].append(extra)
    d["total"] = len(d["entries"])
    return "an entry added without moving the floor"


def zonder_checksum(d):
    d["entries"][3]["sha256"] = ""
    return "an entry with no checksum"


def losse_checksum(d):
    d["entries"][3]["sha256"] = "not-a-hash"
    return "an entry with a checksum that is not one"


def bron_op_een_tak(d):
    sid = next(iter(d["sources"]))
    d["sources"][sid]["commit"] = "master"
    d["sources"][sid]["raw_base"] = (
        "https://raw.githubusercontent.com/mozilla/pdf.js/master/")
    return "a source pinned to a branch instead of a commit"


def url_wijst_elders(d):
    sid = next(iter(d["sources"]))
    d["sources"][sid]["raw_base"] = (
        "https://raw.githubusercontent.com/mozilla/pdf.js/"
        "0000000000000000000000000000000000000000/")
    return "a source fetching from a commit it does not declare"


def zonder_licentie(d):
    d["sources"][next(iter(d["sources"]))]["licence"] = ""
    return "a source with no licence (#215)"


def zonder_herkomst(d):
    d["sources"][next(iter(d["sources"]))]["provenance"] = "somewhere"
    return "a source with no usable provenance line (#215)"


def onbekende_bron(d):
    d["entries"][7]["source"] = "nergens"
    return "an entry naming a source that is not declared"


def dubbele_naam(d):
    d["entries"][5]["name"] = d["entries"][4]["name"]
    return "the same name twice"


def totaal_liegt(d):
    d["total"] = 1
    return "a `total` that disagrees with the entry list"


MUTATIES = [
    zonder_een_entry,
    met_een_entry_extra,
    zonder_checksum,
    losse_checksum,
    bron_op_een_tak,
    url_wijst_elders,
    zonder_licentie,
    zonder_herkomst,
    onbekende_bron,
    dubbele_naam,
    totaal_liegt,
]


def draai(pad: pathlib.Path, floor: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(GUARD), "--manifest", str(pad), "--floor", str(floor)],
        capture_output=True, text=True, check=False, timeout=120,
    )


def main() -> int:
    if not ECHT.is_file():
        print(f"SKIPPED (not a pass): {ECHT} is not here, so there is nothing "
              "to mutate. Run from the repository root.", file=sys.stderr)
        return 1

    basis = json.loads(ECHT.read_text())
    floor = len(basis["entries"])
    mislukt = []

    with tempfile.TemporaryDirectory() as tmp:
        pad = pathlib.Path(tmp) / "manifest.json"

        # Control first: if the untouched copy does not pass, every failure
        # below would be meaningless.
        pad.write_text(json.dumps(basis))
        r = draai(pad, floor)
        if r.returncode != 0:
            print("FAIL: the unmutated manifest does not pass the guard, so "
                  "nothing this test reports means anything.", file=sys.stderr)
            print(r.stdout + r.stderr, file=sys.stderr)
            return 1

        for mutatie in MUTATIES:
            d = copy.deepcopy(basis)
            wat = mutatie(d)
            pad.write_text(json.dumps(d))
            r = draai(pad, floor)
            if r.returncode == 0:
                mislukt.append(wat)
                print(f"  NOT CAUGHT: {wat}", file=sys.stderr)
            else:
                print(f"  caught: {wat}")

    n = len(MUTATIES)
    if n < VLOER:  # FLOOR
        print(f"FATAL: {n} mutation cases, floor is {VLOER}. Cases were "
              "removed; a shorter list is an easier test that still reports "
              "green.", file=sys.stderr)
        return 1
    if n > VLOER:  # FLOOR, upward half
        print(f"FATAL: {n} mutation cases, floor is {VLOER}. Cases were added "
              f"without moving the floor. Set VLOER to {n} so the next "
              "removal is still caught.", file=sys.stderr)
        return 1

    if mislukt:
        print(f"\nFAIL: {len(mislukt)} mutation(s) the guard did not catch. "
              "Each one is a way the gate corpus could change without any "
              "check going red.", file=sys.stderr)
        return 1

    print(f"[gate-corpus] OK: all {n} mutations rejected, clean manifest "
          "accepted.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
