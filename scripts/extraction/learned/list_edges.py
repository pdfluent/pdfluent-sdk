#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The list side of the line-link model (#638 rank 1, #574): labels and columns.

    python3 scripts/extraction/learned/list_edges.py --self-test

The link model (T3) decides, for an edge between two lines, whether they are
one block -- and for a list, one ITEM. This file says what an edge's truth is
on a list and what the list models already know about its two ends; the
trainer imports it.

THE LABEL. Two lines are one item when DocLayNet puts them in one `list_item`
region and no line after the first, up to and including the second, OPENS an
item (the `opens` the join in `align_labels.py` decides over the page). Two
lines of any other region are one block when the region is the same. A line
the join gave no region has no truth, and an edge touching it has no label.

THE COLUMNS, per edge (a, b), a above b in reading order:
  a_list_signal, b_list_signal   `learned::list_signal` (dump /7)
  a_p_list, b_p_list             `line_class_probs[.][2]` (dump /8)
  b_marked                       b opens with a bullet, a number or a figure
  b_under_the_hang               b has no marker, a has one, and b is set in
                                 from a's left edge by at least 0.8 em
  a_ends_terminal                a ends in . ! or ?
  dx_em                          b's left edge less a's, in ems
  same_block                     the heuristics put a and b in one block
All of it is computed in Rust already, so the link model reads nothing new.

The key of a line is its place in `lines_of` order, block by block -- the
order `heading_lines::page_rows`, `table_proposals` and `list_signal` share.
"""
from __future__ import annotations

import math
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from align_labels import lines_of  # noqa: E402
from list_lines import marker_kind  # noqa: E402

COLUMNS: list[str] = [
    "a_list_signal", "b_list_signal", "a_p_list", "b_p_list", "b_marked",
    "b_under_the_hang", "a_ends_terminal", "dx_em", "same_block",
]
# The smallest step, in ems, that stands under a hang: `blocks::INDENT_MIN_EM`.
HANG_MIN_EM = 0.8
# `list` in the line class model's frozen order.
LIST = 2


def page_lines(record: dict) -> list[dict]:
    """Every line of the page in `lines_of` order, block by block, with what
    an edge reads of it."""
    words = record.get("words") or []
    signal = record.get("list_signal") or []
    classes = record.get("line_class_probs") or []
    out = []
    for block in record.get("blocks") or []:
        for line in lines_of(block, words):
            if not line:
                continue
            k = len(out)
            text = " ".join(w["text"] for w in line)
            size = sorted(w["size"] for w in line)[len(line) // 2] or 1.0
            out.append({
                "block": block["i"],
                "left": min(w["bbox"][0] for w in line),
                "size": size,
                "marked": marker_kind(line[0]["text"]) is not None,
                "terminal": text.rstrip().endswith((".", "!", "?")),
                "signal": _num(signal[k] if k < len(signal) else None),
                "p_list": _num(classes[k][LIST] if k < len(classes) and classes[k] else None),
            })
    return out


def _num(v) -> float:
    return math.nan if v is None else float(v)


def list_edge_features(lines: list[dict], a: int, b: int) -> list[float]:
    """The columns of edge (a, b), in `COLUMNS` order."""
    la, lb = lines[a], lines[b]
    em = max(la["size"], lb["size"], 1.0)
    dx = (lb["left"] - la["left"]) / em
    return [
        la["signal"], lb["signal"], la["p_list"], lb["p_list"],
        1.0 if lb["marked"] else 0.0,
        1.0 if (la["marked"] and not lb["marked"] and dx >= HANG_MIN_EM) else 0.0,
        1.0 if la["terminal"] else 0.0,
        dx,
        1.0 if la["block"] == lb["block"] else 0.0,
    ]


def same_item(truth: list[tuple | None], a: int, b: int) -> int | None:
    """The edge's label. `truth[k]` is line k's `(region, label, opens)` from the
    alignment rows, or None where the join gave it no region."""
    ta, tb = truth[a], truth[b]
    if ta is None or tb is None:
        return None
    if ta[0] != tb[0]:
        return 0
    if ta[1] != "list_item":
        return 1
    lo, hi = min(a, b), max(a, b)
    for k in range(lo + 1, hi + 1):
        if truth[k] is not None and truth[k][0] == ta[0] and truth[k][2]:
            return 0
    return 1


def truth_of(record: dict, rows: dict) -> list[tuple | None]:
    """Line k's `(region, label, opens)`, in `page_lines` order. `rows` maps
    `(block i, line in block)` to the alignment row's region, label and opens
    for this page."""
    words = record.get("words") or []
    out = []
    for block in record.get("blocks") or []:
        for k, line in enumerate(lines_of(block, words)):
            if line:
                out.append(rows.get((block["i"], k)))
    return out


def self_test() -> int:
    failures = []

    def check(name, got, want):
        ok = got == want or (isinstance(want, float) and isinstance(got, float)
                             and (math.isnan(want) and math.isnan(got) or abs(got - want) < 1e-9))
        print(f"  {'ok' if ok else 'FAIL'}  {name}: {got!r}")
        if not ok:
            failures.append(name)

    # Two items of one list, the second of two lines; a paragraph after it.
    truth = [(0, "list_item", True), (0, "list_item", True), (0, "list_item", False),
             (1, "text", True), None]
    check("two lines of one item are one item", same_item(truth, 1, 2), 1)
    check("the next item's first line is another item", same_item(truth, 0, 1), 0)
    check("across an opener further down it is another item", same_item(truth, 0, 2), 0)
    check("a paragraph under the list is another block", same_item(truth, 2, 3), 0)
    check("a line without a region has no label", same_item(truth, 3, 4), None)
    check("two lines of one paragraph are one block",
          same_item([(4, "text", True), (4, "text", False)], 0, 1), 1)

    def line(text, x, block, signal=0.5, p=0.7):
        return {"block": block, "left": x, "size": 10.0, "marked": marker_kind(text.split()[0]) is not None,
                "terminal": text.rstrip().endswith((".", "!", "?")), "signal": signal, "p_list": p}

    lines = [line("[5] S.-H. Park, O. Simeone, O. Sahin,", 72.0, 0),
             line("networks, IEEE Trans. Veh. Techn.", 84.0, 0),
             line("[6] J. Hoydis, M. Kobayashi.", 72.0, 1)]
    f = dict(zip(COLUMNS, list_edge_features(lines, 0, 1)))
    check("a continuation under the reference's hang", f["b_under_the_hang"], 1.0)
    check("and it opens no marker", f["b_marked"], 0.0)
    check("set 1.2 em in", f["dx_em"], 1.2)
    g = dict(zip(COLUMNS, list_edge_features(lines, 1, 2)))
    check("the next reference opens a marker", g["b_marked"], 1.0)
    check("and is not under a hang", g["b_under_the_hang"], 0.0)
    check("in another block", g["same_block"], 0.0)
    check("one column per name", len(list_edge_features(lines, 0, 2)), len(COLUMNS))
    print(f"list-edges: {'FAIL' if failures else 'OK'} ({len(failures)} failure(s))")
    return 1 if failures else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    print(__doc__)
