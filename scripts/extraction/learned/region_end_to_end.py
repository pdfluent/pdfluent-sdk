#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The region model and the structure model together, scored as a reader sees
them (#600).

THE ONLY NUMBER THAT SETTLES THIS. A region model with a good per-block F1 may
still hand the structure model a worse region than our heuristics do: blocks
are not tables, and a region that is right about 91 per cent of its blocks can
be wrong about every table if it is wrong at the edges. So the two models are
run in series and TEDS is taken over EVERY annotated table, with the ones no
region reaches scoring the zero they earn.

Three regions are scored side by side, on one table set with one split:

  the annotation's own table box   the upper bound, which the engine never has
  our heuristics' table cells      what ships today
  the region model                 this

TRAINED ON DOCLAYNET, MEASURED HERE ON PUBTABLES. Different corpora on purpose
(T2, #600): DocLayNet labels every block so its negatives mean something, and
a model measured on the corpus it was not trained on cannot be flattered by a
house style it has memorised.

WHY PYTHON MAY COMPUTE THE CANDIDATES HERE. Normally it may not -- the rule is
one computation for the dump and for the engine. It is allowed for this
measurement because `candidates` is now held to EQUAL its Rust counterpart,
pair for pair, over 6,331 pairs of six real regions
(`the_pairs_we_ask_about_are_the_pairs_python_asks_about`). The Python answer
IS the Rust answer, which is exactly what that test was for. If that test ever
goes red this file is measuring something the engine does not do.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import random
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import grid_decoder as gd
import pair_features as pf
import pubtables_alignment as pa
import pubtables_cells
import structured_axes
import table_region as tr

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
SEED = 20260924
HEIGHT_SLACK = 1.5


def runs_of(blocks, inside, gap_lines: float):
    """Connected runs of blocks the model calls inside a table.

    A REGION IS A RUN, NOT A CLUSTER. The blocks arrive in reading order, so a
    table is a consecutive stretch of them; a run breaks where the model says
    "not inside" or where the vertical gap to the next block is larger than
    `gap_lines` line heights. T2 measured that gap on our own splits: joins
    run to a median of 3.1 em and boundaries to 28.5, but the distributions
    overlap COMPLETELY -- 62 of 63 joins and 21 of 33 boundaries share a range
    -- so a gap alone tops out at 92 per cent and cannot be the whole answer.
    It is here as the tie-break after the model has spoken, not instead of it.
    """
    out, current = [], []
    for i, b in enumerate(blocks):
        if not inside[i]:
            if current:
                out.append(current)
                current = []
            continue
        if current:
            before = blocks[current[-1]]
            line = max(1.0, float(before.get("size") or 10.0))
            # AND A RUN BREAKS WHERE READING ORDER GOES BACK UP, the same way
            # `tables::runs` does. On a two-column page the order can finish a
            # table at the foot of the left column and continue at the head of
            # the right one, and then the subtraction below is NEGATIVE and
            # never exceeds the gap: the two tables become one run. Rows of one
            # table descend, so a block whose top is more than a line above the
            # previous block's top is not the next row of it (codex, #600).
            jumped_up = b["bbox"][3] - before["bbox"][3] > line
            if jumped_up or before["bbox"][1] - b["bbox"][3] > gap_lines * line:
                out.append(current)
                current = []
        current.append(i)
    if current:
        out.append(current)
    return out


def may_overrule(doubt, args) -> bool:
    """`tables::may_overrule`, in Python, over the dump's `grid_*` fields.

    ONE RULE IN TWO PLACES, which is the thing this file otherwise refuses --
    but the engine's copy is the one that ships and this one only has to agree
    with it while the thresholds are being fitted. When they are fitted the
    numbers go into the Rust and this reads them back from a run rather than
    keeping its own.
    """
    if doubt is None:
        return False
    return (doubt["fill"] < args.gate_fill
            and doubt["full_rows"] <= args.gate_full_rows
            and doubt["thin_rows"] >= args.gate_thin_rows)


def doubt_of(blocks, indexes):
    """The first cell's record, as `tables::weakness_of` takes it."""
    wanted = set(indexes)
    for block in blocks:
        if block.get("i") in wanted and block.get("grid_fill") is not None:
            return {"fill": block["grid_fill"],
                    "full_rows": block["grid_full_rows"],
                    "thin_rows": block["grid_thin_rows"],
                    "retried_from": block["grid_retried_from"],
                    "columns": block["grid_columns"]}
    return None


def teds_of(words, cells, floor, booster, np):
    """One table's TEDS, from words to a grid to HTML."""
    pairs = pf.candidates(words)
    if not pairs:
        return None
    block = np.asarray([pf.features(words[i], words[j]) for i, j in pairs],
                       dtype=np.float32)
    said = booster.predict(block, num_threads=1)
    scored = {p: said[n] for n, p in enumerate(pairs)}
    out = gd.fill_empty(gd.from_edges(words, scored, row_span=lambda i: 1,
                                      column_span=lambda i: 1, floor=floor))
    return pubtables_cells.html_of({"cells": [
        {"row_nums": c["row_nums"], "column_nums": c["column_nums"],
         "pdf_text_content": c["text"], "is_column_header": 0 in c["row_nums"]}
        for c in out]})[0]


def perfect_grid_html(words, cells, placed):
    """OUR words, in the ANNOTATION's grid: the ceiling a better grid can reach.

    THE POINT IS TO SPLIT THE ERROR IN TWO. Every other column here decides
    both which words belong to a cell AND how the cells are arranged; this one
    is handed the arrangement and keeps our words and our reading of them. What
    it scores is therefore everything that is NOT the grid -- words we never
    extracted, words the annotation does not cover, the order we read a cell
    in -- and the distance from it to `region model, paired` is what a perfect
    grid would buy (COS, #574).
    """
    held: dict[int, list[str]] = {}
    for word, cell in zip(words, placed):
        if cell is None:
            continue
        held.setdefault(cell, []).append(word["text"])
    if len(held) < 4:
        return None
    out = []
    for at, cell in enumerate(cells):
        if at not in held:
            continue
        rows = cell["row_nums"]
        out.append({"row_nums": rows, "column_nums": cell["column_nums"],
                    "pdf_text_content": " ".join(held[at]),
                    "is_column_header": 1 if cell.get("is_column_header") else 0})
    return pubtables_cells.html_of({"cells": out})[0]


def rectangular_grid_html(words, cells, placed):
    """The same, with every merge flattened: what a grid without spans can reach.

    A separation model predicts a row line and a column line, so what it can
    build is a RECTANGLE -- every cell one row by one column. The annotation's
    merged header cells cannot be expressed in that, and this is what losing
    them costs, measured rather than assumed (COS, #574). Each merged cell is
    charged to its first row and column and the rest of its span is left
    empty, which is what a rectangular decoder would produce.
    """
    held: dict[tuple[int, int], list[str]] = {}
    header: set[tuple[int, int]] = set()
    for word, cell in zip(words, placed):
        if cell is None:
            continue
        spot = (cells[cell]["row_nums"][0], cells[cell]["column_nums"][0])
        held.setdefault(spot, []).append(word["text"])
        if cells[cell].get("is_column_header"):
            header.add(spot)
    if len(held) < 4:
        return None
    out = [{"row_nums": [r], "column_nums": [c], "pdf_text_content": " ".join(t),
            "is_column_header": 1 if (r, c) in header else 0}
           for (r, c), t in sorted(held.items())]
    return pubtables_cells.html_of({"cells": out})[0]


# WHAT AN ANNOTATION IS USED FOR HERE, and nothing else. The whole test split
# held as parsed JSON is hundreds of megabytes, and on a Mac several sessions
# share this run was killed for memory twice. Keeping the five fields that are
# read makes it fit; it changes no number, because the rest was never touched.
_CELL_KEYS = ("row_nums", "column_nums", "pdf_bbox", "pdf_text_tight_bbox",
              "is_column_header", "pdf_text_content")
_TABLE_KEYS = ("pdf_page_index", "pdf_table_bbox", "pdf_full_page_bbox",
               "exclude_for_structure")


def _lean(table: dict) -> dict:
    out = {k: table[k] for k in _TABLE_KEYS if k in table}
    out["cells"] = [{k: c[k] for k in _CELL_KEYS if k in c}
                    for c in (table.get("cells") or [])]
    return out


def our_grid_html(blocks, chosen_blocks):
    """The HTML of the grid THE ENGINE ACTUALLY BUILT, from its own cells.

    WITHOUT THE PAIR MODEL, which is the whole point of this column. Every
    other row here goes through `teds_of`, and `teds_of` runs the learned pair
    model to decide the grid -- so a row labelled "our table cells" was the
    heuristics' CELL SELECTION scored through the LEARNED decoder, not what the
    engine ships. Fitting the gate against it compared one learned grid with
    another and called the loser "the heuristics" (#600).

    `tables::find` already wrote `row`, `column`, `row_span`, `column_span` and
    `table` onto each cell block, and the dump carries them, so the shipped
    grid can be read straight off rather than rebuilt.
    """
    cells = []
    for i in sorted(chosen_blocks):
        b = blocks[i]
        if b.get("kind") != "table_cell" or b.get("row") is None:
            continue
        row, column = int(b["row"]), int(b["column"])
        rows = list(range(row, row + max(1, int(b.get("row_span") or 1))))
        cols = list(range(column, column + max(1, int(b.get("column_span") or 1))))
        cells.append({"row_nums": rows, "column_nums": cols,
                      "pdf_text_content": b.get("text") or "",
                      "is_column_header": 0 in rows})
    if len(cells) < 4:
        return None
    return pubtables_cells.html_of({"cells": cells})[0]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", required=True, help="the PubTables feature dump")
    ap.add_argument("--region-model", required=True)
    ap.add_argument("--structure-model", required=True)
    ap.add_argument("--threshold", type=float, default=0.4)
    ap.add_argument("--gap-lines", type=float, default=3.0)
    ap.add_argument("--floor", type=float, default=0.9)
    # THE HYBRID GATE, in the same three numbers as `tables::may_overrule`.
    # The engine decides per table whether the model may overrule the builder;
    # a harness that rebuilt every table would measure a configuration that
    # does not ship (#600).
    ap.add_argument("--gate-fill", type=float, default=0.6)
    ap.add_argument("--gate-full-rows", type=int, default=1)
    # 0 AND NOT 1, because `tables::may_overrule` asks two questions and not
    # three. The default stayed at 1 after the thin-row clause was dropped from
    # the Rust, so every run of this harness measured a gate the engine does
    # not have: it opened on 34 tables where the shipped rule opens on 91, and
    # the subset it picked was one the model loses on. One rule in two places
    # is only safe while the two agree (#600).
    ap.add_argument("--gate-thin-rows", type=int, default=0)
    ap.add_argument("--no-gate", action="store_true",
                    help="rebuild every table, as before the gate existed")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--facts-out", default=None,
                    help="one row per table: the builder's five doubt facts, "
                         "what the engine scored and what the model scored. "
                         "Sweeping a gate width is then arithmetic on this "
                         "file rather than another pass over the corpus.")
    args = ap.parse_args()

    import numpy as np
    import lightgbm as lgb

    region = lgb.Booster(model_file=args.region_model)
    structure = lgb.Booster(model_file=args.structure_model)

    manifest = json.loads((STORE / "pdf" / "MANIFEST.json").read_text())
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    wanted = {p["document"] for p in manifest["pages"]}
    documents = sorted(wanted)
    random.Random(SEED).shuffle(documents)
    # The SAME split as train_on_our_boxes, so the numbers sit beside its.
    test = set(documents[int(len(documents) * 0.75):])

    annotations = {}
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
                annotations[pmc] = [_lean(t) for t in json.load(handle)]
            except Exception:
                pass

    scores = {"annotation box": [], "the annotation's grid, our words": [],
              "a rectangular grid, our words": [],
              "our grid, as it ships": [],
              "our table cells": [], "region model": [],
              "region model, paired": [], "region, heuristic cells": [],
              "the hybrid": []}
    # THE GATED SUBSET, scored on its own. The gate is meant to open rarely, so
    # its effect is invisible in an average over every table: on PubTables it
    # opens on well under a tenth of them and the aggregate moves by a
    # thousandth whichever way the model does on the ones it touched. What
    # decides the thresholds is how the model does WHERE IT SPEAKS.
    opened = {"shipped": [], "heuristics": [], "model": []}
    facts_file = open(args.facts_out, "w") if args.facts_out else None
    if facts_file:
        facts_file.write("fill\tfull_rows\tthin_rows\tcolumns\tretried_from"
                         "\tshipped\trebuilt\n")
    missed = {k: 0 for k in scores}
    tables = 0
    spare_regions = 0
    for line in open(args.dump):
        if args.limit and tables >= args.limit:
            break
        record = json.loads(line)
        page = by_sha.get(record["pdf_sha256"])
        if not page or page["document"] not in test:
            continue
        height = float(record["height"])
        blocks = record["blocks"]
        words = record["words"]
        info = {"body": record.get("body_size"), "width": record["width"],
                "height": height}

        # What the region model says about every block of this page.
        rows = np.asarray([tr.vector(b, info, i, blocks)
                           for i, b in enumerate(blocks)], dtype=np.float32)
        said = region.predict(rows, num_threads=1) if len(rows) else []
        inside = [p >= args.threshold for p in said]
        model_regions = runs_of(blocks, inside, args.gap_lines)
        ours = [i for i, b in enumerate(blocks) if b["kind"] == "table_cell"]

        def words_of(block_indexes):
            keep = set(block_indexes)
            return [k for k, w in enumerate(words) if w.get("block") in keep]

        # PAIR THE REGIONS TO THE TABLES ONCE, BEFORE SCORING ANY OF THEM.
        # Letting every table pick its best region flatters the model twice
        # over: one region can serve two tables, and a region that matches
        # nothing costs nothing. Here each region is spent on at most one
        # table, best overlap first, which is the question a reader asks --
        # did we draw THIS table -- rather than "was this table inside
        # something we drew".
        on_page = [t for t in (annotations.get(page["document"]) or [])
                   if int(t.get("pdf_page_index", -1)) == int(page["page_no"])
                   and not t.get("exclude_for_structure")
                   and abs(height - float(t["pdf_full_page_bbox"][3])) <= HEIGHT_SLACK]
        boxes = {}
        for n, t in enumerate(on_page):
            box = pa.page_frame(t["pdf_table_bbox"], height)
            boxes[n] = {k for k, w in enumerate(words)
                        if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                        and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]}
        overlaps = []
        for r, run in enumerate(model_regions):
            got = set(words_of(run))
            for n, want in boxes.items():
                if want and got & want:
                    overlaps.append((len(got & want) / len(want), r, n))
        overlaps.sort(reverse=True)
        taken_region, taken_table, paired = set(), set(), {}
        for _share, r, n in overlaps:
            if r in taken_region or n in taken_table:
                continue
            taken_region.add(r)
            taken_table.add(n)
            paired[n] = words_of(model_regions[r])
        spare_regions += len(model_regions) - len(taken_region)

        for n, t in enumerate(on_page):
            cells = t["cells"]
            truth = pubtables_cells.html_of(t)[0]
            if not truth:
                continue
            in_box = sorted(boxes[n])
            if len(in_box) < 4:
                continue
            tables += 1

            # THE REGION MODEL'S BEST REGION FOR THIS TABLE is the run whose
            # words overlap the annotation most. Picking the best one is
            # generous, and deliberately so: this measures whether the region
            # CONTAINS the table, not whether we can pair regions to tables,
            # which is a separate question and not this model's.
            best = []
            for run in model_regions:
                got = words_of(run)
                share = len(set(got) & set(in_box)) / max(1, len(in_box))
                if share > (len(set(best) & set(in_box)) / max(1, len(in_box))):
                    best = got

            # THE REGION WITHOUT A GRID BUILDER. The heuristics' own cells,
            # restricted to the blocks the region model claims. This is what
            # "region model on, structure model off" can be at run time: the
            # engine's table builder works on LINES during block building
            # (`tables::find`) and the learned pass sees finished BLOCKS, so
            # the builder cannot be re-run on a region the model found. All a
            # region can do without a structure model is keep or drop cells
            # the heuristics already made -- and the region's whole value is
            # the blocks they did NOT make into cells.
            region_cells = [k for k in (paired.get(n) or [])
                            if k in set(words_of(ours))]
            gate_open = args.no_gate or may_overrule(doubt_of(blocks, ours), args)

            # WHAT THE ENGINE SHIPS WHEN THE GATE STAYS SHUT, scored without
            # the pair model. This is the arm the gate has to beat, and the
            # first fit never measured it (#600).
            shipped = our_grid_html(blocks, [i for i in ours
                                             if any(w.get("block") == i
                                                    and k in boxes[n]
                                                    for k, w in enumerate(words))])
            shipped_teds = structured_axes.teds(truth, shipped) if shipped else 0.0
            scores["our grid, as it ships"].append(shipped_teds)
            if not shipped:
                missed["our grid, as it ships"] += 1
            if gate_open:
                opened["shipped"].append(shipped_teds)

            # THE CEILING: our words in the annotation's own grid.
            ceiling = 0.0
            picked_all = [w for k, w in enumerate(words) if k in set(in_box)]
            if len(picked_all) >= 4:
                spots = pa.place([w["bbox"] for w in picked_all], cells, height)
                html = perfect_grid_html(picked_all, cells, spots)
                ceiling = structured_axes.teds(truth, html) if html else 0.0
            scores["the annotation's grid, our words"].append(ceiling)
            if not ceiling:
                missed["the annotation's grid, our words"] += 1

            # AND THE SAME GRID WITH THE MERGES FLATTENED: the ceiling a
            # separation model can reach, since row and column lines make
            # rectangles and nothing else.
            rect = 0.0
            if len(picked_all) >= 4:
                html = rectangular_grid_html(picked_all, cells, spots)
                rect = structured_axes.teds(truth, html) if html else 0.0
            scores["a rectangular grid, our words"].append(rect)
            if not rect:
                missed["a rectangular grid, our words"] += 1

            # THE MODEL'S SCORE FOR THIS TABLE WHATEVER THE GATE SAYS, so a
            # width can be swept offline instead of by another pass.
            rebuilt = 0.0
            for name, chosen in (("annotation box", in_box),
                                 ("our table cells", words_of(ours)),
                                 ("region model", best),
                                 ("region model, paired", paired.get(n, [])),
                                 ("region, heuristic cells", region_cells),
                                 # THE HYBRID, which is what ships: the model
                                 # rebuilds a table only where the builder's
                                 # own record says it was short of evidence,
                                 # and the heuristics keep it everywhere else.
                                 # `the hybrid` here is only its OPEN half;
                                 # when the gate is shut the engine keeps the
                                 # shipped grid, which does not go through
                                 # `teds_of` at all and is added below.
                                 ("the hybrid",
                                  paired.get(n, []) if gate_open
                                  else words_of(ours))):
                picked = [w for k, w in enumerate(words) if k in set(chosen)]
                if len(picked) < 4:
                    missed[name] += 1
                    scores[name].append(0.0)
                    continue
                placed = pa.place([w["bbox"] for w in picked], cells, height)
                keep = [i for i, p in enumerate(placed) if p is not None]
                if len(keep) < 4:
                    missed[name] += 1
                    scores[name].append(0.0)
                    continue
                html = teds_of([picked[i] for i in keep], cells, args.floor,
                               structure, np)
                got = structured_axes.teds(truth, html) if html else 0.0
                scores[name].append(got)
                if name == "region model, paired":
                    rebuilt = got
                if gate_open and name == "our table cells":
                    opened["heuristics"].append(got)
                if gate_open and name == "the hybrid":
                    opened["model"].append(got)


            # ONE ROW PER TABLE, INCLUDING THE ONES NOTHING REACHED. Written
            # inside the loop over columns it was only reached when the hybrid
            # column did not bail out, so the file held 983 of 1,152 tables and
            # left out exactly the failures: "the engine everywhere" read
            # 0.6984 on the subset against 0.5996 over all of them, and every
            # width in the sweep was flattered by the same amount (#600).
            if facts_file is not None:
                # A TABLE WITH NO RECORD IS A ROW TOO, with NaN for the facts:
                # the gate cannot open on it, so it belongs on the shipped side
                # of every width, and dropping it would shrink the denominator
                # differently for each one.
                d = doubt_of(blocks, ours) or {}
                facts_file.write("\t".join(str(v) for v in (
                    f"{d.get('fill', float('nan')):.4f}",
                    d.get("full_rows", -1), d.get("thin_rows", -1),
                    d.get("columns", -1), d.get("retried_from", -1),
                    f"{shipped_teds:.4f}", f"{rebuilt:.4f}")) + "\n")

            # AND THE SHUT HALF OF THE HYBRID IS THE SHIPPED GRID. The loop
            # above scored it through `teds_of`, which is the learned decoder
            # on the heuristics' cells -- a third thing that is neither arm.
            if not gate_open:
                if scores["the hybrid"][-1] == 0.0 and missed["the hybrid"]:
                    missed["the hybrid"] -= 1
                scores["the hybrid"][-1] = shipped_teds
                if not shipped:
                    missed["the hybrid"] += 1

    print(f"\ntables scored: {tables} (threshold {args.threshold}, "
          f"gap {args.gap_lines} lines, floor {args.floor})")
    for name in ("annotation box", "the annotation's grid, our words",
                 "a rectangular grid, our words",
                 "our grid, as it ships", "our table cells",
                 "region model",
                 "region model, paired", "region, heuristic cells",
                 "the hybrid"):
        got = scores[name]
        print(f"  {name:<22} TEDS {sum(got)/max(1,len(got)):.4f}   "
              f"reached none of it on {missed[name]}")
    if opened["model"]:
        mean = lambda v: sum(v) / max(1, len(v))
        print(f"\n  WHERE THE GATE OPENED ({len(opened['model'])} tables, "
              f"{100*len(opened['model'])/max(1,tables):.1f}% of them):")
        # THREE ARMS, AND ONLY THE FIRST IS WHAT SHIPS. `the heuristics,
        # learned grid` is the heuristics' CELL SELECTION run through the pair
        # model, which is what the first fit mistook for the heuristics; it is
        # kept because the difference between it and the row above is the
        # decoder's own contribution, and that is worth seeing.
        print(f"    the engine keeps it      TEDS {mean(opened['shipped']):.4f}"
              "   <- what the gate must beat")
        print(f"    the model rebuilds it    TEDS {mean(opened['model']):.4f}")
        print(f"    (its cells, learned grid TEDS {mean(opened['heuristics']):.4f})")
    else:
        print("\n  the gate never opened on this set")
    print(f"  regions the model drew that no annotated table claimed: "
          f"{spare_regions}")
    # NOT A FALSE-POSITIVE COUNT, and it must not be reported as one.
    # PubTables does not annotate every table on a page, so a region no
    # annotation claims is often a real table the corpus is silent about --
    # T2 measured 55 of 72 apparent false positives here to be exactly that
    # (#600). The number is printed because it bounds the question, not
    # because it answers it; the corpus that can answer it is DocLayNet.
    print("  (that count is NOT false positives: PubTables leaves real tables "
          "unannotated, so most of those are unlabelled rather than wrong. "
          "Precision belongs on DocLayNet, where every block is labelled.)")
    print("  (`region model` lets every table pick its best region, which one "
          "region may serve twice; `paired` spends each region once.)")
    if facts_file:
        facts_file.close()
        print(f"  per-table facts written to {args.facts_out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
