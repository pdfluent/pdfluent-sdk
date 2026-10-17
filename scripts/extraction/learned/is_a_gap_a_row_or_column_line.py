#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Can a gap be told from a boundary, before any model is trained? (#574)

    is_a_gap_a_row_or_column_line.py --dump dump5.jsonl

WHY A SEPARATION AND NOT A PAIR. Our words in the annotation's own grid score
TEDS 0.9404 where the grid we build scores 0.5996 and the learned pair grid
0.6073: the whole loss is the grid, and the pair model buys 0.0077 of the 0.34
that is there. A pair model asks "do these two words belong together" and
closes the grid transitively, so one wrong pair walks through the closure. A
separation is asked once per gap -- tens of questions per table instead of
thousands -- and a wrong answer stays one wrong row.

WHAT THIS ASKS, and it asks it before training anything. Between two bands of
words there is a gap. Is it a boundary between rows (or columns), or is it the
space inside one? Reported as AUC per feature, threshold-free, split by
document -- because a feature that only separates pooled is recognising the
document, which cost the heading axis 0.783 -> 0.658 in `document_features.py`.

If nothing separates, the route dies here for the price of one pass.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import random
import statistics
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import pubtables_alignment as pa  # noqa: E402
import pubtables_cells  # noqa: E402


def bands(items, low, high, order):
    """Group boxes into bands that overlap along one axis, in reading order."""
    items = sorted(items, key=order)
    out: list[list] = []
    for item in items:
        if out and low(item) <= high(out[-1][-1]) and high(item) >= low(out[-1][-1]):
            out[-1].append(item)
        elif out and any(low(item) <= high(x) and high(item) >= low(x) for x in out[-1]):
            out[-1].append(item)
        else:
            out.append([item])
    return out


def auc(positive: list[float], negative: list[float]) -> float:
    if not positive or not negative:
        return float("nan")
    values = sorted(positive + negative)
    rank, i = {}, 0
    while i < len(values):
        j = i
        while j + 1 < len(values) and values[j + 1] == values[i]:
            j += 1
        for k in range(i, j + 1):
            rank[values[k]] = (i + j) / 2.0 + 1.0
        i = j + 1
    n, m = len(positive), len(negative)
    return (sum(rank[v] for v in positive) - n * (n + 1) / 2.0) / (n * m)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()

    # THE SAME TEST SPLIT AND THE SAME ANNOTATIONS as `region_end_to_end.py`,
    # so these numbers sit beside its ceiling rather than beside a set of their
    # own.
    STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
    SEED = 20260924
    HEIGHT_SLACK = 1.5
    manifest = json.loads((STORE / "pdf" / "MANIFEST.json").read_text())
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    documents = sorted({p["document"] for p in manifest["pages"]})
    random.Random(SEED).shuffle(documents)
    test = set(documents[int(len(documents) * 0.75):])
    annotations: dict[str, list] = {}
    with tarfile.open(STORE / "PubTables-1M-PDF_Annotations.tar.gz", "r|gz") as tar:
        for member in tar:
            if not member.name.endswith("_tables.json"):
                continue
            pmc = member.name.split("_")[0]
            if pmc not in test:
                continue
            handle = tar.extractfile(member)
            if handle is None:
                continue
            try:
                annotations[pmc] = [
                    {"pdf_page_index": t.get("pdf_page_index"),
                     "pdf_table_bbox": t.get("pdf_table_bbox"),
                     "pdf_full_page_bbox": t.get("pdf_full_page_bbox"),
                     "exclude_for_structure": t.get("exclude_for_structure"),
                     "cells": [{"row_nums": c.get("row_nums"),
                                "column_nums": c.get("column_nums"),
                                "pdf_bbox": c.get("pdf_bbox"),
                                "pdf_text_tight_bbox": c.get("pdf_text_tight_bbox")}
                               for c in (t.get("cells") or [])]}
                    for t in json.load(handle)]
            except Exception:
                pass
    print(f"annotations for {len(annotations)} test document(s)", flush=True)

    rows_pos: list[dict] = []
    rows_neg: list[dict] = []
    cols_pos: list[dict] = []
    cols_neg: list[dict] = []
    by_doc_rows: dict[str, tuple[list, list]] = collections.defaultdict(lambda: ([], []))
    tables = 0
    per_table_gaps: list[int] = []

    for line in open(args.dump):
        if args.limit and tables >= args.limit:
            break
        record = json.loads(line)
        words = record.get("words") or []
        rules = record.get("rules") or []
        height = float(record["height"])
        page = by_sha.get(record["pdf_sha256"])
        if not page or page["document"] not in test:
            continue
        ann = [t for t in (annotations.get(page["document"]) or [])
               if int(t.get("pdf_page_index", -1)) == int(page["page_no"])
               and not t.get("exclude_for_structure")
               and abs(height - float(t["pdf_full_page_bbox"][3])) <= HEIGHT_SLACK]
        if not ann:
            continue
        for table in ann:
            cells = table.get("cells") or []
            if len(cells) < 4:
                continue
            box = pa.page_frame(table["pdf_table_bbox"], height)
            inside = [w for w in words
                      if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                      and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
            if len(inside) < 6:
                continue
            placed = pa.place([w["bbox"] for w in inside], cells, height)
            held = [(w, cells[c]) for w, c in zip(inside, placed) if c is not None]
            if len(held) < 6:
                continue
            tables += 1
            median_h = statistics.median([w["bbox"][3] - w["bbox"][1] for w, _ in held]) or 1.0
            median_w = statistics.median([w["bbox"][2] - w["bbox"][0] for w, _ in held]) or 1.0

            # WHAT THE GAP IS AGAINST THIS TABLE'S OWN GAPS. The pooled AUC of
            # the raw gap is 0.803 and the per-table AUC is 0.953: inside one
            # table the question is nearly decided, and what differs between
            # tables is the SCALE. A threshold learned across tables therefore
            # cannot be the one each table wants, and the fix is not a better
            # threshold but a feature that carries the table's own scale.
            def against_the_table(values):
                ordered = sorted(values)
                middle = statistics.median(ordered) if ordered else 1.0
                return middle or 1.0

            def the_tight_scale(values):
                """The SMALLEST gaps, which is the leading inside a cell.

                Dividing by the median made both axes worse -- 0.803 to 0.755
                on rows -- and the reason is in the class balance: 83 per cent
                of the row gaps ARE boundaries, so the median gap is itself a
                boundary and the scale it sets erases the absolute size that
                was doing the work. The contrast a decoder faces is "line
                spacing inside a cell" against "space between rows", so the
                scale that means something is the small end.
                """
                ordered = sorted(v for v in values if v > 0)
                if not ordered:
                    return 1.0
                return ordered[max(0, len(ordered) // 10 - 1)] or 1.0

            # ROWS: bands down the page, y up so the top band comes first.
            lines = bands(held, lambda i: i[0]["bbox"][1], lambda i: i[0]["bbox"][3],
                          lambda i: -i[0]["bbox"][3])
            per_table_gaps.append(max(0, len(lines) - 1))
            row_gaps = [min(w["bbox"][1] for w, _ in a) - max(w["bbox"][3] for w, _ in b)
                        for a, b in zip(lines, lines[1:])]
            row_scale = against_the_table(row_gaps)
            row_tight = the_tight_scale(row_gaps)
            row_sorted = sorted(row_gaps)
            for above, below in zip(lines, lines[1:]):
                top = min(w["bbox"][1] for w, _ in above)
                bottom = max(w["bbox"][3] for w, _ in below)
                gap = top - bottom
                up = {r for _, c in above for r in c["row_nums"]}
                down = {r for _, c in below for r in c["row_nums"]}
                boundary = bool(up) and bool(down) and max(up) < min(down)
                ruled = any(bottom - 1.0 <= r["y"] <= top + 1.0 for r in rules)
                numeric_above = sum(1 for w, _ in above
                                    if any(ch.isdigit() for ch in w["text"])) / len(above)
                numeric_below = sum(1 for w, _ in below
                                    if any(ch.isdigit() for ch in w["text"])) / len(below)
                row = {
                    "the gap itself": gap,
                    "the gap in text heights": gap / median_h,
                    "a ruling lies in it": 1.0 if ruled else 0.0,
                    "the number pattern changes": abs(numeric_above - numeric_below),
                    "how many words above": float(len(above)),
                    "the gap against this table's median": gap / row_scale,
                    "the gap against this table's tightest": gap / row_tight,
                    "where the gap ranks in this table":
                        (sum(1 for g in row_sorted if g < gap) / max(1, len(row_sorted))),
                }
                (rows_pos if boundary else rows_neg).append(row)
                doc = record.get("pdf_sha256", "")
                by_doc_rows[doc][0 if boundary else 1].append(row)

            # COLUMNS: bands across the page.
            stripes = bands(held, lambda i: i[0]["bbox"][0], lambda i: i[0]["bbox"][2],
                            lambda i: i[0]["bbox"][0])
            col_gaps = [min(w["bbox"][0] for w, _ in b) - max(w["bbox"][2] for w, _ in a)
                        for a, b in zip(stripes, stripes[1:])]
            col_scale = against_the_table(col_gaps)
            col_tight = the_tight_scale(col_gaps)
            col_sorted = sorted(col_gaps)
            for left, right in zip(stripes, stripes[1:]):
                edge = max(w["bbox"][2] for w, _ in left)
                start = min(w["bbox"][0] for w, _ in right)
                gap = start - edge
                a = {c for _, cell in left for c in cell["column_nums"]}
                b = {c for _, cell in right for c in cell["column_nums"]}
                boundary = bool(a) and bool(b) and max(a) < min(b)
                numeric_left = sum(1 for w, _ in left
                                   if any(ch.isdigit() for ch in w["text"])) / len(left)
                numeric_right = sum(1 for w, _ in right
                                    if any(ch.isdigit() for ch in w["text"])) / len(right)
                row = {
                    "the gap itself": gap,
                    "the gap in word widths": gap / median_w,
                    "the number pattern changes": abs(numeric_left - numeric_right),
                    "how many words left": float(len(left)),
                    "the gap against this table's median": gap / col_scale,
                    "the gap against this table's tightest": gap / col_tight,
                    "where the gap ranks in this table":
                        (sum(1 for g in col_sorted if g < gap) / max(1, len(col_sorted))),
                }
                (cols_pos if boundary else cols_neg).append(row)

    def report(name, pos, neg, per_doc=None):
        print(f"\n{name}: {len(pos)} boundaries, {len(neg)} gaps that are not "
              f"({100 * len(pos) / max(1, len(pos) + len(neg)):.0f}% positive)")
        if not pos or not neg:
            print("  nothing to separate")
            return
        print(f"  {'feature':32s}{'AUC pooled':>12s}" +
              (f"{'AUC by table':>14s}" if per_doc else ""))
        for feature in pos[0]:
            pooled = auc([r[feature] for r in pos], [r[feature] for r in neg])
            line = f"  {feature:32s}{pooled:12.3f}"
            if per_doc:
                each = [auc([r[feature] for r in p], [r[feature] for r in n])
                        for p, n in per_doc.values() if p and n]
                each = [v for v in each if v == v]
                if each:
                    line += f"{statistics.median(each):14.3f}"
            print(line)

    print(f"{tables} table(s); median {statistics.median(per_table_gaps) if per_table_gaps else 0:.0f} "
          f"row gaps per table")
    report("ROW boundaries", rows_pos, rows_neg, by_doc_rows)
    report("COLUMN boundaries", cols_pos, cols_neg)
    print("\n0.5 is no signal. A boundary is a gap the grid changes across; the "
          "negatives are\nthe gaps inside one row or column, which is the "
          "contrast a decoder actually faces.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
