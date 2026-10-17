#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A page's lines, and the table regions each proposer draws over them (#574).

ONE IMPLEMENTATION, IMPORTED BY THE JUDGE'S TRAINER AND BY ITS PARITY FIXTURE,
for the reason every file in this directory gives: two spellings of "what a
proposal is" agree until they do not.

THREE KINDS OF PROPOSAL, each a SET OF LINE INDICES:
  * `regions`   -- the builder's tables: `table_cell` blocks grouped by their
                   `table` index (T2's lane);
  * `alignment` -- #632's proposer, our own implementation of the published
                   Nurminen / Camelot-stream idea: rows of two or more chunks
                   whose column edges align, broken by a chunk that crosses a
                   column gap;
  * `union`     -- a region and an alignment proposal that share a line, and
                   every region one alignment proposal reaches, joined.

T3's line runs (the line model's `table` class) join as a fourth kind once
`line_class_probs` is in the dump.
"""
from __future__ import annotations

import collections
import re
import sys
import pathlib

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import align_labels as al  # noqa: E402

DIGIT = re.compile(r"[0-9]")

# How far two column edges may differ and still align, in points, and how many
# multi-chunk rows make a table: #632's values, unchanged.
ALIGN_TOLERANCE = 3.0
MIN_ROWS = 3


def page_lines(page: dict) -> tuple[list[dict], list[dict]]:
    """Our lines in reading order, each with the annotated region it falls in."""
    words = page.get("words") or []
    regions = al.regions_of(page)
    # /7: what the list models say of each line, in this same order (#574).
    signal = page.get("list_signal") or []
    lines = []
    for block in page.get("blocks") or []:
        for piece in al.lines_of(block, words):
            if not piece:
                continue
            ink = sum(al.area(w["bbox"]) for w in piece) or 1.0
            best, region = 0.0, None
            for ri, reg in enumerate(regions):
                held = sum(al.overlap(w["bbox"], reg["bbox"]) for w in piece) / ink
                if held > best:
                    best, region = held, ri
            if best < al.CLAIM:
                region = None
            text = " ".join(w["text"] for w in piece)
            lines.append({
                "text": text,
                "bbox": [min(w["bbox"][0] for w in piece), min(w["bbox"][1] for w in piece),
                         max(w["bbox"][2] for w in piece), max(w["bbox"][3] for w in piece)],
                "n_words": len(piece),
                "digits": sum(1 for w in piece if DIGIT.search(w["text"])),
                "kind": block["kind"], "table": block.get("table"),
                "grid_fill": block.get("grid_fill") if block.get("grid_fill") is not None else float("nan"),
                "grid_columns": block.get("grid_columns") if block.get("grid_columns") is not None else float("nan"),
                "list": (signal[len(lines)] if len(lines) < len(signal)
                         and signal[len(lines)] is not None else float("nan")),
                "region": region,
            })
    return lines, regions


def text(lines: list[dict], idx) -> str:
    return " ".join(lines[i]["text"] for i in sorted(idx))


def iou(a: set, b: set) -> float:
    return len(a & b) / max(1, len(a | b))


def bands(boxes: list[list[float]], low: int, high: int) -> list[list[int]]:
    """Indices of boxes grouped where they overlap along one axis."""
    order = sorted(range(len(boxes)), key=lambda i: boxes[i][low])
    out: list[list[int]] = []
    reach = None
    for i in order:
        if out and boxes[i][low] <= reach:
            out[-1].append(i)
            reach = max(reach, boxes[i][high])
        else:
            out.append([i])
            reach = boxes[i][high]
    return out


# --- alignment (#632) ------------------------------------------------------

def _rows(lines):
    order = sorted(range(len(lines)), key=lambda i: -lines[i]["bbox"][3])
    rows = []
    for i in order:
        b = lines[i]["bbox"]
        for r in rows:
            lo, hi = r["lo"], r["hi"]
            short = min(b[3] - b[1], hi - lo) or 1.0
            if (min(b[3], hi) - max(b[1], lo)) / short > 0.5:
                r["m"].append(i)
                r["lo"] = min(lo, b[1])
                r["hi"] = max(hi, b[3])
                break
        else:
            rows.append({"m": [i], "lo": b[1], "hi": b[3]})
    for r in rows:
        r["m"].sort(key=lambda i: lines[i]["bbox"][0])
    rows.sort(key=lambda r: -r["hi"])
    return rows


def _edges(lines, row):
    return [(lines[i]["bbox"][0], lines[i]["bbox"][2]) for i in row["m"]]


def _aligned(a, b, tol):
    n = 0
    for (a0, a1) in a:
        for (b0, b1) in b:
            if abs(a0 - b0) <= tol or abs(a1 - b1) <= tol or abs((a0 + a1) - (b0 + b1)) / 2 <= tol:
                n += 1
                break
    return n


def _crosses(row_edges, gaps):
    return any(x0 < g0 - 1 and x1 > g1 + 1 for (x0, x1) in row_edges for (g0, g1) in gaps)


def alignment(lines: list[dict], tol: float = ALIGN_TOLERANCE,
              min_rows: int = MIN_ROWS) -> list[set[int]]:
    """Tables as runs of rows whose chunks share column edges."""
    rows = _rows(lines)
    tables, cur = [], []

    def close():
        multi = [r for r in cur if len(r["m"]) >= 2]
        if len(multi) >= min_rows or (len(multi) >= 2 and max(len(r["m"]) for r in multi) >= 3):
            while cur and len(cur[-1]["m"]) < 2:
                cur.pop()
            tables.append(list(cur))

    prev_multi = None
    for r in rows:
        e = _edges(lines, r)
        gap = (cur[-1]["lo"] - r["hi"]) if cur else 0
        height = (r["hi"] - r["lo"]) or 1.0
        if len(e) >= 2:
            if (prev_multi is not None and _aligned(_edges(lines, prev_multi), e, tol) >= 2
                    and gap < 2.5 * height):
                cur.append(r)
            else:
                if cur:
                    close()
                cur = [r]
            prev_multi = r
        else:
            if cur and gap < 2.5 * height and prev_multi is not None:
                pe = sorted(_edges(lines, prev_multi))
                gaps = [(pe[k][1], pe[k + 1][0]) for k in range(len(pe) - 1)]
                if not _crosses(e, gaps):
                    cur.append(r)
                    continue
            if cur:
                close()
            cur, prev_multi = [], None
    if cur:
        close()
    return [set(i for r in t for i in r["m"]) for t in tables]


# --- all of them -----------------------------------------------------------

# The kinds a proposal can come from, in the judge's feature order. `builder`
# is the table the dump's blocks already form; the four detectors and
# `alignment_lines` are T2's `table_proposals` (dump /6); `union` is ours.
# The dump's `source` values the table judge is trained and run on.
JUDGE_SOURCES = {"region", "alignment", "split"}

KINDS = ["builder", "aligned", "booktabs", "pair", "leader", "alignment_lines", "split",
         "union"]


def proposals(lines: list[dict], page: dict | None = None) -> list[tuple[set[int], str, bool]]:
    """Every distinct proposal on a page, with the kind that drew it first.

    With a /6 page, T2's detectors and the Rust alignment proposer are read
    from `table_proposals`; without one, the Python alignment port stands in
    for them, as it did before /6 existed.
    """
    groups = collections.defaultdict(set)
    for i, line in enumerate(lines):
        if line["kind"] == "table_cell":
            groups[line["table"]].add(i)
    builder = [g for _, g in sorted(groups.items(), key=lambda kv: str(kv[0])) if g]
    drawn: list[tuple[set[int], str, bool]] = []
    if page is not None and page.get("table_proposals") is not None:
        for prop in page["table_proposals"]:
            # THE JUDGE'S SOURCES ONLY. `lattice` and `ruled` proposals are the
            # verifier's candidates (T2's `learned::vector_proposals`, agreed
            # with T4 on #574); the Rust judge does not see them, and read here
            # they would train it on inputs it never gets (codex, #2058).
            if prop.get("source", "region") not in JUDGE_SOURCES:
                continue
            got = {i for i in prop.get("lines") or [] if 0 <= i < len(lines)}
            if got:
                drawn.append((got, prop.get("detector") or prop.get("source"),
                              bool(prop.get("accepted", True))))
    else:
        drawn = [(c, "alignment_lines", True) for c in alignment(lines)]
    regions = builder + [c for c, k, _ in drawn if k not in ("alignment_lines", "split")]
    aligned = [c for c, k, _ in drawn if k == "alignment_lines"]
    unions = []
    for a in regions:
        for b in aligned:
            if a & b:
                unions.append(a | b)
    for b in aligned:
        hit = [a for a in builder if a & b]
        if len(hit) >= 2:
            unions.append(set().union(*hit) | b)
    out, seen = [], set()
    pools = ([("builder", builder, True)] + [(k, [c], a) for c, k, a in drawn]
             + [("union", unions, True)])
    for kind, pool, accepted in pools:
        for c in pool:
            key = frozenset(c)
            if key not in seen:
                seen.add(key)
                out.append((set(c), kind if kind in KINDS else "aligned", accepted))
    return out


def _self_test() -> None:
    """The judge reads its own sources only (codex, #2058)."""
    lines = [{"kind": "paragraph"} for _ in range(6)]
    page = {"table_proposals": [
        {"source": "region", "detector": "aligned", "lines": [0, 1, 2], "accepted": True},
        {"source": "ruled", "detector": "ruled_frame", "lines": [0, 1, 2, 3, 4], "accepted": True},
        {"source": "lattice", "detector": "lattice", "lines": [3, 4, 5], "accepted": True},
    ]}
    got = [sorted(c) for c, _, _ in proposals(lines, page)]
    assert got == [[0, 1, 2]], got
    print("OK table_proposals: the judge reads region, alignment and split only")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        _self_test()
