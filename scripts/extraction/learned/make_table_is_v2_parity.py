"""Write the fixture that holds the final-table features to their Python (#574, #638).

    make_table_is_v2_parity.py

Synthetic tables only -- no rig pages, no dumps: each one's cells (text, box,
row, column, spans, the heading probability the pass decides on), the page
height, the judge probability, whether it came from the builder, and whether
the page drew rulings under it, plus what Python makes of them -- the 29
features in `TABLE_IS_V2_NAMES` order. The Rust test rebuilds the blocks and
hints and recomputes all of it. A feature that drifts between the two turns
it red.

Python recomputes here and NOWHERE else: the trainer reads the dump's
vectors and never calls this.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations

import json
import math
import pathlib
import re
import statistics
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
REPO = HERE.parents[3]
OUT = REPO / "crates" / "pdf-engine" / "tests" / "fixtures" / "learned" / "table_is_v2_parity.json"

NUM = re.compile(r'^[\s$€£%()+\-–−.,0-9]+$')
NAMES = ['rows', 'cols', 'cells', 'numeric', 'words_cell', 'col_words_max', 'col_words_min',
         'heading_share', 'p_head_max', 'p_head_mean', 'first_col_p_head', 'empty', 'height_share',
         'wordy_share', 'first_col_words', 'words_filled_median', 'words_filled_mean',
         'figure_share_filled', 'sentence_share', 'lines_median', 'run_on_share', 'row_overlap',
         'column_start_spread', 'judge_p', 'from_builder', 'grid_rows', 'grid_columns',
         'filled_share', 'has_rulings']


def _filled(cells):
    return [c for c in cells if (c["text"] or "").strip()]


def _ends_sentence(text):
    t = (text or "").rstrip()
    return len(t) >= 2 and t[-1] in ".!?" and t[-2].isascii() and t[-2].islower()


def _starts_lower(text):
    t = (text or "").lstrip()
    return bool(t) and t[0].islower()


def _median(xs):
    return statistics.median(xs) if xs else 0.0


def _lines_of(spans):
    """How many text lines a cell's spans stand on: `v2_lines_of_cell`."""
    boxes = [[x, y, x + w, y + h] for x, y, w, h in spans]
    if not boxes:
        return 1
    boxes.sort(key=lambda b: -b[3])
    lines = []
    for b in boxes:
        for i, (low, high) in enumerate(lines):
            shorter = max(min(b[3] - b[1], high - low), sys.float_info.min)
            shared = max(0.0, min(b[3], high) - max(b[1], low))
            if shared / shorter > 0.5:
                lines[i] = (min(low, b[1]), max(high, b[3]))
                break
        else:
            lines.append((b[1], b[3]))
    return max(1, len(lines))


def _row_overlap(cells):
    by_row = {}
    for c in _filled(cells):
        by_row.setdefault(c["row"], []).append(c)
    ratios = []
    for row in by_row.values():
        if len(row) < 2:
            continue
        bottom = max(c["bbox"][1] for c in row)
        top = min(c["bbox"][3] for c in row)
        shared = max(0.0, top - bottom)
        mean_h = statistics.fmean(max(0.0, c["bbox"][3] - c["bbox"][1]) for c in row)
        if mean_h > 0:
            ratios.append(shared / mean_h)
    return statistics.fmean(ratios) if ratios else 1.0


def _column_start_spread(cells):
    by_col = {}
    for c in _filled(cells):
        by_col.setdefault(c["column"], []).append(c)
    tops = []
    for col in by_col.values():
        col.sort(key=lambda c: c["row"])
        tops.append(col[0]["bbox"][3])
    heights = [max(0.0, c["bbox"][3] - c["bbox"][1]) for c in _filled(cells)]
    median_h = statistics.median(heights) if heights else float("nan")
    if not math.isfinite(median_h) or median_h <= 0:
        return 0.0
    return statistics.pstdev(tops) / median_h if tops else 0.0


def _run_on_share(cells):
    by_col = {}
    for c in _filled(cells):
        by_col.setdefault(c["column"], []).append(c)
    pairs = on = 0
    for col in by_col.values():
        col.sort(key=lambda c: c["row"])
        for upper, lower in zip(col, col[1:]):
            pairs += 1
            upper_t, lower_t = (upper["text"] or "").strip(), (lower["text"] or "").strip()
            if upper_t and lower_t and not _ends_sentence(upper_t) and _starts_lower(lower_t):
                on += 1
    return on / pairs if pairs else 0.0


def _v1(height, cells):
    """The first fifteen, as `train_table_is.feats` computes them."""
    import train_table_is as T
    wrapped = [{
        "row": c["row"], "column": c["column"],
        "n_words": len((c["text"] or "").split()),
        "p_heading_pass": c.get("p"),
        "bbox": c["bbox"], "text": c["text"] or "",
    } for c in cells]
    return T.feats({"height": height}, wrapped)


def feats(height, cells, judge_p, from_builder, has_rulings):
    """All 29, in NAMES order: `table_is_v2_features`."""
    v1 = _v1(height, cells)
    filled = _filled(cells)
    words = [len((c["text"] or "").split()) for c in filled]
    figure = sum(1 for c in filled if NUM.match(c["text"] or "")) / max(1, len(filled))
    sentence = sum(1 for c in filled if _ends_sentence(c["text"] or "")) / max(1, len(filled))
    lines = _median([_lines_of(c.get("spans") or []) for c in filled])
    grid_rows = max([c["row"] + max(1, c.get("row_span") or 1) for c in cells], default=0)
    grid_cols = max([c["column"] + max(1, c.get("column_span") or 1) for c in cells], default=0)
    filled_share = len(filled) / (grid_rows * grid_cols) if grid_rows and grid_cols else 0.0
    return v1 + [
        _median(words),
        statistics.fmean(words) if words else 0.0,
        figure if filled else 0.0,
        sentence if filled else 0.0,
        lines,
        _run_on_share(cells),
        _row_overlap(cells),
        _column_start_spread(cells),
        judge_p,
        1.0 if from_builder else 0.0,
        float(grid_rows),
        float(grid_cols),
        filled_share,
        1.0 if has_rulings else 0.0,
    ]


def _cell(row, column, text, x0, y0, x1, y1, p=0.0, spans=None, row_span=1, column_span=1):
    return {"text": text, "bbox": [x0, y0, x1, y1], "row": row, "column": column,
            "row_span": row_span, "column_span": column_span, "p": p,
            "spans": spans if spans is not None else [[x0, y0, x1 - x0, y1 - y0]]}


def tables():
    """Synthetic tables: figures, prose, rulings, spans, fragments, holes."""
    out = []
    # A financial statement: labels plus figures in three columns.
    cells = [_cell(0, c, t, 70 + 90 * c, 700, 150 + 90 * c, 712, 0.05)
             for c, t in enumerate(["", "2025", "2024", "2023"])]
    rows = [["Revenue", "12,345", "11,204", "9,876"],
            ["Costs", "8,112", "7,990", "7,101"],
            ["Profit", "4,233", "3,214", "2,775"]]
    for r, texts in enumerate(rows, start=1):
        for c, t in enumerate(texts):
            cells.append(_cell(r, c, t, 70 + 90 * c, 700 - 20 * r, 150 + 90 * c, 712 - 20 * r, 0.02))
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.91,
                "from_builder": True, "has_rulings": False})
    # Two columns of director biographies that read on: the upper ends
    # mid-sentence and the lower starts lower case, down both columns.
    bios = [
        ("Anna Berg joined the board in", "and chairs the audit committee."),
        ("Bo Dahl led the turnaround of", "the nordic division from 2019."),
        ("Cia Elm founded two software", "companies before joining in 2021."),
        ("Dan Fox ran operations across", "three continents for a decade."),
    ]
    cells = []
    for r, (left, right) in enumerate(bios):
        # Staggered: the right column sits half a line below the left.
        cells.append(_cell(r, 0, left, 70, 700 - 22 * r, 290, 714 - 22 * r, 0.1))
        cells.append(_cell(r, 1, right, 310, 694 - 22 * r, 530, 708 - 22 * r, 0.1))
    # The columns read on vertically too: each column's lower cell starts low.
    cells.append(_cell(4, 0, "she previously served as CFO", 70, 612, 290, 626, 0.1))
    cells.append(_cell(4, 1, "he previously served as COO", 310, 606, 530, 620, 0.1))
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.62,
                "from_builder": False, "has_rulings": False})
    # A text table: names and one-sentence descriptions, no figures, ruled.
    cells = [_cell(0, 0, "Name", 70, 700, 150, 712, 0.3),
             _cell(0, 1, "Role", 160, 700, 400, 712, 0.3)]
    for r, (name, role) in enumerate([("Ada", "Built the first engine."),
                                      ("Bob", "Runs the second plant."),
                                      ("Cy", "Keeps the third ledger.")], start=1):
        cells.append(_cell(r, 0, name, 70, 700 - 20 * r, 150, 712 - 20 * r, 0.05))
        cells.append(_cell(r, 1, role, 160, 700 - 20 * r, 400, 712 - 20 * r, 0.05))
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.44,
                "from_builder": False, "has_rulings": True})
    # A fragment: five rows of a twenty-row statement, figures intact.
    cells = []
    for r in range(5):
        cells.append(_cell(r, 0, f"Line {r}", 70, 700 - 20 * r, 150, 712 - 20 * r, 0.02))
        cells.append(_cell(r, 1, f"{1000 + r:,}", 160, 700 - 20 * r, 240, 712 - 20 * r, 0.01))
        cells.append(_cell(r, 2, f"{2000 + r:,}", 250, 700 - 20 * r, 330, 712 - 20 * r, 0.01))
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.58,
                "from_builder": False, "has_rulings": False})
    # A header over two columns plus a hole: spans and empties.
    cells = [_cell(0, 0, "Quarter", 70, 700, 250, 712, 0.2, column_span=2),
             _cell(1, 0, "Q1", 70, 680, 150, 692, 0.05),
             _cell(1, 1, "1,001", 160, 680, 250, 692, 0.01),
             _cell(2, 0, "", 70, 660, 150, 672, 0.0),
             _cell(2, 1, "2,002", 160, 660, 250, 672, 0.01)]
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.77,
                "from_builder": True, "has_rulings": True})
    # A cell of two lines plus a superscript that stays on its line.
    cells = [_cell(0, 0, "Alpha", 70, 700, 150, 712, 0.05),
             _cell(0, 1, "Beta", 160, 700, 250, 712, 0.05),
             _cell(1, 0, "Gamma delta", 70, 670, 150, 694, 0.05,
                    spans=[[70, 682, 80, 12], [70, 670, 80, 10]]),
             _cell(1, 1, "E=mc2", 160, 680, 250, 692, 0.05,
                    spans=[[160, 680, 70, 12], [225, 688, 8, 6]])]
    out.append({"height": 792.0, "cells": cells, "judge_p": float("nan"),
                "from_builder": True, "has_rulings": False})
    # One column reads on, the other stands alone; staggered starts.
    cells = []
    for r, (a, b) in enumerate([("first half of", "Total"),
                                ("a long sentence here", "1,234"),
                                ("and its ending here.", "5,678")]):
        y = 700 - 20 * r if a != "Total" else 700 - 20 * r
        cells.append(_cell(r, 0, a, 70, y, 250, y + 12, 0.05))
        # The second column starts a line lower.
        y2 = y if r else y - 20
        cells.append(_cell(r, 1, b, 260, y2, 340, y2 + 12, 0.05))
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.51,
                "from_builder": False, "has_rulings": False})
    # A single column of short labels.
    cells = [_cell(r, 0, t, 70, 700 - 20 * r, 150, 712 - 20 * r, 0.05)
             for r, t in enumerate(["North", "South", "East", "West"])]
    out.append({"height": 792.0, "cells": cells, "judge_p": 0.2,
                "from_builder": True, "has_rulings": False})
    return out


def main() -> int:
    found = tables()
    out = []
    for t in found:
        x = feats(t["height"], t["cells"], t["judge_p"], t["from_builder"], t["has_rulings"])
        # NaN is not JSON: a missing judge probability is `null`, as the dump
        # writes it, and the Rust test reads `null` back as NaN.
        clean = [None if isinstance(v, float) and math.isnan(v) else v for v in x]
        judge = None if isinstance(t["judge_p"], float) and math.isnan(t["judge_p"]) else t["judge_p"]
        out.append({"height": t["height"], "cells": t["cells"], "judge_p": judge,
                    "from_builder": t["from_builder"], "has_rulings": t["has_rulings"],
                    "features": clean})
    OUT.write_text(json.dumps({"names": NAMES, "tables": out}, allow_nan=False) + "\n")
    print(f"{OUT.relative_to(REPO)}: {len(out)} tables")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
