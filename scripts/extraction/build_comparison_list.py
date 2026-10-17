#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The document list every column of the extraction comparison runs on (#390).

WHY A LIST AND NOT THE WHOLE SET

The pinned `gate` set is 500 documents and two of the columns are model
pipelines that read a page in seconds rather than milliseconds. A run of all
five hundred would take days, and the honest alternatives are a smaller list
that every column runs, or a table where the cheap tools were measured on more
documents than the expensive ones. The second is not a comparison.

So: one list, every column, same documents, same session. This file writes it,
and it writes it by a rule rather than by a sample, so that the next round can
regenerate it and get the same documents.

THE RULE

  1. Every document the #367 baseline could assess for recall -- oracle text of
     at least twenty words. These are the documents where recall and reading
     order mean anything at all; on the rest the axes report "not measured", and
     a list made of them would compare four tools on their silence.
  2. Every document #389 classified in the recall or reading-order tail, whether
     or not rule 1 already has it. T7 named them (comment on #389, 07-09-2026)
     and they are named here rather than sampled, because a sample that misses
     the known-hard documents publishes a flattering number by accident.
  3. Every tenth of what is left, in manifest order. The gate set is mostly
     documents that once broke a reader -- no text layer, damaged by design --
     and that is exactly what the `parses` axis is for. Without them the table
     would say nothing about the tools that crash.

Usage:
    build_comparison_list.py --out benchmarks/extraction/comparison/gate-tail.txt
    build_comparison_list.py --self-test
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = REPO / "corpus" / "extraction" / "tuning.json"
BASELINE = REPO / "benchmarks" / "runs" / "extraction" / (
    "extraction-2026-09-07-dev-macbook-m1pro-gate.json")

EVERY_NTH = 10

# The tail #389 classified, quoted from T7's reading of the same baseline
# (comment on #389, 07-09-2026). Named and not sampled: these are the documents
# where the tools disagree, and a comparison that drew a sample around them
# would be measuring the easy half of the corpus.
TAIL_FROM_389 = {
    "recall below 95%": [
        "pdfjs__test_pdfs_TAMReview.pdf",
        "pdfjs__test_pdfs_issue6127.pdf",
        "pdfjs__test_pdfs_XiaoBiaoSong.pdf",
        "pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_encryption_Acroform-PDFBOX-2333.pdf",
        "pdfbox__pdfbox_src_test_resources_input_data-000001.pdf",
    ],
    "reading order, both readers agree and we do not": [
        "pdfjs__test_pdfs_tracemonkey.pdf",
        "pdfjs__test_pdfs_highlights.pdf",
        "pdfjs__test_pdfs_issue14438.pdf",
        "pdfbox__pdfbox_src_test_resources_input_merge_PDFBox.GlobalResourceMergeTest.Doc01.pdf",
        "pdfjs__test_pdfs_issue17069.pdf",
        "pdfbox__pdfbox_src_test_resources_input_rendering_source.pdf",
        "verapdf__PDF_UA-2_8.2_Logical_structure_8.2.5_Additional_requirements_for_specific_structure_types_8.2.5.26_Table__Table__TR__TH__TD__THead__TBody__TFoot__8.2.5.26-t05-pass-d.pdf",
    ],
    "reading order, the two readers disagree with each other": [
        "pdfjs__test_pdfs_bigboundingbox.pdf",
        "pdfjs__test_pdfs_160F-2019.pdf",
        "pdfbox__pdfbox_src_test_resources_input_PDFBOX-3110-poems-beads-cropbox.pdf",
        "pdfjs__test_pdfs_canvas.pdf",
        "pdfjs__test_pdfs_issue6605.pdf",
    ],
    "under twenty words, and class-defining": [
        "pdfjs__test_pdfs_copy_paste_ligatures.pdf",
        "pdfbox__pdfbox_src_test_resources_org_apache_pdfbox_text_BidiSample.pdf",
        "pdfjs__test_pdfs_noembed-identity-2.pdf",
        "pdfjs__test_pdfs_issue7492.pdf",
    ],
}


def assessed(baseline: dict) -> set[str]:
    """The documents the baseline could judge: oracle text, and enough of it."""
    return {
        name for name, record in baseline["documents"].items()
        if record.get("char_recall", -1) >= 0 and not record.get("short")
    }


def build(names: list[str], assessable: set[str], tail: set[str],
          every_nth: int) -> tuple[list[str], dict]:
    """The list, in manifest order, and what each rule contributed.

    Manifest order and not the order the rules ran in: a list whose order
    encodes which rule caught a document would make the tenth-document rule
    depend on the other two.
    """
    rest = [n for n in names if n not in assessable and n not in tail]
    sampled = set(rest[every_nth - 1::every_nth])
    chosen = assessable | tail | sampled
    ordered = [n for n in names if n in chosen]
    return ordered, {
        "assessed by the #367 baseline": len(assessable),
        "the #389 tail": len(tail),
        "every %dth of the remainder" % every_nth: len(sampled),
        "total": len(ordered),
    }


def header(counts: dict, every_nth: int) -> list[str]:
    lines = [
        "# The documents every column of the extraction comparison runs on (#390).",
        "# Generated by scripts/extraction/build_comparison_list.py -- do not edit by",
        "# hand: the rule is the point, and a hand-edited list is a sample nobody can",
        "# reproduce. Names are entries of the `gate` set of corpus/extraction/tuning.json,",
        "# in manifest order, and the harness verifies every SHA-256 before measuring.",
        "#",
        "# Rule: every document the #367 baseline could assess for recall, every document",
        f"# #389 classified in the tail, and every {every_nth}th of the remainder.",
        "#",
    ]
    lines += [f"#   {label}: {count}" for label, count in counts.items()]
    return lines


def self_test() -> int:
    failures = []

    def check(label, got, want):
        if got != want:
            failures.append(f"{label}: got {got!r}, expected {want!r}")

    names = [f"{i:03d}.pdf" for i in range(1, 21)]
    assessable = {"001.pdf", "002.pdf"}
    tail = {"020.pdf"}
    chosen, counts = build(names, assessable, tail, every_nth=5)
    check("manifest order is kept", chosen, sorted(chosen))
    check("the assessable documents are in", {"001.pdf", "002.pdf"} <= set(chosen), True)
    check("the named tail is in", "020.pdf" in chosen, True)
    # 17 remain after the three above; every fifth of them is three documents.
    check("the sample is every fifth of the remainder",
          counts["every 5th of the remainder"], 3)
    check("the counts add up", counts["total"], len(chosen))
    check("no document twice", len(set(chosen)), len(chosen))

    # A tail document that the sample would never have reached is still there.
    chosen_without_tail, _ = build(names, assessable, set(), every_nth=5)
    check("the tail is what naming it adds",
          "020.pdf" in chosen_without_tail, False)

    listed = {n for group in TAIL_FROM_389.values() for n in group}
    check("the #389 tail is 21 named documents", len(listed), 21)

    for problem in failures:
        print(f"FAIL {problem}")
    if failures:
        return 1
    print("OK self-test: the rule is order-independent, names beat samples, "
          "and nothing is listed twice")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--set", default="gate")
    parser.add_argument("--every-nth", type=int, default=EVERY_NTH)
    parser.add_argument("--out")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.out:
        parser.error("--out is required unless --self-test")

    with MANIFEST.open(encoding="utf-8") as handle:
        manifest = json.load(handle)
    names = [e["name"] for e in manifest["sets"][args.set]["entries"]]
    with BASELINE.open(encoding="utf-8") as handle:
        baseline = json.load(handle)

    known = set(names)
    tail = {n for group in TAIL_FROM_389.values() for n in group}
    unknown = tail - known
    if unknown:
        print(f"FAIL {len(unknown)} document(s) of the #389 tail are not in set "
              f"{args.set}: {', '.join(sorted(unknown))}", file=sys.stderr)
        return 1

    chosen, counts = build(names, assessed(baseline) & known, tail, args.every_nth)
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(header(counts, args.every_nth) + chosen) + "\n",
                   encoding="utf-8")
    print(f"OK wrote {out} ({counts['total']} documents)")
    for label, count in counts.items():
        print(f"   {label}: {count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
