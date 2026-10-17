#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the separation grid is worth, scored as TEDS (#574).

    separations_teds.py --dump dump5.jsonl --rows sep-rows.txt --columns sep-cols.txt

APART FROM THE TRAINER ON PURPOSE, and not only for tidiness: the trainer
holds every training gap and the annotations of all 2,001 documents at once,
and on a Mac several sessions share that was killed for memory. This reads the
saved models and the 501 HELD-OUT documents, which fits.

IT IMPORTS `decode` RATHER THAN REIMPLEMENTING IT. Two spellings of how lines
become a grid would agree until they did not, and that is the defect this pair
of tickets has paid for seven times.
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
import pubtables_cells  # noqa: E402
import structured_axes  # noqa: E402
import pubtables_alignment as pa  # noqa: E402
import train_separations as ts  # noqa: E402

# What our own words in the annotation's grid score, flattened to rectangles:
# measured by `region_end_to_end.py` over these same held-out tables.
KNOWN_CEILING = 0.9085


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--grids", type=pathlib.Path, required=True)
    ap.add_argument("--rows", required=True)
    ap.add_argument("--columns", required=True)
    ap.add_argument("--floor", type=float, default=0.5)
    ap.add_argument("--bridges", default=None,
                    help="the bridge model of `train_bridges.py`; without it the "
                         "stripes are built from every word, as before it")
    ap.add_argument("--bridge-floor", type=float, default=0.5,
                    help="chosen out of fold on the TRAINING documents by "
                         "`train_bridges.py`, never here")
    ap.add_argument("--split", choices=("train", "test"), default="test",
                    help="which documents to score. THE FLOOR IS CHOSEN ON "
                         "`train` AND REPORTED ON `test`: sweeping it on the "
                         "half the number is quoted from is tuning on the "
                         "evaluation set, and the best of eight floors there "
                         "is not the floor's worth, it is the sweep's.")
    ap.add_argument("--slack", type=float, default=0.002,
                    help="how far the measured ceiling may sit from the known "
                         "one before this refuses to report anything")
    args = ap.parse_args()

    import lightgbm as lgb
    import numpy as np

    rows_model = lgb.Booster(model_file=args.rows)
    cols_model = lgb.Booster(model_file=args.columns)
    bridges = lgb.Booster(model_file=args.bridges) if args.bridges else None

    manifest = json.loads((ts.STORE / "pdf" / "MANIFEST.json").read_text())
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    documents = sorted({p["document"] for p in manifest["pages"]})
    random.Random(ts.SEED).shuffle(documents)
    cut = int(len(documents) * 0.75)
    test = set(documents[:cut] if args.split == "train" else documents[cut:])
    grids = {}
    with gzip.open(args.grids, "rt") as handle:
        for line in handle:
            row = json.loads(line)
            if row["document"] in test:
                grids[row["sha256"]] = row
    print(f"grids for {len(grids)} {args.split} page(s)", flush=True)

    # THE INSTRUMENT IS CHECKED BEFORE IT IS BELIEVED, on every run.
    #
    # The ceiling -- our own words in the annotation's grid, flattened -- is a
    # KNOWN quantity: 0.9085, measured by `region_end_to_end.py` over these
    # same tables. Computing it here from the same truth the decoder is scored
    # against turns a broken truth into a refusal instead of a result.
    #
    # It has already earned its place. A compact cache of the annotations
    # dropped `pdf_text_content`, `html_of` wrote a truth of empty cells, and
    # the decoder scored 0.2013 against nothing. The ceiling read 0.1504 in
    # the same run and 0.9085 in the harness next door, and only that
    # disagreement showed which of the two was broken (COS, #574).
    scores: list[float] = []
    ceiling: list[float] = []
    empty = 0
    for line in open(args.dump):
        record = json.loads(line)
        page = by_sha.get(record["pdf_sha256"])
        if not page or page["document"] not in test:
            continue
        height = float(record["height"])
        for table in (grids.get(record["pdf_sha256"], {}).get("tables") or []):
            if abs(height - float(table["page_height"])) > ts.HEIGHT_SLACK:
                continue
            cells = [{"row_nums": c["r"], "column_nums": c["c"],
                      "pdf_bbox": c["b"], "pdf_text_tight_bbox": c["t"],
                      "pdf_text_content": c["x"],
                      "is_column_header": c["h"]} for c in table["cells"]]
            if len(cells) < 4:
                continue
            table = {"pdf_table_bbox": table["box"]}
            want = pubtables_cells.html_of({"cells": cells})[0]
            if not want:
                continue
            html = ts.decode(record, table, rows_model, cols_model, args.floor, np,
                             bridges=bridges, bridge_floor=args.bridge_floor)
            if not html:
                empty += 1
            scores.append(structured_axes.teds(want, html) if html else 0.0)

            # The same words, placed by the annotation, merges flattened.
            box = pa.page_frame(table["pdf_table_bbox"], height)
            words = [w for w in (record.get("words") or [])
                     if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                     and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
            held: dict[tuple[int, int], list[str]] = {}
            # A HEADER IS THE ANNOTATION'S FLAG, not the number of the row.
            # `rectangular_grid_html` reads `is_column_header`, and taking row
            # zero instead put the ceiling at 0.8853 against its known 0.9085
            # -- the guard below refused the run over exactly that. It is also
            # the defect review found in the Rust rebuild, arriving a second
            # time in the harness that measures it.
            header: set[tuple[int, int]] = set()
            if len(words) >= 4:
                for word, spot in zip(words, pa.place([w["bbox"] for w in words],
                                                      cells, height)):
                    if spot is None:
                        continue
                    key = (cells[spot]["row_nums"][0], cells[spot]["column_nums"][0])
                    held.setdefault(key, []).append(word["text"])
                    if cells[spot].get("is_column_header"):
                        header.add(key)
            perfect = pubtables_cells.html_of({"cells": [
                {"row_nums": [r], "column_nums": [c], "pdf_text_content": " ".join(v),
                 "is_column_header": 1 if (r, c) in header else 0}
                for (r, c), v in sorted(held.items())]})[0] if len(held) >= 4 else None
            ceiling.append(structured_axes.teds(want, perfect) if perfect else 0.0)

    got = statistics.fmean(scores) if scores else float("nan")
    reached = statistics.fmean(ceiling) if ceiling else float("nan")
    print(f"\nthe ceiling this run measures: {reached:.4f} "
          f"(known to be {KNOWN_CEILING:.4f})")
    # THE KNOWN CEILING IS A TEST-HALF FIGURE, so on the train half the check
    # is that the truth is intact rather than that it equals a number from
    # another population. A truth of empty cells reads about 0.15, so a floor
    # of 0.85 separates "broken" from "a different set of tables" without
    # pretending to a precision this split cannot have.
    if args.split == "train":
        if reached < 0.85:
            print(f"\nREFUSED: the ceiling on the train half is {reached:.4f}, "
                  f"far below the 0.9085 the test half reads.\nA truth that "
                  f"cannot be read is not a set to tune a threshold on.",
                  file=sys.stderr)
            return 2
    elif abs(reached - KNOWN_CEILING) > args.slack:
        print(f"\nREFUSED: the ceiling is {abs(reached - KNOWN_CEILING):.4f} away "
              f"from the known {KNOWN_CEILING:.4f}, which is more than "
              f"{args.slack}.\nThe truth this run scores against is not the "
              f"truth that number came from, so the\ndecoder's figure would "
              f"be a statement about the instrument. No result printed.",
              file=sys.stderr)
        return 2
    bridged = f", bridges at {args.bridge_floor}" if bridges else ""
    print(f"TEDS over {len(scores)} {args.split} table(s) at floor {args.floor}{bridged}: "
          f"{got:.4f}   (reached none of it on {empty})")
    print("  as it ships                     0.5996")
    print("  the learned pair grid           0.6073")
    print("  a rectangular grid, our words   0.9085")
    print("  the annotation's grid           0.9404")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
