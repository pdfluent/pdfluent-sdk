#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Python's table judge on a few real pages, for `judge.rs` to match (#574).

    make_judge_parity.py --dump val6.jsonl --model judge.txt

For each page: the lines as `table_proposals.page_lines` reads them, the
region hulls T2's detectors drew, and what Python makes of them -- every
proposal with its kind, its features, the model's probability, and the
tables chosen at the shipped floor. The Rust has to agree on all of it, which
holds the hull rule, the alignment port, the proposals, the 29 features, the
exported model and the choice together in one test.

FROM THE VALIDATION SPLIT: a fixture is not training data.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import table_proposals as tp  # noqa: E402
import train_table_judge as J  # noqa: E402

REPO = HERE.parents[3]
OUT = REPO / "crates/pdf-engine/tests/fixtures/learned/judge_parity.json"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--floor", type=float, required=True)
    ap.add_argument("--pages", type=int, default=12)
    args = ap.parse_args()

    import lightgbm as lgb
    import numpy as np
    model = lgb.Booster(model_file=args.model)

    out, kinds_seen = [], set()
    for raw in open(args.dump):
        page = json.loads(raw)
        if not str(page.get("source", "")).endswith("/validation"):
            continue
        drawn = page.get("table_proposals") or []
        if not drawn:
            continue
        try:
            lines, _ = tp.page_lines(page)
        except (Exception, SystemExit):
            continue
        if not 8 <= len(lines) <= 120:
            continue
        props = tp.proposals(lines, page)
        kinds = {k for _, k, _ in props}
        # Pages that add a kind not yet in the fixture, so every kind is held.
        if len(out) >= args.pages // 2 and kinds <= kinds_seen:
            continue
        regions_sets = [c for c, k, _ in props if k not in ("alignment_lines", "split", "union")]
        align_sets = [c for c, k, _ in props if k == "alignment_lines"]
        xs = [J.features(page, lines, c, k, regions_sets, align_sets, a) for c, k, a in props]
        p = list(model.predict(np.asarray(xs, dtype=np.float32), num_threads=1))
        rows = [{"lines": set(c), "x": x} for (c, _, _), x in zip(props, xs)]
        chosen = J.choose(rows, p, args.floor)
        out.append({
            "page_id": page.get("page_id"),
            "size": [float(page["width"]), float(page["height"])],
            "lines": [{"bbox": l["bbox"], "text": l["text"], "words": l["n_words"],
                       "digits": l["digits"], "kind": l["kind"], "table": l["table"],
                       "grid_fill": l["grid_fill"], "grid_columns": l["grid_columns"],
                       "list": l["list"]}
                      for l in lines],
            "hulls": [{"detector": d["detector"], "hull": d["hull"], "lines": sorted(d["lines"]),
                       "accepted": bool(d.get("accepted", True))}
                      for d in drawn if d.get("source") == "region"],
            # The pieces as the dump wrote them, so the Rust `splits::pieces`
            # is held to the same sets.
            "pieces": [sorted(d["lines"]) for d in drawn if d.get("source") == "split"],
            "proposals": [{"lines": sorted(c), "kind": k, "accepted": a} for c, k, a in props],
            "features": xs,
            "probabilities": p,
            "chosen": [sorted(c) for c in chosen],
        })
        kinds_seen |= kinds
        if len(out) >= args.pages:
            break
    OUT.parent.mkdir(parents=True, exist_ok=True)
    # NaN AS null: JSON has no NaN, and serde_json refuses the token Python
    # writes for it. The Rust reads null back as NaN, which is what LightGBM
    # was given.
    def clean(v):
        if isinstance(v, float) and v != v:
            return None
        if isinstance(v, list):
            return [clean(x) for x in v]
        if isinstance(v, dict):
            return {k: clean(x) for k, x in v.items()}
        return v
    OUT.write_text(json.dumps(clean({"names": J.NAMES, "floor": args.floor, "pages": out}),
                              allow_nan=False) + "\n")
    print(f"{OUT.relative_to(REPO)}: {len(out)} pages, kinds {sorted(kinds_seen)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
