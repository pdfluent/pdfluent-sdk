#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Write a model the way `export_rust.py` and the parity tests expect (#600).

A model that ships is three files beside each other, and each answers a
question that has been asked in anger:

  `model.json.gz`  the LightGBM dump, which the generated Rust names by its
                   sha256 so a dump that changes without the exporter being
                   run again is a red test and not a silent divergence;
  `manifest.json`  what it was trained on and with -- the split, the counts,
                   the parameters, the threshold. A number in a commit message
                   is gone in a month; this travels with the model;
  `parity.json`    rows of real feature vectors and LightGBM's OWN margins for
                   them. The generated Rust must reproduce these, which is the
                   only check that the exporter did not quietly mistranslate a
                   split. For a multiclass model each row's margins are a LIST,
                   one per class, because the failure this catches is exactly
                   the one where the classes get added together.

THE ROWS ARE REAL, NOT RANDOM. A random vector mostly lands in the same few
leaves and a mistranslated split deep in a tree is never reached. These are
drawn from the training matrix itself, evenly spaced through it, so the rows
that are common are common here too.
"""
from __future__ import annotations

import gzip
import json
import pathlib

PARITY_ROWS = 300


def require_training_source(manifest: dict, where: str, split: str = "train") -> None:
    """Refuse a corpus manifest that is not the TRAINING split.

    WHY EVERY TRAINER SHOULD CALL THIS. T3 found a glob of `*.jsonl.gz` that
    swept up the validation shards beside the train ones, and 58 per cent of
    the set they were measuring on had been trained upon. Nothing failed: the
    model simply looked better than it was.

    A trainer that reads a manifest and trusts its declarations cannot catch
    that. This makes the declaration a CHECK -- the split it claims, and that
    the rig was excluded when it was drawn -- so the refusal lives in one place
    and every trainer gets it by importing this module rather than by
    remembering (COS, #574).

    It is not a substitute for `learned_training_uses_no_rig_document.py`,
    which reads the model's own manifest at CI time. This is the earlier and
    cheaper half: it refuses before the training run rather than after it.
    """
    got = manifest.get("split")
    if got != split:
        raise SystemExit(
            f"{where}: the manifest says split {got!r}, not {split!r}. "
            "Training on anything but the training split puts the pages a "
            "number is quoted from inside the model that produced it."
        )
    if "rig_documents_excluded" not in manifest:
        raise SystemExit(
            f"{where}: the manifest does not say whether rig documents were "
            "excluded when it was drawn. A corpus that cannot state that is "
            "not one to train on."
        )
    print(f"[source] {where}: split {got}, "
          f"{manifest['rig_documents_excluded']} rig document(s) excluded, "
          f"licences {manifest.get('licence_codes')}", flush=True)


def write(directory: str | pathlib.Path, booster, x, names: list[str],
          manifest: dict, rows: int = PARITY_ROWS) -> pathlib.Path:
    """The three files, and the manifest given back with what was written."""
    import numpy as np

    out = pathlib.Path(directory)
    out.mkdir(parents=True, exist_ok=True)

    dump = json.dumps(booster.dump_model()).encode()
    (out / "model.json.gz").write_bytes(gzip.compress(dump, mtime=0))

    x = np.asarray(x, dtype=np.float32)
    step = max(1, len(x) // rows)
    picked = x[::step][:rows]
    margins = booster.predict(picked, raw_score=True, num_threads=1)
    margins = np.asarray(margins)
    (out / "parity.json").write_text(json.dumps({
        "features": names,
        "rows": [[None if np.isnan(v) else float(v) for v in row]
                 for row in picked],
        "margins": ([[float(v) for v in row] for row in margins]
                    if margins.ndim == 2 else [float(v) for v in margins]),
    }))

    manifest = dict(manifest)
    manifest["features"] = names
    manifest["parity_rows"] = len(picked)
    try:
        import lightgbm as lgb
        manifest["lightgbm"] = lgb.__version__
    except Exception:
        pass
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1,
                                                  sort_keys=True))
    return out
