#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The extraction gate set names documents the gate corpus actually delivers.

Two manifests, one step. `Text extraction measures its four axes` fetches the
gate corpus from `corpus/GATE_CORPUS_MANIFEST.json` and then measures the
`gate` set of `corpus/extraction/tuning.json` inside it. Nothing has ever made
the two agree, and they are edited by different pieces of work: one is the
corpus a renderer gate is calibrated on, the other is the set an extraction
baseline was measured over.

When they drift the step does not degrade, it stops: extraction_axes.py refuses
a partial set on purpose (`FAIL n document(s) of set gate are not in <dir>`).
That refusal is correct and it arrives forty minutes into a wave, on a machine
that is billed by the hour. This says the same thing in two seconds, offline,
before the push.

Also checked: every name is a bare filename. fetch_gate_corpus.py writes the
corpus flat, one file per entry, so a name carrying a directory would be looked
for in a place the fetcher never writes.

Exit codes:
  0  every document of the extraction gate set is in the gate corpus
  1  one is not, a name is not flat, or a set came back so small that the
     comparison would pass on emptiness
"""
from __future__ import annotations

import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CORPUS = REPO / "corpus" / "GATE_CORPUS_MANIFEST.json"
TUNING = REPO / "corpus" / "extraction" / "tuning.json"
SET = "gate"

# FLOOR: the gate corpus is ratcheted at 500 entries by gate_corpus_is_pinned.py
# and the extraction set was measured over the same 500. An empty read compares
# nothing and reports agreement, which is the failure this file exists to avoid.
MINIMUM = 500


def disagreements(extraction_names: list[str], corpus_names: list[str]) -> list[str]:
    """What the extraction set asks for and the corpus does not deliver."""
    have = set(corpus_names)
    problems = []
    for name in extraction_names:
        if "/" in name or "\\" in name:
            problems.append(
                f"{name}: the corpus is written flat, so a name with a directory "
                "in it is looked for where nothing is written"
            )
        elif name not in have:
            problems.append(f"{name}: not in the gate corpus manifest")
    return problems


def main() -> int:
    for path in (CORPUS, TUNING):
        if not path.is_file():
            print(f"[extraction-set] FATAL: {path} is missing, so nothing was "
                  "compared. That is not agreement.", file=sys.stderr)
            return 1

    corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
    tuning = json.loads(TUNING.read_text(encoding="utf-8"))

    corpus_names = [e["name"] for e in corpus["entries"]]
    if SET not in tuning.get("sets", {}):
        print(f"[extraction-set] FATAL: {TUNING.name} has no set {SET!r}; the CI "
              f"step measures that one. It has {sorted(tuning.get('sets', {}))}.",
              file=sys.stderr)
        return 1
    extraction_names = [e["name"] for e in tuning["sets"][SET]["entries"]]

    if len(corpus_names) < MINIMUM or len(extraction_names) < MINIMUM:
        print(f"[extraction-set] FATAL: corpus {len(corpus_names)}, extraction set "
              f"{len(extraction_names)}, floor {MINIMUM} for both. A short read "
              "agrees with anything.", file=sys.stderr)
        return 1

    problems = disagreements(extraction_names, corpus_names)
    if problems:
        print(f"[extraction-set] FAIL: {len(problems)} document(s) of set {SET!r} "
              "cannot come out of the gate corpus:", file=sys.stderr)
        for p in problems[:25]:
            print(f"    {p}", file=sys.stderr)
        if len(problems) > 25:
            print(f"    ... and {len(problems) - 25} more", file=sys.stderr)
        print("\n  Rebuild one of the two so they name the same documents, or the "
              "extraction step stops on the throwaway instance rather than here. "
              "(#448)", file=sys.stderr)
        return 1

    print(f"[extraction-set] OK: all {len(extraction_names)} document(s) of set "
          f"{SET!r} are in the {len(corpus_names)}-entry gate corpus")
    return 0


if __name__ == "__main__":
    sys.exit(main())
