#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A generated model is what the exporter makes of its dump, byte for byte (#598).

The parity test holds the Rust to LightGBM's margins within 1e-6, which a hand
edit of a leaf in the tenth decimal passes. This closes that: every model dir
under scripts/extraction/learned/models/ names its generated file in
`manifest.json` (`rust`), and re-exporting `model.json` must reproduce that file
exactly. No LightGBM needed -- the exporter reads the dump -- so this runs in CI.
Also: the exporter refuses a categorical split rather than approximating it.
"""
from __future__ import annotations

import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "learned"))
from export_rust import (export, export_chunked,
                         export_multiclass_chunked, read_model)  # noqa: E402
from export_gnn_rust import export as export_gnn  # noqa: E402


def main() -> int:
    fails, seen = [], 0
    for manifest in sorted((REPO / "scripts/extraction/learned/models").glob("*/manifest.json")):
        data = json.loads(manifest.read_text())
        target = data.get("rust")
        if not target:
            fails.append(f"{manifest.relative_to(REPO)}: names no generated file ('rust')")
            continue
        # A NETWORK'S WEIGHTS, not a tree dump (#638): re-exported by its own
        # exporter, file for file, the same way.
        if data.get("weights"):
            weights = manifest.parent / data["weights"]
            if not weights.exists():
                fails.append(f"{manifest.relative_to(REPO)}: no {data['weights']} beside it")
                continue
            seen += 1
            files = export_gnn(weights.read_bytes())
            where = REPO / target
            have = {p.name for p in where.glob("*.rs")}
            for stale in sorted(have - set(files)):
                fails.append(f"{target}/{stale}: not made by export_gnn_rust.py from {weights.name}")
            for name, code in files.items():
                path = where / name
                if not path.exists() or path.read_text() != code:
                    fails.append(f"{target}/{name}: differs from what export_gnn_rust.py makes of "
                                 f"{weights.relative_to(REPO)} -- hand-edited, or not re-exported")
            continue
        dump = next((manifest.parent / n for n in ("model.json", "model.json.gz")
                     if (manifest.parent / n).exists()), None)
        if dump is None:
            fails.append(f"{manifest.relative_to(REPO)}: no model.json or model.json.gz beside it")
            continue
        seen += 1
        where = REPO / target
        if where.is_dir():
            # A model in parts (#599): every file the exporter makes, and no
            # stale part left over from a larger model.
            # A MULTICLASS MODEL HAS NO ONE THRESHOLD, because it has no one
            # decision: `predict_margins` gives a margin per class and the
            # caller takes the argmax or a floor of its own. `classes` in the
            # manifest is what says which writer made the file, and reading
            # `threshold` unconditionally used to raise a KeyError here rather
            # than check anything (T4, #600).
            if len(data.get("classes") or []) > 1:
                files, _ = export_multiclass_chunked(read_model(dump),
                                                     manifest.parent.name)
            elif "threshold" not in data:
                fails.append(f"{manifest.relative_to(REPO)}: a single-class model "
                             f"in parts must name its 'threshold'")
                continue
            else:
                files, _ = export_chunked(read_model(dump), manifest.parent.name,
                                          data["threshold"])
            have = {p.name for p in where.glob("*.rs")}
            for stale in sorted(have - set(files)):
                fails.append(f"{target}/{stale}: not made by export_rust.py from {dump.name}")
            for name, code in files.items():
                path = where / name
                if not path.exists() or path.read_text() != code:
                    fails.append(f"{target}/{name}: differs from what export_rust.py makes of "
                                 f"{dump.relative_to(REPO)} -- hand-edited, or not re-exported")
            continue
        code, _ = export(read_model(dump), manifest.parent.name)
        if code != where.read_text():
            fails.append(f"{target}: differs from what export_rust.py makes of "
                         f"{dump.relative_to(REPO)} -- hand-edited, or not re-exported")
    categorical = json.dumps({"feature_names": ["a"], "tree_info": [{"tree_structure": {
        "split_feature": 0, "threshold": "1||2", "decision_type": "==", "default_left": True,
        "missing_type": "None", "left_child": {"leaf_value": 1.0},
        "right_child": {"leaf_value": 0.0}}}]}).encode()
    try:
        export(categorical, "c")
        fails.append("a categorical split was exported instead of refused")
    except SystemExit:
        pass
    if seen == 0:
        fails.append("no model to check; the floor is one")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print(f"learned-export: {seen} generated model(s) reproduce from their dumps.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
