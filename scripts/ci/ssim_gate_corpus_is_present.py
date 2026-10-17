#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Are the fifty documents the SSIM gate grades actually on this runner?

The gate downstream renders fifty IRS and USCIS forms and compares them against
a mean SSIM of 0.94 and a floor of 35% fully correct. Those two numbers were
measured on THESE fifty documents; run against any other set they are numbers
that have stopped meaning anything.

The documents left the repository on 26-05-2026 in 2e3d2052 ("M9-A release
hygiene") and the manifest stayed pinned to their old paths, so 48 of the 50
have been absent ever since. That is not a rendering regression and it is not a
pass, and until 07-09-2026 the wave reported it as a failure -- red for 9m54s of
every run, in the same colour a real regression would have used (#361).

So this answers one question and answers it in the shape the rest of this
repository uses for a measurement that could not be made:

  0  every document is present and is the document the thresholds were
     measured on; the gate can run
  3  SKIPPED (not a pass) -- said on stderr, with what is missing
  2  the manifest itself cannot be read, which is a defect in the tree rather
     than a state of the runner

PRESENCE IS NOT IDENTITY. IRS and USCIS reissue their forms every year, so a
runner carrying this year's f1040.pdf would satisfy a file-count check and then
be scored against thresholds calibrated on a different document. The sha256 in
the manifest is the bytes those numbers were measured on, recovered from git
history, so a right name with wrong bytes is a skip and not a run.

Usage: ssim_gate_corpus_is_present.py --manifest corpus/SSIM_GATE_MANIFEST.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys

# How many names to print. The failing case is "48 of 50 are gone", and fifty
# paths in the log tell a reader nothing the count did not already.
TOON = 5


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", required=True, type=pathlib.Path)
    args = p.parse_args()

    try:
        entries = json.loads(args.manifest.read_text(encoding="utf-8"))["entries"]
    except (OSError, ValueError, KeyError) as exc:
        print(f"[ssim-corpus] FATAL: cannot read {args.manifest}: {exc}", file=sys.stderr)
        return 2
    if not entries:
        print(f"[ssim-corpus] FATAL: {args.manifest} names no documents at all, so "
              "'all present' would be true of an empty gate", file=sys.stderr)
        return 2

    weg: list[str] = []
    verkeerd: list[str] = []
    for entry in entries:
        pad = pathlib.Path(entry["source"])
        if not pad.is_file():
            weg.append(entry["file"])
            continue
        if entry.get("sha256") and hashlib.sha256(pad.read_bytes()).hexdigest() != entry["sha256"]:
            verkeerd.append(entry["file"])

    if weg:
        print(f"SKIPPED (not a pass): {len(weg)}/{len(entries)} pinned forms are not "
              "on this runner.", file=sys.stderr)
        print(f"  first missing: {' '.join(weg[:TOON])}", file=sys.stderr)
        print("  corpus/SSIM_GATE_MANIFEST.json names them as paths inside the "
              "repository; they were removed in 2e3d2052 on 26-05-2026 and the gate "
              "corpus fetched before this step contains none of them. No page was "
              "rendered and no SSIM was computed, so this is not a regression and "
              "not a pass. The 0.94/35% thresholds return with the corpus, not as "
              "lower numbers. (#276)", file=sys.stderr)
        return 3

    if verkeerd:
        print(f"SKIPPED (not a pass): {len(verkeerd)}/{len(entries)} pinned forms are "
              "present but are not the documents these thresholds were calibrated "
              f"on: {' '.join(verkeerd[:TOON])}", file=sys.stderr)
        print("  IRS and USCIS reissue their forms, so a runner can carry a file of "
              "the right name and the wrong year. Scoring 0.94 mean and 35% fully "
              "correct against a different document is a number that has stopped "
              "meaning anything. Rebuild the snapshot from the pinned bytes. (#276)",
              file=sys.stderr)
        return 3

    print(f"[ssim-corpus] OK: all {len(entries)} pinned forms are present and match "
          "the bytes the thresholds were measured on.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
