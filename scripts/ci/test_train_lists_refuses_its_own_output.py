#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A model is never trained on blocks the models already touched (#599).

Once `pdf_engine::learned::refine` runs, a block's kind is the model's own
decision. A dump made in that state and trained on teaches a model its own
output, and the score goes up for no reason anyone can see (T3). So the dump
records `refine`, `feature_dump` always writes "off", and
`scripts/extraction/learned/train_lists.py` refuses a record that says anything
but "off" unless `--second-generation` gives a reason.

ANYTHING BUT "off", and not the one word "on": `feature_dump --refine heading`
stamps the switch SET it ran, so a refusal that named "on" let every measurement
dump straight through -- past the one guard between a model and its own output
(codex, #1983). A named set is a case here for that reason.

Three cases, and a mutation: the refusal is taken out of a copy of the trainer,
and the "on" record then has to get through -- which proves the refusal is what
stopped it, and not something else in the file.

Every record carries this tree's engine, so the refine guard is tested alone:
the engine guard has its own cases in `test_training_rules_refuse.py`.
"""
from __future__ import annotations

import gzip
import importlib.util
import json
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
TRAINER = REPO / "scripts" / "extraction" / "learned" / "train_lists.py"


def load(source: str, name: str):
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False,
                                     dir=TRAINER.parent) as f:
        f.write(source)
        path = pathlib.Path(f.name)
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        path.unlink()


def features(tmp: pathlib.Path, refine: str | None, tree: str) -> pathlib.Path:
    d = tmp / (refine or "none")
    d.mkdir()
    record = {"schema": "pdfluent.features/1", "page_id": "p", "pdf_sha256": "s", "blocks": [], "words": [],
              "engine": {"tree": tree, "commit": "test"}}
    if refine is not None:
        record["refine"] = refine
    with gzip.open(d / "train-00000.jsonl.gz", "wt") as f:
        f.write(json.dumps(record) + "\n")
    return d


def consumes(module, path: pathlib.Path, second: str | None) -> bool:
    try:
        list(module.records(path, second))
        return True
    except SystemExit:
        return False


def main() -> int:
    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_lists_real")
    sys.path.insert(0, str(TRAINER.parent))
    import training_rules
    serving = training_rules.serving_tree()
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        off, on, old = (features(tmp, "off", serving), features(tmp, "on", serving),
                        features(tmp, None, serving))
        named = features(tmp, "heading,heading_ends", serving)
        if not consumes(real, off, None):
            fails.append("a record dumped with refine off was refused")
        if not consumes(real, old, None):
            fails.append("a record from before `refine` existed was refused")
        if consumes(real, on, None):
            fails.append("a record dumped with refine ON was trained on")
        if consumes(real, named, None):
            fails.append("a record dumped with a named switch set was trained on")
        if not consumes(real, on, "a deliberate second generation, measured against the first"):
            fails.append("--second-generation with a reason did not let the record through")

        guard = 'if said != "off" and not second_generation:'
        if guard not in source:
            fails.append(f"the refusal ({guard!r}) is not in train_lists.py; the mutation cannot be made")
        else:
            mutant = load(source.replace(guard, "if False:"), "train_lists_mutant")
            if not consumes(mutant, on, None):
                fails.append("with the refusal taken out, the ON record is still stopped -- "
                             "something else stops it, and this test is not testing the refusal")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("train-lists: a dump made with the models on is refused, and only the refusal refuses it.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
