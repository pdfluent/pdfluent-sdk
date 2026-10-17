#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The block trainer refuses what `training_rules` says it must (#602).

`scripts/extraction/learned/training_rules.py` holds four refusals, each
learnt by breaking it: a page from a split the axis is measured on, a dump made
with the learned pass on, a dump another engine tree segmented, and a half that
five documents speak for. This test holds the rules AND the trainer's use of
them:

- `train_blocks.matrix` takes a train page, refuses a validation page and a
  refine-on page, and lets either through with `--second-generation`;
- it refuses a page from before the engine guard and a page from another
  tree -- the message naming both trees -- lets either through with
  `--engine-skew-is-fine`, keeps the reason for the manifest, and takes a
  page from this tree;
- an "unknown" tree is refused even where the serving tree is "unknown" too,
  and a dirty tree ("<serving>-dirty") is refused with both trees named --
  each let through by `--engine-skew-is-fine`;
- the judging-half cap keeps 500 rows of one document and no more;
- a split whose top five documents carry over a fifth is refused, a balanced
  one is not, and `--lopsided-is-fine` lets the lopsided one through;
- every file matching `train_*.py` calls `training_rules.same_engine`, or
  carries TRAINING_RULES_NO_DUMP with the reason it reads no engine dump --
  the glob, not a list of names, so a trainer that lands tomorrow is covered
  tomorrow.

And three mutations: each refusal is taken out of a copy of the trainer, and
the refused page then has to get through -- which proves the call in the
trainer is what stops it, and not something else in the file. The third takes
the engine call out of a copy of a trainer's source and requires the glob
check to flag it -- and a fixture trainer that never had the call must fail
it too, so the check cannot pass by never looking.

No numpy on the gate, so `matrix` runs against a stand-in that returns its
input: the refusals happen before any array is built.
"""
from __future__ import annotations

import gzip
import importlib.util
import json
import pathlib
import sys
import tempfile
import types

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
TRAINER = LEARNED / "train_blocks.py"
CALL = "training_rules.only_the_train_split(record, str(path), second_generation)"
ENGINE_CALL = "training_rules.same_engine(record, str(path), engine_skew_is_fine)"
OTHER_TREE = "0" * 40
NO_DUMP_MARKER = "TRAINING_RULES_NO_DUMP"


def stand_in_numpy() -> None:
    if "numpy" in sys.modules:
        return
    fake = types.ModuleType("numpy")
    fake.asarray = lambda values, dtype=None: values
    sys.modules["numpy"] = fake


def load(source: str, name: str):
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False,
                                     dir=LEARNED) as f:
        f.write(source)
        path = pathlib.Path(f.name)
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        path.unlink()


def shard(tmp: pathlib.Path, name: str, source: str, refine: str = "off",
          tree: str | None = "serving") -> pathlib.Path:
    """One page: `tree` names the engine that segmented it -- "serving" for
    this tree, None for a dump from before the guard, or a tree hash."""
    record = {"schema": "pdfluent.features/1", "page_id": name, "document": name,
              "source": source, "refine": refine, "blocks": [], "words": []}
    if tree == "serving":
        import training_rules
        tree = training_rules.serving_tree()
    if tree is not None:
        record["engine"] = {"tree": tree, "commit": "test"}
    path = tmp / f"{name}.jsonl.gz"
    with gzip.open(path, "wt") as f:
        f.write(json.dumps(record) + "\n")
    return path


def takes(module, path: pathlib.Path, second: str | None,
          engine: str | None = None) -> bool:
    try:
        module.matrix([path], {}, keep=None, second_generation=second,
                      engine_skew_is_fine=engine)
        return True
    except SystemExit:
        return False


def refusal(module, path: pathlib.Path, second: str | None,
            engine: str | None = None) -> str:
    """The refusal message, or "" when the page went through."""
    try:
        module.matrix([path], {}, keep=None, second_generation=second,
                      engine_skew_is_fine=engine)
        return ""
    except SystemExit as e:
        return str(e)


def engine_guard_missing(source: str) -> bool:
    """The glob rule: a trainer calls `same_engine`, or says it reads no
    engine dump. `train_relations.py` is the one that says it: it trains on
    PubTables' own annotation words, which carry no engine stamp to refuse."""
    return "same_engine(" not in source and NO_DUMP_MARKER not in source


def refuses_split(rules, documents: list[str], allow: bool = False) -> bool:
    try:
        rules.refuse_a_lopsided_split("half", documents, [True] * len(documents),
                                      allow=allow)
        return False
    except SystemExit:
        return True


def main() -> int:
    stand_in_numpy()
    sys.path.insert(0, str(LEARNED))
    import training_rules as rules

    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_blocks_real")
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        train = shard(tmp, "a", "doclaynet/train")
        valid = shard(tmp, "b", "doclaynet/validation")
        on = shard(tmp, "c", "doclaynet/train", refine="heading")
        reason = "a deliberate second generation, measured against the first"
        if not takes(real, train, None):
            fails.append("a train page was refused")
        if takes(real, valid, None):
            fails.append("a validation page was trained on")
        if takes(real, on, None):
            fails.append("a page dumped with the learned pass on was trained on")
        if not takes(real, valid, reason) or not takes(real, on, reason):
            fails.append("--second-generation with a reason did not let the page through")

        if CALL not in source:
            fails.append(f"the split refusal ({CALL!r}) is not in train_blocks.py; "
                         "the mutation cannot be made")
        else:
            mutant = load(source.replace(CALL, "None"), "train_blocks_mutant")
            if not takes(mutant, valid, None):
                fails.append("with the split refusal taken out, the validation page "
                             "is still stopped -- something else stops it, and this "
                             "test is not testing the refusal")

        serving = rules.serving_tree()
        old = shard(tmp, "old", "doclaynet/train", tree=None)
        skewed = shard(tmp, "skewed", "doclaynet/train", tree=OTHER_TREE)
        skew_reason = "records from before the engine guard (#574)"
        if takes(real, old, None):
            fails.append("a page from before the engine guard was trained on")
        message = refusal(real, skewed, None)
        if not message:
            fails.append("a page segmented by another engine tree was trained on")
        else:
            if OTHER_TREE not in message or serving not in message:
                fails.append("the engine refusal does not name both trees: "
                             f"{message[:200]}")
            if "the blocks of this dump are not those of the engine that will read the model" not in message:
                fails.append("the engine refusal does not say whose blocks these are: "
                             f"{message[:200]}")
        if not takes(real, old, None, engine=skew_reason):
            fails.append("--engine-skew-is-fine with a reason did not let the old page through")
        kept = rules.engine_skew_manifest()
        if kept is None or kept.get("reason") != skew_reason:
            fails.append("the override reason is not what the manifest would record")
        if '"engine_skew"' not in source:
            fails.append("train_blocks.py never writes engine_skew to its manifest")
        current = shard(tmp, "current", "doclaynet/train", tree=serving)
        if not takes(real, current, None):
            fails.append("a page segmented by this engine tree was refused")
        saved, rules._SERVING_TREE = rules._SERVING_TREE, "unknown"
        try:
            ghost = shard(tmp, "ghost", "doclaynet/train", tree="unknown")
            if takes(real, ghost, None):
                fails.append('an "unknown" tree was trained on an "unknown" '
                             "serving tree: unknown agrees with nothing")
            if not takes(real, ghost, None, engine=skew_reason):
                fails.append('--engine-skew-is-fine did not let the "unknown" '
                             "page through")
        finally:
            rules._SERVING_TREE = saved
        dirty = shard(tmp, "dirty", "doclaynet/train", tree=serving + "-dirty")
        message = refusal(real, dirty, None)
        if not message:
            fails.append("a page segmented by a dirty engine tree was trained on")
        elif serving not in message or serving + "-dirty" not in message:
            fails.append("the dirty-tree refusal does not name both trees: "
                         f"{message[:200]}")
        elif not takes(real, dirty, None, engine=skew_reason):
            fails.append("--engine-skew-is-fine did not let the dirty page through")

        if ENGINE_CALL not in source:
            fails.append(f"the engine refusal ({ENGINE_CALL!r}) is not in train_blocks.py; "
                         "the mutation cannot be made")
        else:
            mutant = load(source.replace(ENGINE_CALL, "None"),
                            "train_blocks_engine_mutant")
            if not takes(mutant, old, None):
                fails.append("with the engine refusal taken out, the old page "
                             "is still stopped -- something else stops it, and this "
                             "test is not testing the refusal")

        # AND EVERY TRAINER ANSWERS TO THE TREE -- the glob, not a list of
        # names, so a trainer that lands tomorrow is covered tomorrow. A
        # trainer that reads no engine dump at all carries
        # TRAINING_RULES_NO_DUMP with its reason instead of the call.
        for trainer in sorted(LEARNED.glob("train_*.py")):
            if engine_guard_missing(trainer.read_text(encoding="utf-8")):
                fails.append(f"{trainer.name} neither calls "
                             "training_rules.same_engine nor carries "
                             "TRAINING_RULES_NO_DUMP: it trains on this tree's "
                             "dump or says why it reads no dump (#574)")
        # The check is itself mutated: the call taken out of a copy of a
        # trainer's source must be flagged -- and a fixture trainer that
        # never had the call must fail it too.
        if not engine_guard_missing(source.replace("same_engine(",
                                                   "same_engine_taken_out(")):
            fails.append("with the engine call taken out of a copy of the source, "
                         "the glob check still passes -- it is not checking the call")
        if not engine_guard_missing("def matrix(paths):\n    return read(paths)\n"):
            fails.append("a fixture trainer without the engine call passes the "
                         "glob check -- it cannot fail, so it proves nothing")
        if engine_guard_missing(source):
            fails.append("the block trainer's own source fails the glob check: "
                         "the check flags everything, so it proves nothing")
        if engine_guard_missing(f'{NO_DUMP_MARKER} = "trains on annotation words"\n'):
            fails.append("a fixture carrying TRAINING_RULES_NO_DUMP fails the glob "
                         "check: the marker branch is dead")

    manual = ["manual"] * 700 + ["other"] * 10
    kept = rules.capped(manual, [True] * len(manual), rules.PER_DOCUMENT_JUDGING)
    if sum(kept[:700]) != 500 or sum(kept[700:]) != 10:
        fails.append(f"the judging cap kept {sum(kept[:700])} of one document's 700 "
                     f"rows and {sum(kept[700:])} of another's 10; expected 500 and 10")
    if rules.capped(manual, [True] * len(manual), None) != [True] * len(manual):
        fails.append("no cap still dropped rows")

    lopsided = [f"big{i}" for i in range(5) for _ in range(30)] + \
               [f"small{i}" for i in range(100) for _ in range(3)]
    balanced = [f"doc{i}" for i in range(100) for _ in range(3)]
    if not refuses_split(rules, lopsided):
        fails.append("a half that five documents carry a third of was not refused")
    if refuses_split(rules, balanced):
        fails.append("a balanced half was refused")
    if refuses_split(rules, lopsided, allow=True):
        fails.append("--lopsided-is-fine did not let the lopsided half through")

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("training-rules: the block trainer refuses a measured split, its own output, "
          "another engine's dump -- unknown or dirty included -- and a lopsided half, "
          "and only each refusal refuses its own.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
