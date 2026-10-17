"""Write the fixture that holds the table-or-not model's Rust to its Python (#638).

    make_table_is_parity.py --dump TRAIN.jsonl --model table_is.txt [--tables 24]

Builder tables from DocLayNet-train pages, real and false alike: each one's
cells (text, box, row, column, the heading probability the pass decides on),
the page height, and what Python makes of them -- `train_table_is.feats` and
the model's probability. The Rust test rebuilds the blocks and hints and
recomputes both.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations

import argparse
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
REPO = HERE.parents[3]
OUT = REPO / "crates" / "pdf-engine" / "tests" / "fixtures" / "learned" / "table_is_parity.json"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--tables", type=int, default=24)
    args = ap.parse_args()
    import lightgbm as lgb
    import numpy as np
    import train_table_is as T

    model = lgb.Booster(model_file=args.model)
    out, seen = [], {0: 0, 1: 0}
    for raw in open(args.dump):
        record = json.loads(raw)
        for _, cells in T.tables_of(record).items():
            if len(cells) < 4:
                continue
            y = T.label(record, cells)
            if y is None or seen[y] >= args.tables // 2:
                continue
            seen[y] += 1
            x = T.feats(record, cells)
            out.append({
                "height": float(record["height"]),
                "cells": [{"text": c["text"], "bbox": c["bbox"], "row": c["row"],
                           "column": c["column"], "p": c.get("p_heading_pass")} for c in cells],
                "features": x,
                "probability": float(model.predict(np.asarray([x], dtype=np.float32))[0]),
            })
        if len(out) >= args.tables:
            break
    OUT.write_text(json.dumps({"names": T.NAMES, "tables": out}, allow_nan=False) + "\n")
    print(f"{OUT.relative_to(REPO)}: {len(out)} tables ({seen[1]} real, {seen[0]} false)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
