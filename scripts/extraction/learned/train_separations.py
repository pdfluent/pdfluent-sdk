#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Row and column boundaries as a decision per gap, and the grid they build (#574).

    train_separations.py --dump dump5.jsonl --trees 200 --max-depth 8

WHAT THIS REPLACES. The pair model of #600 asks "do these two words belong
together" and closes the grid transitively: one wrong pair walks through the
closure, and on PubTables it buys 0.0077 TEDS over the heuristic builder while
the ceiling -- our own words in the annotation's grid -- is 0.9404 against the
0.5996 we ship. The whole loss is the grid and the pair shape cannot reach it.

A separation is asked once per gap, a median of 11 per table, and a wrong
answer stays one wrong row. A rectangular grid built that way reaches 0.9085
of the 0.9404, so the merged header cells are worth 0.0319 and are a second
step rather than a reason not to take the first.

THE SPLIT IS BY DOCUMENT and it is the same split `train_on_our_boxes.py` and
`region_end_to_end.py` use, so every number here can be laid beside theirs.
The rig is never trained on: `learned_training_uses_no_rig_document.py` holds
that, and the manifest names every document by hand for it to check.

Dumps from another engine tree are refused, unless --engine-skew-is-fine says
why: the words of such a dump are not those of the engine that will read the
model (#574).
"""
from __future__ import annotations

import argparse
import gzip
import json
import pathlib
import random
import statistics
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import model_dir  # noqa: E402
import pubtables_alignment as pa  # noqa: E402
import pubtables_cells  # noqa: E402
import separation_features as sf  # noqa: E402
import structured_axes  # noqa: E402
import training_rules  # noqa: E402

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
SEED = 20260924
HEIGHT_SLACK = 1.5


def rows_and_columns(record, table, cells, uncovered):
    """Both axes' candidates for one table, each with its truth."""
    height = float(record["height"])
    box = pa.page_frame(table["pdf_table_bbox"], height)
    words = [w for w in (record.get("words") or [])
             if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
             and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
    if len(words) < 6:
        return [], []
    # THE BANDS ARE BUILT FROM EVERY WORD IN THE BOX, exactly as they are at
    # serve time, and only the LABEL comes from the annotation.
    #
    # The first version built them from the words `pa.place` could place and
    # decoded from all of them, so the model was shown one set of gaps and
    # asked about another. Per-gap F1 read 0.967 on rows and 0.916 on columns
    # and the grid it decoded scored TEDS 0.2013 against the 0.5996 we ship --
    # the numbers were of two different questions. Train and serve have to see
    # the same words, and the annotation may only decide the answer (#574).
    placed = pa.place([w["bbox"] for w in words], cells, height)
    cell_of = {id(w): cells[c] for w, c in zip(words, placed) if c is not None}
    if len(cell_of) < 6:
        return [], []
    text_height = statistics.median([w["bbox"][3] - w["bbox"][1] for w in words]) or 1.0
    word_width = statistics.median([w["bbox"][2] - w["bbox"][0] for w in words]) or 1.0

    def label(above, below, key):
        """A boundary where the grid changes across the gap, by the words the
        annotation covers. A band of only unplaced words says nothing, and the
        gap beside it is dropped rather than guessed at."""
        up = {n for w in above if id(w) in cell_of for n in cell_of[id(w)][key]}
        down = {n for w in below if id(w) in cell_of for n in cell_of[id(w)][key]}
        if not up or not down:
            return None
        return 1 if max(up) < min(down) else 0

    # AND A GAP THE ANNOTATION DOES NOT COVER STILL EXISTS AT SERVE TIME, so
    # dropping it here is a smaller version of the skew that was just fixed:
    # the model would never have been asked about a caption or a footnote
    # inside the table box, and at serve it is (COS, #574). `uncovered` counts
    # them so the size of that is a number rather than a worry, and
    # `--uncovered-is-no-line` labels them 0 instead of dropping them.
    rows, cols = [], []
    for above, below, x in sf.row_candidates(words, record.get("rules") or [], text_height):
        y = label(above, below, "row_nums")
        if y is None:
            uncovered[0] += 1
            if not uncovered[2]:
                continue
            y = 0
        rows.append((x, y))
        uncovered[1] += 1
    for left, right, x in sf.column_candidates(words, word_width):
        y = label(left, right, "column_nums")
        if y is None:
            uncovered[0] += 1
            if not uncovered[2]:
                continue
            y = 0
        cols.append((x, y))
        uncovered[1] += 1
    return rows, cols


# The three readings of a placed grid that #632 named, each measured out of
# fold on the training documents before it was kept (27/28-09, 3,538 tables,
# bridges at 0.03, from 0.8287): header rows +0.0037, section rows +0.0040,
# row groups in the stub column +0.0032. The fourth, folding a continuation
# row into the row above, LOST -- 0.0509 as a rule and 0.0039 even when held
# to rows without a digit -- and is not here: the row model already decides
# that gap, and a rule on top of it second-guesses the better half of its calls.
DRESSING = ("header", "section", "stub")

# More header rows than this and the "first full row" reading has found no
# body at all; the header is then the first row, as before.
MOST_HEADER_ROWS = 4

# Whether a lone header label spans the stub column as well (#574): False
# spans the figure columns 1..last (SPAN_TO_ALL, as FinTabNet writes it),
# True spans 0..last (SPAN_TO_ALL_WITH_STUB, as PubTables writes it). Each
# variant is measured against the current decoder on PubTables-300, FinTabNet,
# the proxy held-out half and ParseBench before one lands.
# `HEADER_SPAN_TAKES_STUB` in `separations.rs` is the other half of this
# switch; the two must agree or the parity test names the table.
HEADER_SPAN_TAKES_STUB = True

# The fewest figure columns a lone header label may span (#574): with a
# single figure column the label sits above the one column it names, so it
# keeps its bridge reach. `LONE_SPAN_MIN_COLUMNS` in `separations.rs` is the
# other half of this; the two must agree or the parity test names the table.
LONE_SPAN_MIN_COLUMNS = 2

# How far into the figure columns' extent a lone header label must sit (#574):
# its horizontal centre must lie within [left + BAND * w, right - BAND * w],
# or it names its own column ("| | | Amount") rather than heading the row.
# `LONE_SPAN_BAND` in `separations.rs` is the other half of this.
LONE_SPAN_BAND = 0.25


def bridging(words, word_width, bridges, bridge_floor, np):
    """The ids of the words the bridge model says span columns (#574)."""
    if bridges is None:
        return set()
    rows = sf.bridge_candidates(words, word_width)
    said = bridges.predict(np.asarray([x for _, x in rows], dtype=np.float32),
                           num_threads=1)
    return {id(w) for (w, _), p in zip(rows, said) if p >= bridge_floor}


def decode(record, table, rows_model, cols_model, floor, np,
           bridges=None, bridge_floor=0.5, dressing=DRESSING):
    """Predicted lines into a grid, and its HTML.

    A GAP AT A TIME AND NOTHING TRANSITIVE. Each band of words gets a row
    index by counting how many predicted row lines lie above it, and a column
    index the same way across. A wrong line therefore misplaces the bands on
    one side of it and nothing else -- which is the property the pair decoder
    does not have.

    THE STRIPES ARE BUILT WITHOUT THE WORDS THAT BRIDGE THEM. A header over
    two columns joins their stripes and so every body cell below it; the
    bridge model names those words first, the columns are found without them,
    and each is then given the columns its extent covers -- which is the
    merged header cell, as a span rather than a guess (#574).
    """
    height = float(record["height"])
    box = pa.page_frame(table["pdf_table_bbox"], height)
    words = [w for w in (record.get("words") or [])
             if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
             and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
    if len(words) < 4:
        return None
    text_height = statistics.median([w["bbox"][3] - w["bbox"][1] for w in words]) or 1.0
    word_width = statistics.median([w["bbox"][2] - w["bbox"][0] for w in words]) or 1.0
    row_of, col_of, spans = grid_positions(
        words, record.get("rules") or [], text_height, word_width,
        rows_model, cols_model, floor, np,
        bridging(words, word_width, bridges, bridge_floor, np))
    held: dict[tuple[int, int], list] = {}
    for w in words:
        held.setdefault((row_of[id(w)], col_of[id(w)]), []).append(w)
    if len(held) < 4:
        return None
    boxes = {key: (min(w["bbox"][0] for w in group),
                     max(w["bbox"][2] for w in group))
             for key, group in held.items()}
    header, rowspans, spans = dress(set(held), spans, dressing, boxes)
    cells = []
    for (r, c), group in sorted(held.items()):
        group.sort(key=lambda w: w["bbox"][0])
        cells.append({"row_nums": rowspans.get((r, c), [r]),
                      "column_nums": spans.get((r, c), [c]),
                      "pdf_text_content": " ".join(w["text"] for w in group),
                      "is_column_header": 1 if r in header else 0})
    return pubtables_cells.html_of({"cells": cells})[0]


def lone_header_span(here, columns, centre, extent, takes_stub=None):
    """The columns a lone header label spans, or None when the row is not one.

    `(first, last)` when the header row holds exactly one label outside the
    stub column, the table holds at least `LONE_SPAN_MIN_COLUMNS` figure
    columns, and the label's horizontal centre sits in the middle band of the
    figure columns' body extent (`extent`, the leftmost word in column 1 and
    the rightmost in the last column over the body rows). `lone_header_span`
    in `separations.rs` is the other half of this.
    """
    if takes_stub is None:
        takes_stub = HEADER_SPAN_TAKES_STUB
    if len(here) != 1 or here[0] == 0:
        return None
    if columns - 1 < LONE_SPAN_MIN_COLUMNS:
        return None
    if extent is None:
        return None
    left, right = extent
    w = right - left
    if not w >= 0.0:
        return None
    low = left + LONE_SPAN_BAND * w
    high = right - LONE_SPAN_BAND * w
    if centre < low or centre > high:
        return None
    return (0 if takes_stub else 1, columns - 1)


def dress(held, spans, dressing=DRESSING, boxes=None):
    """Header rows, section rows and stub row groups on a placed grid.

    `held` is the set of (row, first column) slots that hold words and
    `spans` the columns each bridged cell spans. `boxes` maps each held slot
    to the (left, right) extent of the words in it. Returns the header rows,
    the rows each cell spans, and the spans with section rows widened.

    HEADER ROWS are the rows above the first row that fills every column --
    the first row that looks like data -- when there are at most four of them.

    A SECTION ROW is a body row whose only cell is in the first column; it
    spans the table, as a label that divides the rows below it does.

    A ROW GROUP is a first-column cell with empty first-column slots under it
    in rows that hold data: it spans them, as a stub label set once for
    several rows does.

    A LONE HEADER LABEL is a header row whose only filled slot stands outside
    the stub column, centred over at least `LONE_SPAN_MIN_COLUMNS` figure
    columns; it spans the columns it heads, with the stub column among them
    when `HEADER_SPAN_TAKES_STUB` says so (#574). Without `boxes` there is no
    centre to judge, so nothing spans.
    """
    spans = dict(spans)
    columns = max(c for _, c in held) + 1
    rows = max(r for r, _ in held) + 1
    header = {0}
    if "header" in dressing:
        header = set()
        for r in range(rows):
            if r > 0 and sum(1 for c in range(columns) if (r, c) in held) == columns:
                break
            header.add(r)
        if len(header) > MOST_HEADER_ROWS:
            header = {0}
    taken = set(held)
    if "section" in dressing and columns >= 2:
        for r in range(rows):
            if r in header:
                continue
            here = [c for c in range(columns) if (r, c) in held]
            if here == [0]:
                spans[(r, 0)] = list(range(columns))
                taken |= {(r, k) for k in range(columns)}
    # A LONE HEADER LABEL SPANS THE COLUMNS IT HEADS (#574), when it is
    # centred over them. The bridge only reaches as far as the model names,
    # so one centred label over the figure columns came out colspan 3 against
    # a truth of 9; it spans the figures instead -- but a label over its own
    # last column ("| | | Amount") names that column and keeps its reach. A
    # row of two labels keeps each label's own bridge reach.
    # `lone_header_span` in `separations.rs` is the other half of this.
    extent = None
    if boxes is not None and columns >= 2:
        lefts = [boxes[(r, 1)][0] for r in range(rows)
                 if r not in header and (r, 1) in boxes]
        rights = [boxes[(r, columns - 1)][1] for r in range(rows)
                  if r not in header and (r, columns - 1) in boxes]
        if lefts and rights:
            extent = (min(lefts), max(rights))
    if "header" in dressing:
        for r in sorted(header):
            here = [c for c in range(columns) if (r, c) in held]
            if len(here) != 1:
                continue
            span = None
            if boxes is not None and (r, here[0]) in boxes:
                x0, x1 = boxes[(r, here[0])]
                span = lone_header_span(here, columns, (x0 + x1) / 2, extent)
            if span is not None:
                first, _ = span
                spans[(r, here[0])] = list(range(first, columns))
                taken |= {(r, k) for k in range(first, columns)}
    rowspans: dict[tuple[int, int], list[int]] = {}
    if "stub" in dressing:
        for r in range(rows):
            if r in header or (r, 0) not in held or len(spans.get((r, 0), [0])) > 1:
                continue
            last = r
            while (last + 1 < rows and last + 1 not in header
                   and (last + 1, 0) not in taken
                   and any((last + 1, k) in held for k in range(1, columns))):
                last += 1
            if last > r:
                taken |= {(q, 0) for q in range(r + 1, last + 1)}
                rowspans[(r, 0)] = list(range(r, last + 1))
    return header, rowspans, spans


def grid_positions(words, rules, text_height, word_width, rows_model, cols_model,
                   floor, np, bridge):
    """Row and column per word, and the columns a bridged cell spans.

    `bridge` is the set of word ids left out of the stripes. With it empty
    this is exactly the decoder the boundary models were measured with.
    """
    lines = sf.bands(words, lambda w: w["bbox"][1], lambda w: w["bbox"][3],
                     lambda w: -w["bbox"][3])
    core = [w for w in words if id(w) not in bridge]
    # A table cannot be all bridge: with fewer than two words left there are
    # no columns to span, and the stripes are built from every word as before.
    if len(core) < 2:
        core, bridge = words, set()
    stripes = sf.bands(core, lambda w: w["bbox"][0], lambda w: w["bbox"][2],
                       lambda w: w["bbox"][0])
    row_x = [x for _, _, x in sf.row_candidates(words, rules, text_height)]
    col_x = [x for _, _, x in sf.column_candidates(core, word_width)]
    row_cut = (rows_model.predict(np.asarray(row_x, dtype=np.float32), num_threads=1)
               if row_x else [])
    col_cut = (cols_model.predict(np.asarray(col_x, dtype=np.float32), num_threads=1)
               if col_x else [])

    row_of, at = {}, 0
    for i, band in enumerate(lines):
        for w in band:
            row_of[id(w)] = at
        if i < len(row_cut) and row_cut[i] >= floor:
            at += 1
    col_of, at = {}, 0
    extent = []
    for i, band in enumerate(stripes):
        for w in band:
            col_of[id(w)] = at
        extent.append((min(w["bbox"][0] for w in band),
                       max(w["bbox"][2] for w in band), at))
        if i < len(col_cut) and col_cut[i] >= floor:
            at += 1

    # Each bridging word takes the columns whose stripes it overlaps, or the
    # nearest one when it overlaps none; its cell starts at the first.
    reach: dict[int, tuple[int, int]] = {}
    for w in words:
        if id(w) not in bridge:
            continue
        hit = [c for x0, x1, c in extent if w["bbox"][0] < x1 and w["bbox"][2] > x0]
        if not hit:
            middle = (w["bbox"][0] + w["bbox"][2]) / 2
            hit = [min(extent, key=lambda e: abs((e[0] + e[1]) / 2 - middle))[2]]
        col_of[id(w)] = min(hit)
        reach[id(w)] = (min(hit), max(hit))
    filled = {(row_of[id(w)], col_of[id(w)]) for w in words}
    wanted: dict[tuple[int, int], int] = {}
    for w in words:
        if id(w) in reach:
            key = (row_of[id(w)], col_of[id(w)])
            wanted[key] = max(wanted.get(key, key[1]), reach[id(w)][1])
    # A span stops at the first column its row already fills: two cells never
    # claim one slot.
    spans: dict[tuple[int, int], list[int]] = {}
    for (r, c), last in wanted.items():
        end = c
        while end < last and (r, end + 1) not in filled:
            end += 1
        spans[(r, c)] = list(range(c, end + 1))
    return row_of, col_of, spans


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--grids", type=pathlib.Path, required=True,
                    help="the compact per-page cell grids from "
                         "pubtables_grids.py; the tar held in memory is what "
                         "had three runs killed")
    ap.add_argument("--trees", type=int, default=200)
    ap.add_argument("--leaves", type=int, default=31)
    ap.add_argument("--max-depth", type=int, default=8,
                    help="the speed budget is trees x depth and LightGBM grows "
                         "leaf-wise, so depth is what has to be capped")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--decision-floor", type=float, default=0.1,
                    help="the probability at which a gap becomes a line, as "
                         "`tables::SEPARATION_FLOOR` ships it; recorded in the "
                         "manifest because the export guard re-exports from it")
    ap.add_argument("--save-rows", default=None)
    ap.add_argument("--save-columns", default=None)
    ap.add_argument("--uncovered-is-no-line", action="store_true",
                    help="label a gap the annotation does not cover as NO "
                         "boundary rather than dropping it, because at serve "
                         "time it exists and must be answered")
    ap.add_argument("--floor", type=float, default=0.5,
                    help="the probability at which a gap becomes a line")
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    args = ap.parse_args()

    import lightgbm as lgb
    import numpy as np

    manifest = json.loads((STORE / "pdf" / "MANIFEST.json").read_text())
    model_dir.require_training_source(manifest, str(STORE / "pdf" / "MANIFEST.json"))
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    documents = sorted({p["document"] for p in manifest["pages"]})
    random.Random(SEED).shuffle(documents)
    cut = int(len(documents) * 0.75)
    train_docs, test_docs = set(documents[:cut]), set(documents[cut:])
    print(f"documents {len(documents)}: {len(train_docs)} train, {len(test_docs)} test",
          flush=True)
    grids = {}
    with gzip.open(args.grids, "rt") as handle:
        for line in handle:
            row = json.loads(line)
            grids[row["sha256"]] = row
    print(f"grids for {len(grids)} page(s)", flush=True)

    train_rows, train_cols, test_rows, test_cols = [], [], [], []
    # [gaps the annotation does not cover, gaps kept, whether to label them 0]
    uncovered = [0, 0, args.uncovered_is_no_line]
    tables = 0
    for line in open(args.dump):
        record = json.loads(line)
        # The words are one tree's segmentation: a grid quoted from another
        # engine's words is about that engine, not this one (#574).
        training_rules.same_engine(record, str(args.dump), args.engine_skew_is_fine)
        page = by_sha.get(record["pdf_sha256"])
        if not page:
            continue
        training = page["document"] in train_docs
        height = float(record["height"])
        for table in (grids.get(record["pdf_sha256"], {}).get("tables") or []):
            if abs(height - float(table["page_height"])) > HEIGHT_SLACK:
                continue
            cells = [{"row_nums": c["r"], "column_nums": c["c"],
                      "pdf_bbox": c["b"], "pdf_text_tight_bbox": c["t"],
                      "pdf_text_content": c["x"],
                      "is_column_header": c["h"]} for c in table["cells"]]
            if len(cells) < 4:
                continue
            table = {"pdf_table_bbox": table["box"]}
            rows, cols = rows_and_columns(record, table, cells, uncovered)
            if not rows and not cols:
                continue
            tables += 1
            (train_rows if training else test_rows).extend(rows)
            (train_cols if training else test_cols).extend(cols)
    share = 100 * uncovered[0] / max(1, uncovered[0] + uncovered[1])
    print(f"gaps the annotation does not cover: {uncovered[0]} of "
          f"{uncovered[0] + uncovered[1]} ({share:.1f}%), "
          f"{'labelled 0' if uncovered[2] else 'dropped'}")
    print(f"{tables} table(s); row gaps {len(train_rows)} train / {len(test_rows)} test; "
          f"column gaps {len(train_cols)} train / {len(test_cols)} test", flush=True)

    params = {
        "objective": "binary", "num_leaves": args.leaves, "max_depth": args.max_depth,
        "learning_rate": 0.1, "min_data_in_leaf": 100, "seed": SEED,
        "deterministic": True, "force_row_wise": True,
        "num_threads": args.threads, "verbosity": -1,
    }

    def fit(name, rows, names, save):
        x = np.asarray([r[0] for r in rows], dtype=np.float32)
        y = np.asarray([r[1] for r in rows], dtype=np.int32)
        cut = int(len(x) * 0.85)
        booster = lgb.train(
            params, lgb.Dataset(x[:cut], label=y[:cut], feature_name=names),
            num_boost_round=args.trees,
            valid_sets=[lgb.Dataset(x[cut:], label=y[cut:])],
            callbacks=[lgb.early_stopping(25, verbose=False)])
        trees = booster.best_iteration or args.trees
        print(f"  {name}: stopped at {trees} of {args.trees}, "
              f"{100 * y.mean():.1f}% positive", flush=True)
        if save:
            booster.save_model(save)
        return booster, trees

    print("\ntraining:", flush=True)
    rows_model, rows_trees = fit("rows", train_rows, sf.ROW_NAMES, args.save_rows)
    cols_model, cols_trees = fit("columns", train_cols, sf.COLUMN_NAMES, args.save_columns)

    def report(name, model, rows):
        x = np.asarray([r[0] for r in rows], dtype=np.float32)
        y = np.asarray([r[1] for r in rows], dtype=np.int32)
        said = model.predict(x, num_threads=1)
        print(f"\n{name} on the held-out documents ({len(y)} gaps, "
              f"{100 * y.mean():.1f}% positive):")
        print(f"  {'threshold':>10s}{'precision':>11s}{'recall':>9s}{'F1':>8s}"
              f"{'accuracy':>10s}")
        for t in (0.3, 0.4, 0.5, 0.6, 0.7):
            call = said >= t
            hit = int((call & (y == 1)).sum())
            precision = hit / max(1, int(call.sum()))
            recall = hit / max(1, int((y == 1).sum()))
            f1 = 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)
            accuracy = float(((call.astype(int)) == y).mean())
            print(f"  {t:10.1f}{precision:11.3f}{recall:9.3f}{f1:8.3f}{accuracy:10.3f}")
        gain = sorted(zip(model.feature_name(), model.feature_importance("gain")),
                      key=lambda p: -p[1])
        total = sum(g for _, g in gain) or 1.0
        print("  what it reads:", ", ".join(f"{n} {100 * g / total:.0f}%"
                                            for n, g in gain[:5]))

    report("ROW boundaries", rows_model, test_rows)
    report("COLUMN boundaries", cols_model, test_cols)

    # WHAT THE GRID IS WORTH lives in `separations_teds.py`, not here.
    #
    # This file used to carry a `--teds` pass and it referenced `annotations`,
    # a name that stopped existing when the compact grid cache replaced the tar
    # load: every use of the flag raised NameError. It was never reached in the
    # runs that produced the numbers on this branch -- the scoring was always
    # the other script -- but a flag that cannot run is worse than no flag,
    # because the next person reads it as the way to check the grid
    # (codex, #574).
    #
    # It is gone rather than repaired: the scorer next door holds the ceiling
    # check that refuses to report through a broken truth, and two spellings of
    # "what is this grid worth" is the defect this lane keeps paying for.

    if args.model_dir:
        for name, booster, rows, names, trees in (
                ("rows", rows_model, train_rows, sf.ROW_NAMES, rows_trees),
                ("columns", cols_model, train_cols, sf.COLUMN_NAMES, cols_trees)):
            x = np.asarray([r[0] for r in rows], dtype=np.float32)
            manifest = {
                "task": f"table_{name[:-1]}_boundary",
                "schema": "pdfluent.separations/1",
                "seed": SEED,
                "corpus": "pubtables-1m over pmc open access",
                "documents": [{"document": d, "source": "pubtables-1m/structure",
                               "split": "train"} for d in sorted(train_docs)]
                             + [{"document": d, "source": "pubtables-1m/structure",
                                 "split": "test"} for d in sorted(test_docs)],
                "rows": {"train": len(rows)},
                # THE FLOOR THE ENGINE SHIPS, which the manifest has to name:
                # `test_learned_export_is_reproducible.py` re-exports a
                # single-class model from this value and compares the generated
                # Rust with what is committed, so a manifest without it cannot
                # be checked at all. It is `tables::SEPARATION_FLOOR`, chosen on
                # the training half (0.7981 / 0.7998 / 0.7940 at 0.05 / 0.10 /
                # 0.25) and not on the half the figure is quoted from.
                "threshold": args.decision_floor,
                "trees": int(booster.num_trees()),
                "params": params,
                "rust": f"crates/pdf-engine/src/learned/models/table_{name[:-1]}_boundary",
            }
            skew = training_rules.engine_skew_manifest()
            if skew is not None:
                manifest["engine_skew"] = skew
            model_dir.write(f"{args.model_dir}-{name}", booster, x, names, manifest)
            print(f"  wrote {args.model_dir}-{name}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
