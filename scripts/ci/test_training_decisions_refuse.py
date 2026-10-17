#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A stacked trainer refuses verdicts another tree's decisions wrote (#574).

`training_rules.same_decisions` is the stacked trainers' guard, with the same
rules as `same_engine`: the table judge trains ON the dumped `list_signal`,
so a decision-only landing changes its inputs without changing the tree. This
test holds the rule AND the trainer's use of it:

- `train_table_judge.load` refuses a page whose decisions are not the serving
  ones -- the message naming both hashes -- lets it through with
  `--decisions-skew-is-fine`, keeps the reason for the manifest, and takes a
  page from this tree's decisions;
- an "unknown" decisions hash is refused even where the serving decisions are
  "unknown" too, and a dirty decisions hash ("<serving>-dirty") is refused
  with both hashes named -- each let through by `--decisions-skew-is-fine`;
- a page from before the decisions guard (no "decisions" stamp) is refused
  the same way;
- `train_heading_lines.records` takes that same skewed page: a trainer that
  reads no dumped model output answers to the tree alone;
- every `train_*.py` that names a dumped verdict (`list_signal`,
  `line_class_probs`, `p_heading`, `p_heading_pass`) calls
  `training_rules.same_decisions` -- the glob, not a list of names, so a
  stacked trainer that lands tomorrow is covered tomorrow.

And two mutations: the decisions refusal is taken out of a copy of the judge,
and the skewed page then has to get through -- which proves the call in the
trainer is what stops it, and not something else in the file. The second takes
the decisions call out of a copy of a trainer's source and requires the glob
check to flag it -- and a fixture trainer that names a dumped verdict without
the call must fail it too, so the check cannot pass by never looking.

No lightgbm on the gate: the refusals happen while the pages are read, before
any model is fitted.
"""
from __future__ import annotations

import gzip
import importlib.util
import json
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
JUDGE = LEARNED / "train_table_judge.py"
DECISIONS_CALL = "training_rules.same_decisions(r, path, decisions_skew_is_fine)"
OTHER_DECISIONS = "1" * 64
OTHER_TREE = "0" * 64
STACKED_SIGNALS = ("list_signal", "line_class_probs", "p_heading", "p_heading_pass")


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


def page(name: str, tree: str | None, decisions: str | None) -> dict:
    """One judge page: `tree`/`decisions` name what segmented it and what
    wrote its verdicts -- None for a stamp the dump predates."""
    record = {"document": name, "source": "doclaynet/train",
              "blocks": [], "words": []}
    engine = {}
    if tree is not None:
        engine["tree"] = tree
    if decisions is not None:
        engine["decisions"] = decisions
    if engine:
        engine["commit"] = "test"
        record["engine"] = engine
    return record


def dump(tmp: pathlib.Path, name: str, record: dict) -> pathlib.Path:
    path = tmp / f"{name}.jsonl"
    path.write_text(json.dumps(record) + "\n", encoding="utf-8")
    return path


def takes(module, path: pathlib.Path, engine: str | None = None,
          decisions: str | None = None) -> bool:
    try:
        module.load(str(path), 10, 20260928,
                    engine_skew_is_fine=engine,
                    decisions_skew_is_fine=decisions)
        return True
    except SystemExit:
        return False


def decisions_guard_missing(source: str) -> bool:
    """The glob rule: a trainer that names a dumped verdict calls
    `same_decisions`. The check is textual and a mention anywhere counts,
    comments included: name one of these values and the test holds you to
    the call, so a stacked trainer cannot read them without the guard."""
    if not any(signal in source for signal in STACKED_SIGNALS):
        return False
    return "same_decisions(" not in source


def refusal(module, path: pathlib.Path, engine: str | None = None,
            decisions: str | None = None) -> str:
    """The refusal message, or "" when the page went through."""
    try:
        module.load(str(path), 10, 20260928,
                    engine_skew_is_fine=engine,
                    decisions_skew_is_fine=decisions)
        return ""
    except SystemExit as e:
        return str(e)


def main() -> int:
    sys.path.insert(0, str(LEARNED))
    import training_rules as rules
    import train_heading_lines as heading

    source = JUDGE.read_text(encoding="utf-8")
    real = load(source, "train_table_judge_real")
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, decisions = rules.serving_tree(), rules.serving_decisions()
        current = dump(tmp, "current", page("current", tree, decisions))
        skewed = dump(tmp, "skewed", page("skewed", tree, OTHER_DECISIONS))
        reason = "re-measuring the floor on last week's dump (#574)"
        if not takes(real, current):
            fails.append("a page from this tree's decisions was refused")
        message = refusal(real, skewed)
        if not message:
            fails.append("a page whose verdicts another tree's decisions wrote "
                         "was trained on")
        else:
            if OTHER_DECISIONS not in message or decisions not in message:
                fails.append("the decisions refusal does not name both hashes: "
                             f"{message[:200]}")
            if "the verdicts of this dump are not those of the decisions that will read the model" not in message:
                fails.append("the decisions refusal does not say whose verdicts these are: "
                             f"{message[:200]}")
        if not takes(real, skewed, decisions=reason):
            fails.append("--decisions-skew-is-fine with a reason did not let "
                         "the skewed page through")
        kept = rules.decisions_skew_manifest()
        if kept is None or kept.get("reason") != reason:
            fails.append("the override reason is not what the manifest would record")
        if '"decisions_skew"' not in source:
            fails.append("train_table_judge.py never writes decisions_skew to its manifest")

        old = dump(tmp, "old", page("old", tree, None))
        if takes(real, old):
            fails.append("a page from before the decisions guard was trained on")
        if not takes(real, old, decisions=reason):
            fails.append("--decisions-skew-is-fine did not let the unstamped page through")

        saved, rules._SERVING_DECISIONS = rules._SERVING_DECISIONS, "unknown"
        try:
            ghost = dump(tmp, "ghost", page("ghost", tree, "unknown"))
            if takes(real, ghost):
                fails.append('an "unknown" decisions hash was trained on an "unknown" '
                             "serving decisions: unknown agrees with nothing")
            if not takes(real, ghost, decisions=reason):
                fails.append('--decisions-skew-is-fine did not let the "unknown" '
                             "page through")
        finally:
            rules._SERVING_DECISIONS = saved
        dirty = dump(tmp, "dirty", page("dirty", tree, decisions + "-dirty"))
        message = refusal(real, dirty)
        if not message:
            fails.append("a page from a dirty decisions hash was trained on")
        elif decisions not in message or decisions + "-dirty" not in message:
            fails.append("the dirty-decisions refusal does not name both hashes: "
                         f"{message[:200]}")
        elif not takes(real, dirty, decisions=reason):
            fails.append("--decisions-skew-is-fine did not let the dirty page through")

        if DECISIONS_CALL not in source:
            fails.append(f"the decisions refusal ({DECISIONS_CALL!r}) is not in "
                         "train_table_judge.py; the mutation cannot be made")
        else:
            mutant = load(source.replace(DECISIONS_CALL, "None"),
                            "train_table_judge_mutant")
            if not takes(mutant, skewed):
                fails.append("with the decisions refusal taken out, the skewed page "
                             "is still stopped -- something else stops it, and this "
                             "test is not testing the refusal")

        # AND EVERY STACKED TRAINER ANSWERS TO THE DECISIONS -- the glob, not
        # a list of names, so a stacked trainer that lands tomorrow is covered
        # tomorrow. A trainer that names no dumped verdict answers to the tree
        # alone, as the heading case below holds.
        for trainer in sorted(LEARNED.glob("train_*.py")):
            if decisions_guard_missing(trainer.read_text(encoding="utf-8")):
                fails.append(f"{trainer.name} names a dumped verdict but never calls "
                             "training_rules.same_decisions: a decision-only landing "
                             "changes its inputs without changing the tree (#574)")
        # The check is itself mutated: the call taken out of a copy of a
        # trainer's source must be flagged -- and a fixture trainer that names
        # a dumped verdict without the call must fail it too.
        if not decisions_guard_missing(source.replace("same_decisions(",
                                                       "same_decisions_taken_out(")):
            fails.append("with the decisions call taken out of a copy of the source, "
                         "the glob check still passes -- it is not checking the call")
        if not decisions_guard_missing("x = r.get('p_heading_pass') or 0.0\n"):
            fails.append("a fixture trainer naming a dumped verdict without the "
                         "decisions call passes the glob check -- it cannot fail, "
                         "so it proves nothing")
        if decisions_guard_missing(source):
            fails.append("the judge's own source fails the glob check: "
                         "the check flags everything, so it proves nothing")
        heading_source = (LEARNED / "train_heading_lines.py").read_text(encoding="utf-8")
        if decisions_guard_missing(heading_source):
            fails.append("the heading trainer's source fails the glob check: it names "
                         "no dumped verdict and must answer to the tree alone")

        # AND THE TRAINER THAT READS NO DUMPED OUTPUT answers to the tree
        # alone: the same skewed page goes through `train_heading_lines`.
        bundle = tmp / "train-x.jsonl.gz"
        with gzip.open(bundle, "wt", encoding="utf-8") as f:
            f.write(json.dumps({
                "page_id": "skewed", "document": "skewed",
                "source": "doclaynet/train", "refine": "off",
                "pdf_sha256": "skewed", "blocks": [], "words": [],
                "engine": {"tree": tree, "decisions": OTHER_DECISIONS,
                           "commit": "test"},
            }) + "\n")
        try:
            got = list(heading.records(tmp, None, None))
        except SystemExit as e:
            fails.append("train_heading_lines refused a page whose decisions differ: "
                         f"{e}")
            got = []
        if not got:
            fails.append("train_heading_lines read no page from the skewed bundle")
        # The control: another TREE is still refused, so the case above is
        # the decisions guard's absence and not a harness that never refuses.
        # Read alone, because the skewed-decisions bundle still sits beside it.
        skewed_tree_only = tmp / "only-tree"
        skewed_tree_only.mkdir()
        with gzip.open(skewed_tree_only / "train-y.jsonl.gz", "wt",
                       encoding="utf-8") as f:
            f.write(json.dumps({
                "page_id": "other-tree", "document": "other-tree",
                "source": "doclaynet/train", "refine": "off",
                "pdf_sha256": "other-tree", "blocks": [], "words": [],
                "engine": {"tree": OTHER_TREE, "decisions": decisions,
                           "commit": "test"},
            }) + "\n")
        try:
            list(heading.records(skewed_tree_only, None, None))
            fails.append("train_heading_lines trained on a page another tree segmented: "
                         "the control is void, and so is the case above")
        except SystemExit:
            pass

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("stacked-training: the table judge refuses another decisions' verdicts -- "
          "unknown, dirty or unstamped included -- and only each refusal refuses "
          "its own; the heading trainer answers to the tree alone.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
