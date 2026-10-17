"""Write the fixture that holds `learned::verifier` to `table_verifier.py` (#638).

    make_verifier_parity.py --dump PAGES.jsonl --model verifier.txt [--pages 8]

Per page: its lines and words, the judge's proposals and probabilities, and
what Python makes of them -- the candidates, each one's features and the
ranker's score, and the tables chosen. The Rust test recomputes all of it
from the lines, words, proposals and probabilities alone.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations

import argparse
import json
import math
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
REPO = HERE.parents[3]
OUT = REPO / "crates" / "pdf-engine" / "tests" / "fixtures" / "learned" / "verifier_parity.json"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", action="append", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--pages", type=int, default=8)
    args = ap.parse_args()
    sys.argv = ["x", "--train", "x", "--validation", "x"]
    import lightgbm as lgb
    import numpy as np
    import table_verifier as V
    import train_table_judge as J

    ranker = lgb.Booster(model_file=args.model)
    judge = V.Trees("table_judge")
    models = (V.Trees("table_row_boundary"), V.Trees("table_column_boundary"),
              V.Trees("table_column_bridge"))
    floor = judge.threshold
    out = []
    for path in args.dump:
        taken = 0
        for raw in open(path):
            record = json.loads(raw)
            if "labels" not in record:
                record["regions"] = []
            try:
                rows, cands = V.page_candidates(record, judge, models)
            except (Exception, SystemExit):
                continue
            if len(cands) < 3 or not any(c["p"] >= floor for c in cands):
                continue
            props = rows["props"]
            probs = [1.0 / (1.0 + math.exp(-judge.margin(r["x"]))) for r in props]
            scores = list(ranker.predict(np.asarray([c["x"] for c in cands], dtype=np.float32)))
            order = sorted([k for k, c in enumerate(cands) if c["p"] >= floor],
                           key=lambda k: (-scores[k], -cands[k]["p"], cands[k]["i"]))
            used, chosen = set(), []
            words = [{"bbox": w["bbox"], "text": w["text"]} for w in record.get("words") or []]
            for k in order:
                if used & set(cands[k]["lines"]):
                    continue
                kept = V.without_title_rows(rows["lines"], words, cands[k]["lines"],
                                            cands[k]["inside"], cands[k]["cells"])
                used |= set(kept)
                chosen.append(kept)
            kinds = [J.tp.KINDS[[i for i in range(len(J.tp.KINDS)) if r["x"][i] == 1.0][0]] for r in props]
            out.append({
                "size": [float(record["width"]), float(record["height"])],
                "lines": [{"bbox": l["bbox"], "text": l["text"]} for l in rows["lines"]],
                "words": [{"bbox": w["bbox"], "text": w["text"]} for w in record.get("words") or []],
                "proposals": [{"lines": sorted(r["lines"]), "kind": k} for r, k in zip(props, kinds)],
                "probabilities": probs,
                "candidates": [c["i"] for c in cands],
                "features": [c["x"] for c in cands],
                "scores": [float(s) for s in scores],
                "chosen": chosen,
            })
            taken += 1
            if taken >= args.pages:
                break
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"names": V.NAMES, "floor": floor, "pages": out}, allow_nan=False) + "\n")
    print(f"{OUT.relative_to(REPO)}: {len(out)} pages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
