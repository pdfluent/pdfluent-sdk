#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Python's separation features for a few tables, for the Rust to match (#574).

    make_separation_parity.py --dump dump5.jsonl \
        [--grids grids.jsonl.gz --rows rows.txt --columns cols.txt \
         --bridges bridges.txt --bridge-floor F]

The second line adds WHOLE DECODES: real PubTables tables that hold at least
one bridging word, with the bridge decision and the row, column and last column
Python's `grid_positions` gives every word under the shipped models. The Rust
has to place every word the same way, which holds the features, the three
exported models and the decoder together in one assertion (#574).

THE MODEL IS TRAINED ON WHAT THIS FILE COMPUTES and run on what
`separations.rs` computes. A feature that differs in the sixth decimal is not
a small error: it is the model reading input it never saw in training, and it
does not announce itself -- the grids come out slightly worse and it reads as
a weak model, so it gets answered with more trees instead of with a fix.

This pair of tickets has paid for that lesson nine times, most recently when a
harness scored a decoder against a truth of empty cells and the number looked
merely disappointing rather than broken.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import statistics
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import separation_features as sf  # noqa: E402

REPO = HERE.parents[3]
OUT = REPO / "crates/pdf-engine/tests/fixtures/learned/separation_parity.json"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--tables", type=int, default=12)
    ap.add_argument("--grids", type=pathlib.Path, default=None)
    ap.add_argument("--rows", default=None)
    ap.add_argument("--columns", default=None)
    ap.add_argument("--bridges", default=None)
    ap.add_argument("--bridge-floor", type=float, default=None)
    ap.add_argument("--separation-floor", type=float, default=0.1)
    ap.add_argument("--decodes", type=int, default=8)
    args = ap.parse_args()

    out = []
    for line in open(args.dump):
        record = json.loads(line)
        words = [w for w in (record.get("words") or []) if (w.get("text") or "").strip()]
        if len(words) < 12:
            continue
        # A slice of the page that looks like a table: enough words, and a
        # spread of them. The fixture is about the ARITHMETIC, so any real
        # geometry serves; what it must not be is invented boxes, because
        # those never exercise the ties and the zero gaps that real pages do.
        words = words[:60]
        rules = [{"y": r["y"]} for r in (record.get("rules") or [])]
        height = statistics.median([w["bbox"][3] - w["bbox"][1] for w in words]) or 1.0
        width = statistics.median([w["bbox"][2] - w["bbox"][0] for w in words]) or 1.0
        rows = [{"above": [words.index(w) for w in a],
                 "below": [words.index(w) for w in b], "x": x}
                for a, b, x in sf.row_candidates(words, rules, height)]
        cols = [{"left": [words.index(w) for w in a],
                 "right": [words.index(w) for w in b], "x": x}
                for a, b, x in sf.column_candidates(words, width)]
        if not rows or not cols:
            continue
        out.append({
            "page_id": record.get("page_id"),
            # THE TEXT TRAVELS WITH THE BOX so the Rust side can derive
            # `numeric` itself. Shipping the flag pre-computed made the fixture
            # blind to exactly the divergence review found: the two sides
            # disagreed on what a digit is and every value in the fixture
            # still matched (codex, #574).
            "words": [{"bbox": w["bbox"], "text": w.get("text") or "",
                       "numeric": sf.a_digit_in(w.get("text") or "")}
                      for w in words],
            "rules": rules,
            "text_height": height,
            "word_width": width,
            "rows": rows,
            "columns": cols,
            "bridges": [x for _, x in sf.bridge_candidates(words, width)],
        })
        if len(out) >= args.tables:
            break

    decodes = decodes_of(args) if args.grids else []

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"tables": out, "row_names": sf.ROW_NAMES,
                               "column_names": sf.COLUMN_NAMES,
                               "bridge_names": sf.BRIDGE_NAMES,
                               "decodes": decodes,
                               "separation_floor": args.separation_floor,
                               "bridge_floor": args.bridge_floor},
                              sort_keys=True) + "\n")
    print(f"{len(decodes)} whole decode(s)")
    gaps = sum(len(t["rows"]) + len(t["columns"]) for t in out)
    print(f"{OUT.relative_to(REPO)}: {len(out)} tables, {gaps} gaps")
    return 0


def decodes_of(args) -> list[dict]:
    """Real tables with a bridge in them, decoded by the shipped models."""
    import gzip
    import lightgbm as lgb
    import numpy as np
    import pubtables_alignment as pa
    import train_separations as ts

    rows_model = lgb.Booster(model_file=args.rows)
    cols_model = lgb.Booster(model_file=args.columns)
    bridges = lgb.Booster(model_file=args.bridges)
    manifest = json.loads((ts.STORE / "pdf" / "MANIFEST.json").read_text())
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    documents = sorted({p["document"] for p in manifest["pages"]})
    import random
    random.Random(ts.SEED).shuffle(documents)
    # FROM THE HELD-OUT HALF: a fixture is not training data, and a table the
    # bridge model was fitted on would show it at its most agreeable.
    held = set(documents[int(len(documents) * 0.75):])
    grids = {}
    with gzip.open(args.grids, "rt") as handle:
        for line in handle:
            row = json.loads(line)
            if row["document"] in held:
                grids[row["sha256"]] = row
    out: list[dict] = []
    for line in open(args.dump):
        record = json.loads(line)
        page = by_sha.get(record["pdf_sha256"])
        if not page or page["document"] not in held:
            continue
        height = float(record["height"])
        for table in (grids.get(record["pdf_sha256"], {}).get("tables") or []):
            box = pa.page_frame(table["box"], height)
            words = [{"bbox": w["bbox"], "text": w["text"]} for w in (record.get("words") or [])
                     if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                     and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
            if not 12 <= len(words) <= 160:
                continue
            text_height = statistics.median([w["bbox"][3] - w["bbox"][1] for w in words]) or 1.0
            width = statistics.median([w["bbox"][2] - w["bbox"][0] for w in words]) or 1.0
            bridge = ts.bridging(words, width, bridges, args.bridge_floor, np)
            if not bridge:
                continue
            row_of, col_of, spans = ts.grid_positions(
                words, [], text_height, width, rows_model, cols_model,
                args.separation_floor, np, bridge)
            slots = {(row_of[id(w)], col_of[id(w)]) for w in words}
            boxes: dict = {}
            for w in words:
                key = (row_of[id(w)], col_of[id(w)])
                if key in boxes:
                    boxes[key] = (min(boxes[key][0], w["bbox"][0]),
                                  max(boxes[key][1], w["bbox"][2]))
                else:
                    boxes[key] = (w["bbox"][0], w["bbox"][2])
            header, rowspans, spans = ts.dress(slots, spans, boxes=boxes)
            reach = {key: cols[-1] for key, cols in spans.items()}
            low = {key: rows[-1] for key, rows in rowspans.items()}
            out.append({
                "page_id": record.get("page_id"),
                "words": [{"bbox": w["bbox"], "text": w["text"]} for w in words],
                "bridge": [id(w) in bridge for w in words],
                "row_of": [row_of[id(w)] for w in words],
                "column_of": [col_of[id(w)] for w in words],
                # The columns each CELL spans, keyed by its first word: what
                # `cells_placed` must rebuild from the placement.
                "cell_last_column": [reach.get((row_of[id(w)], col_of[id(w)]),
                                               col_of[id(w)]) for w in words],
                "cell_last_row": [low.get((row_of[id(w)], col_of[id(w)]),
                                          row_of[id(w)]) for w in words],
                "header_rows": len(header),
            })
            if len(out) >= args.decodes:
                return out
    return out


if __name__ == "__main__":
    raise SystemExit(main())
