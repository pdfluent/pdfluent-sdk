#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A trained line graph network's parity pages and manifest (#574).

    ~/.cache/pdfluent/gnn-venv/bin/python make_gnn_parity.py \
        --model /tmp/line-gnn --cache graphs.pkl --features DUMP_DIR \
        --out models/line_gnn_v2 --trainer "train_line_gnn.py --groups style ..."

PARITY: five validation pages of a graph cache, each as the engine would
build it -- the raw node rows (the columns of the model's own groups), the
neighbours and the raw edge rows -- with PyTorch's class and link logits.
`learned_list_models_reproduce_lightgbm.rs` holds the Rust network to them.
Any valid graph serves: the test is of the arithmetic, not of the data.

MANIFEST: the documents the network trained on, chosen the trainer's way
(`chosen_pages` over the dump, at most `--pages-per-document` pages each) and
split the trainer's way (`third`), for the leakage guard to check against the
rig.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import pickle
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from train_heading_lines import chosen_pages, records  # noqa: E402
from train_line_gnn import SEED, group_columns, model  # noqa: E402
from train_lists import third  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--model", type=pathlib.Path, required=True)
    ap.add_argument("--cache", type=pathlib.Path, required=True)
    ap.add_argument("--features", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--trainer", required=True)
    ap.add_argument("--pages-per-document", type=int, default=20)
    ap.add_argument("--pages", type=int, default=5)
    args = ap.parse_args()
    import numpy as np
    import torch

    cfg = json.loads((args.model / "config.json").read_text())
    norm = np.load(args.model / "norm.npz")
    net = model(cfg["hidden"], cfg["layers"], cfg["node_width"])
    if len(cfg["classes"]) != net.classes.out_features:
        net.classes = torch.nn.Linear(cfg["hidden"], len(cfg["classes"]))
    net.load_state_dict(torch.load(args.model / "line_gnn.pt", map_location="cpu"))
    net.eval()
    cols = group_columns(cfg.get("groups", []))
    assert len(cols) == cfg["node_width"], (len(cols), cfg["node_width"])

    graphs = pickle.load(open(args.cache, "rb"))
    pool = [g for g in graphs if g[5] == "validation" and 5 <= len(g[0]) <= 60]
    rng = np.random.default_rng(SEED)
    chosen = [pool[i] for i in sorted(rng.choice(len(pool), size=args.pages, replace=False))]
    pages = []
    for g in chosen:
        nodes = g[0][:, cols]
        x = torch.tensor((nodes - norm["mx"]) / norm["sx"], dtype=torch.float32)
        ef = torch.tensor((g[2] - norm["me"]) / norm["se"], dtype=torch.float32)
        with torch.no_grad():
            logits, link = net(x, torch.from_numpy(g[1]), ef)
        pages.append({"nodes": nodes.tolist(), "neighbours": g[1].tolist(),
                      "edges": g[2].tolist(), "classes": logits.tolist(),
                      "link": link.tolist()})
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "parity.json").write_text(json.dumps({"pages": pages}) + "\n")

    wanted = chosen_pages(args.features, args.pages_per_document, None)
    documents, schema = {}, None
    for r in records(args.features, None):
        if r["page_id"] in wanted:
            documents[r.get("document") or r["pdf_sha256"]] = None
            schema = schema or r.get("schema")
    name = args.out.name
    manifest = {
        "task": name, "weights": "weights.json.gz",
        "rust": f"crates/pdf-engine/src/learned/models/{name}",
        "classes": cfg["classes"], "groups": cfg.get("groups", []),
        "pages": len(wanted), "pages_per_document": args.pages_per_document,
        "schema": schema, "trainer": args.trainer,
        "documents": [{"source": "doclaynet-v1.2/train", "document": d, "split": third(d)}
                      for d in sorted(documents)],
    }
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
    print(f"[gnn-parity] {args.out}: {len(pages)} parity page(s), "
          f"{len(documents)} document(s), {len(wanted)} page(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
