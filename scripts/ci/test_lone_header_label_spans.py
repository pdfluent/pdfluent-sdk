#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A lone header label spans the columns it heads, in `dress` (#574).

`cells_placed` in `separations.rs` is the other half of this: the grids below
are the same synthetic slots the Rust tests place, with the same boxes, and
both sides must read the same spans. The separations parity test holds the
two together on real tables; these hold the rule itself, including the
variant the shipped switch does not select.
"""
from __future__ import annotations

import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "learned"))

import train_separations as ts  # noqa: E402


FAILURES: list = []


def check(label: str, got, want) -> None:
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, expected {want!r}")


def full(rows, columns: int) -> set:
    return {(r, c) for r in rows for c in range(columns)}


def boxes_for(held, over=None) -> dict:
    """The (left, right) extent per slot: x=20*c, width 10, as `placed_of`
    builds them in the Rust tests; `over` replaces (r, c) extents."""
    over = over or {}
    return {(r, c): over.get((r, c), (20.0 * c, 20.0 * c + 10.0))
            for (r, c) in held}


def test_lone_header_label_spans_the_columns_it_heads() -> None:
    """One label in column 4 over a stub and eight figure columns."""
    held = {(0, 4)} | full((1, 2), 9)
    header, _, spans = ts.dress(set(held), {}, boxes=boxes_for(held))
    check("header rows", header, {0})
    first = 0 if ts.HEADER_SPAN_TAKES_STUB else 1
    check("the lone label spans the figures",
          spans.get((0, 4)), list(range(first, 9)))


def test_each_variant_reaches_the_columns_its_corpus_writes() -> None:
    held = {(0, 4)} | full((1, 2), 9)
    boxes = boxes_for(held)
    saved = ts.HEADER_SPAN_TAKES_STUB
    try:
        ts.HEADER_SPAN_TAKES_STUB = False
        _, _, spans = ts.dress(set(held), {}, boxes=boxes)
        check("SPAN_TO_ALL leaves the stub slot empty",
              spans.get((0, 4)), list(range(1, 9)))
        ts.HEADER_SPAN_TAKES_STUB = True
        _, _, spans = ts.dress(set(held), {}, boxes=boxes)
        check("SPAN_TO_ALL_WITH_STUB takes the stub slot",
              spans.get((0, 4)), list(range(0, 9)))
    finally:
        ts.HEADER_SPAN_TAKES_STUB = saved


def test_two_header_labels_keep_their_own_spans() -> None:
    """Two groups on one row keep the reach their words were drawn across."""
    held = {(0, 1), (0, 5)} | full((1, 2), 9)
    header, _, spans = ts.dress(set(held), {(0, 1): [1, 2, 3],
                                            (0, 5): [5, 6, 7]},
                                boxes=boxes_for(held))
    check("header rows", header, {0})
    check("the left group keeps its span", spans.get((0, 1)), [1, 2, 3])
    check("the right group keeps its span", spans.get((0, 5)), [5, 6, 7])


def test_a_stub_header_label_stays_put() -> None:
    held = {(0, 0)} | full((1, 2), 9)
    header, _, spans = ts.dress(set(held), {}, boxes=boxes_for(held))
    check("header rows", header, {0})
    check("the stub head takes no span", (0, 0) in spans, False)


def test_a_section_row_still_spans_the_table() -> None:
    held = {(0, 1), (0, 2)} | full((1,), 3) | {(2, 0)} | full((3,), 3)
    header, _, spans = ts.dress(set(held), {}, boxes=boxes_for(held))
    check("header rows", header, {0})
    check("the section row spans the table", spans.get((2, 0)), [0, 1, 2])


def test_undressed_leaves_a_lone_header_unspanned() -> None:
    held = {(0, 4)} | full((1, 2), 9)
    _, _, spans = ts.dress(set(held), {}, dressing=(), boxes=boxes_for(held))
    check("undressed spans nothing", spans.get((0, 4)), None)


def test_a_label_over_the_last_column_keeps_one_column() -> None:
    """"| | | Amount": the label names its column, it does not head the row."""
    held = {(0, 3)} | full((1, 2), 4)
    header, _, spans = ts.dress(set(held), {}, boxes=boxes_for(held))
    check("header rows", header, {0})
    check("the last-column label keeps its column", spans.get((0, 3)), None)


def test_a_lone_label_over_one_figure_column_does_not_span() -> None:
    held = {(0, 1)} | full((1, 2), 2)
    header, _, spans = ts.dress(set(held), {}, boxes=boxes_for(held))
    check("header rows", header, {0})
    check("one figure column is named, not headed", spans.get((0, 1)), None)
    # The boundary: two figure columns, the label centred, spans.
    held = {(0, 1)} | full((1, 2), 3)
    boxes = boxes_for(held, over={(0, 1): (25.0, 45.0)})
    _, _, spans = ts.dress(set(held), {}, boxes=boxes)
    first = 0 if ts.HEADER_SPAN_TAKES_STUB else 1
    check("two centred figure columns span",
          spans.get((0, 1)), list(range(first, 3)))


def test_a_label_centred_at_the_band_edge_spans_and_past_it_does_not() -> None:
    held = {(0, 2)} | full((1, 2), 5)
    first = 0 if ts.HEADER_SPAN_TAKES_STUB else 1
    spanned = list(range(first, 5))

    def columns_of(x0: float, x1: float):
        _, _, spans = ts.dress(set(held), {},
                               boxes=boxes_for(held, over={(0, 2): (x0, x1)}))
        return spans.get((0, 2))

    check("on the low edge: spans", columns_of(32.5, 42.5), spanned)
    check("past the low edge: stays", columns_of(32.4, 42.4), None)
    check("on the high edge: spans", columns_of(67.5, 77.5), spanned)
    check("past the high edge: stays", columns_of(67.6, 77.6), None)


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for test in tests:
        test()
    for f in FAILURES:
        print(f"FAIL {f}")
    if FAILURES:
        print(f"\n{len(FAILURES)} failure(s) in {len(tests)} tests")
        return 1
    print(f"OK {len(tests)} tests: a lone header label spans the columns it heads")
    return 0


if __name__ == "__main__":
    sys.exit(main())
