#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A trainer reuses the dump across a retrain of its own model (#574).

Retraining `table_is` changes the serving decisions, which would refuse the
dump the retrain itself needs -- for values the retrain cannot have touched,
because the model's own output reaches no refine-off record its trainer reads.
So the record stamps `decisions_without`, one hash per self-excludable model
over the decisions minus that model's directory, and the trainer compares that
hash instead. This test holds the exclusion AND its refusals:

- a byte under `models/table_is/` changes `decisions` but not
  `decisions_without["table_is"]`; a byte under `models/heading_lines/`
  changes both;
- build.rs and training_rules.py hold the same SELF_EXCLUDABLE map;
- a record whose full decisions skewed but whose without-hash matches passes
  with `exclude="table_is"` -- and is refused without it, which proves the
  pass came from the exclusion and not a lax harness;
- `exclude="heading_lines"` is REFUSED: `heading_lines` produces
  `p_heading_pass`, which that trainer reads -- an exclusion for a model whose
  output feeds its features;
- `exclude="table_is"` on a refined record is REFUSED: its blocks are
  decisions the model already touched;
- a record from before the stamp (no `decisions_without`) falls back to the
  full check: refused on skew as today, let through by the existing override;
- every `exclude=` call site in a trainer is sound -- the model produces none
  of the fields that trainer reads -- and the soundness check itself flags
  (`train_table_is.py`, `heading_lines`), so it cannot pass by never looking.

That the Rust stamp equals `serving_decisions_without()` for this tree is held
by `the_decisions_without_stamp_is_the_serving_decisions_without` in
pdf-learned, the way `the_decisions_stamp_is_the_serving_decisions` holds the
full decisions; scope is held here.
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402
LEARNED = REPO / "scripts" / "extraction" / "learned"
BUILD_RS = REPO / "crates" / "pdf-learned" / "build.rs"
ENGINE = "crates/pdf-engine/src"
LEARNED_MODELS = f"{ENGINE}/learned/models"
OTHER_BLOB = "0" * 40
OTHER_DECISIONS = "1" * 64


def ls_tree() -> list[tuple[str, str]]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-tree", "-r", "HEAD", ENGINE],
        capture_output=True, text=True, check=True, env=sealed_env())
    pairs = []
    for line in out.stdout.splitlines():
        meta, _, path = line.partition("\t")
        fields = meta.split()
        if len(fields) == 3 and fields[1] == "blob" and path:
            pairs.append((path, fields[2]))
    return pairs


def with_blob(pairs: list[tuple[str, str]], path: str, blob: str):
    return [(p, blob if p == path else b) for p, b in pairs]


def build_rs_map() -> dict[str, str]:
    text = BUILD_RS.read_text(encoding="utf-8")
    found = re.search(r"const SELF_EXCLUDABLE[^=]*=\s*\[(.*?)\];", text, re.S)
    if not found:
        raise AssertionError("SELF_EXCLUDABLE is not in build.rs")
    return dict(re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', found.group(1)))


def record(tree: str | None, decisions: str | None,
           without: dict[str, str] | None, refine: str = "off") -> dict:
    """One stacked-training record: `None` stamps are a dump that predates them."""
    page: dict = {"document": "test", "source": "doclaynet/train",
                  "refine": refine, "blocks": [], "words": []}
    engine: dict = {}
    if tree is not None:
        engine["tree"] = tree
    if decisions is not None:
        engine["decisions"] = decisions
    if without is not None:
        engine["decisions_without"] = without
    if engine:
        engine["commit"] = "test"
        page["engine"] = engine
    return page


def takes(rules, page: dict, decisions: str | None = None,
          exclude: str | None = None) -> bool:
    try:
        rules.same_decisions(page, "test", decisions, exclude=exclude)
        return True
    except SystemExit:
        return False


def refusal(rules, page: dict, decisions: str | None = None,
            exclude: str | None = None) -> str:
    try:
        rules.same_decisions(page, "test", decisions, exclude=exclude)
        return ""
    except SystemExit as e:
        return str(e)


def main() -> int:
    sys.path.insert(0, str(LEARNED))
    import training_rules as rules

    fails = []
    try:
        pairs = ls_tree()
    except subprocess.CalledProcessError as e:
        print(f"FAIL: git ls-tree failed: {e}", file=sys.stderr)
        return 1
    if not pairs:
        print("FAIL: git ls-tree returned no pairs", file=sys.stderr)
        return 1
    base = rules.decisions_hash(pairs)
    base_without = rules.decisions_without_hash(pairs, "table_is")

    own = next((p for p, _ in pairs
                if p.startswith(f"{LEARNED_MODELS}/table_is/")), None)
    if own is None:
        fails.append(f"no file under {LEARNED_MODELS}/table_is/ is on this tree; "
                     "the self-exclusion case is void")
    else:
        moved = with_blob(pairs, own, OTHER_BLOB)
        if rules.decisions_hash(moved) == base:
            fails.append(f"a change in {own} left the decisions equal: "
                         "the model's files are in the decisions")
        if rules.decisions_without_hash(moved, "table_is") != base_without:
            fails.append(f"a change in {own} changed decisions_without: "
                         "the model's own files are out of its without-hash")
    other = next((p for p, _ in pairs
                  if p.startswith(f"{LEARNED_MODELS}/heading_lines/")), None)
    if other is None:
        fails.append(f"no file under {LEARNED_MODELS}/heading_lines/ is on this "
                     "tree; the other-model case is void")
    else:
        moved = with_blob(pairs, other, OTHER_BLOB)
        if rules.decisions_hash(moved) == base:
            fails.append(f"a change in {other} left the decisions equal: "
                         "the control is void")
        if rules.decisions_without_hash(moved, "table_is") == base_without:
            fails.append(f"a change in {other} left decisions_without equal: "
                         "another model's files are in the without-hash")

    try:
        rust = build_rs_map()
    except AssertionError as e:
        fails.append(str(e))
        rust = {}
    python = dict(rules.SELF_EXCLUDABLE)
    if rust != python:
        fails.append("build.rs and training_rules.py hold different "
                     f"SELF_EXCLUDABLE maps: rust={rust} python={python}")
    for name, directory in python.items():
        if not (REPO / ENGINE / "learned" / "models" / directory).is_dir():
            fails.append(f"SELF_EXCLUDABLE[{name!r}] names {directory!r}, which "
                         "is not a directory under learned/models/: the entry "
                         "does not name what the model ships in")
    for field, producers in rules.FIELD_PRODUCERS.items():
        for model in producers:
            if not (REPO / ENGINE / "learned" / "models" / model).is_dir():
                fails.append(f"FIELD_PRODUCERS[{field!r}] names {model!r}, which "
                             "is not a directory under learned/models/")
    for trainer in rules.TRAINER_READS:
        if not (LEARNED / trainer).is_file():
            fails.append(f"TRAINER_READS names {trainer}, which is no trainer "
                         "in scripts/extraction/learned/: the entry is stale")

    # EVERY EXCLUSION A TRAINER ASKS FOR IS SOUND: the excluded model produces
    # none of the fields that trainer reads. The check is textual -- a trainer
    # that passes `exclude=` without an entry in TRAINER_READS fails it, so an
    # undeclared read cannot pass silently.
    asked: dict[str, set[str]] = {}
    for trainer in sorted(LEARNED.glob("train_*.py")):
        for name in re.findall(r'exclude\s*=\s*"([^"]+)"', trainer.read_text()):
            asked.setdefault(trainer.name, set()).add(name)
    if not asked:
        fails.append("no trainer passes exclude=: the soundness case is void")
    for trainer, names in sorted(asked.items()):
        if trainer not in rules.TRAINER_READS:
            fails.append(f"{trainer} passes exclude= but TRAINER_READS names "
                         "no fields it reads: the exclusion is undeclared")
            continue
        for name in sorted(names):
            if name not in rules.SELF_EXCLUDABLE:
                fails.append(f"{trainer} passes exclude={name!r}, which is not "
                             "in SELF_EXCLUDABLE")
            elif not rules.exclusion_is_sound(trainer, name):
                reads = rules.TRAINER_READS[trainer]
                fed = [f for f in reads
                       if name in rules.FIELD_PRODUCERS.get(f, [])]
                fails.append(f"{trainer} passes exclude={name!r}, but {name} "
                             f"produces {fed}, which that trainer reads: the "
                             "exclusion is unsound")
    # The check can fail: `heading_lines` produces `p_heading_pass`, which the
    # is-table trainer reads, so the pair is unsound -- and an unknown trainer
    # or field is unsound too, not silently sound.
    if rules.exclusion_is_sound("train_table_is.py", "heading_lines"):
        fails.append("exclusion_is_sound says heading_lines is sound for "
                     "train_table_is.py, which reads p_heading_pass that it "
                     "produces: the check cannot fail, so it proves nothing")
    if not rules.exclusion_is_sound("train_table_is.py", "table_is"):
        fails.append("exclusion_is_sound says table_is is unsound for its own "
                     "trainer: the exclusion has no declared basis")
    if rules.exclusion_is_sound("train_no_such_trainer.py", "table_is"):
        fails.append("exclusion_is_sound says an unlisted trainer is sound: "
                     "undeclared reads pass silently")
    if "train_table_is.py" not in asked:
        fails.append('train_table_is.py passes no exclude=: the retrain still '
                     "compares the full decisions")
    elif "table_is" not in asked["train_table_is.py"]:
        fails.append("train_table_is.py excludes no table_is: the retrain still "
                     "compares the full decisions")

    tree, decisions = rules.serving_tree(), rules.serving_decisions()
    without = rules.serving_decisions_without("table_is")
    reason = "re-measuring the floor on last week's dump (#574)"

    # THE RETRAIN'S OWN CASE: the full decisions skewed -- the retrain changed
    # them -- but the without-hash matches, and the page goes through. Without
    # the exclusion the same page is refused, which proves the pass came from
    # the exclusion and not a harness that never refuses.
    retrained = record(tree, OTHER_DECISIONS, {"table_is": without})
    if not takes(rules, retrained, exclude="table_is"):
        fails.append("a page whose decisions skewed only by the model's own "
                     "retrain was refused with exclude=: the retrain re-dumps")
    if takes(rules, retrained):
        fails.append("a page from another decisions went through WITHOUT the "
                     "exclusion: the control is void, and so is the case above")
    message = refusal(rules, record(tree, OTHER_DECISIONS, {"table_is": "2" * 64}),
                      exclude="table_is")
    if not message:
        fails.append("a page whose decisions-without skewed was trained on")
    elif without not in message or "2" * 64 not in message:
        fails.append("the without refusal does not name both hashes: "
                     f"{message[:200]}")
    saved, warned = rules._DECISIONS_SKEW, rules._DECISIONS_SKEW_WARNED
    rules._DECISIONS_SKEW, rules._DECISIONS_SKEW_WARNED = None, False
    try:
        skewed = record(tree, OTHER_DECISIONS, {"table_is": "2" * 64})
        if not takes(rules, skewed, decisions=reason, exclude="table_is"):
            fails.append("--decisions-skew-is-fine did not let the "
                         "without-skewed page through")
        kept = rules.decisions_skew_manifest()
        if kept is None or kept.get("reason") != reason:
            fails.append("the override reason is not what the manifest would "
                         "record under an exclusion")
        if kept is not None and kept.get("exclude") != "table_is":
            fails.append("the manifest does not say the comparison excluded "
                         f"table_is: {kept}")
    finally:
        rules._DECISIONS_SKEW, rules._DECISIONS_SKEW_WARNED = saved, warned

    # AN EXCLUSION FOR A MODEL WHOSE OUTPUT FEEDS ITS FEATURES IS REFUSED.
    # `heading_lines` produces `p_heading_pass`, which the is-table trainer
    # reads -- so it is not self-excludable, and asking is refused even where
    # the hashes would match.
    message = refusal(rules, retrained, exclude="heading_lines")
    if not message:
        fails.append('exclude="heading_lines" went through: a model whose '
                     "output feeds the trainer's features was excluded")
    elif "not self-excludable" not in message or "heading_lines" not in message:
        fails.append("the refusal does not name the reason: "
                     f"{message[:200]}")

    # A REFINED RECORD IS REFUSED: its blocks are decisions the excluded model
    # already touched.
    message = refusal(
        rules, record(tree, decisions, {"table_is": without}, refine="lists+tables"),
        exclude="table_is")
    if not message:
        fails.append("a refined record went through with exclude=: the model's "
                     "own decisions would train it")
    elif "learned pass ON" not in message:
        fails.append("the refined refusal does not name the reason: "
                     f"{message[:200]}")

    # A DUMP FROM BEFORE THE STAMP falls back to the full check: refused on
    # skew, as today -- and let through by the existing override, or where the
    # full decisions match.
    old = record(tree, OTHER_DECISIONS, None)
    if takes(rules, old, exclude="table_is"):
        fails.append("a page from before the without stamp went through with "
                     "exclude=: the fallback is not the full check")
    if not takes(rules, old, decisions=reason, exclude="table_is"):
        fails.append("--decisions-skew-is-fine did not let the unstamped page "
                     "through under an exclusion")
    if not takes(rules, record(tree, decisions, None), exclude="table_is"):
        fails.append("an unstamped page from this tree's decisions was refused "
                     "with exclude=: the fallback refuses what the full check "
                     "takes")
    if not takes(rules, record(tree, decisions, {"table_is": without}),
                 exclude="table_is"):
        fails.append("a stamped page from this tree went through nowhere: the "
                     "exclusion refuses its own tree")

    saved_without = dict(rules._SERVING_DECISIONS_WITHOUT)
    try:
        rules._SERVING_DECISIONS_WITHOUT["table_is"] = "unknown"
        if takes(rules, record(tree, decisions, {"table_is": "unknown"}),
                 exclude="table_is"):
            fails.append('an "unknown" without-hash was trained on an "unknown" '
                         "serving without: unknown agrees with nothing")
    finally:
        rules._SERVING_DECISIONS_WITHOUT.clear()
        rules._SERVING_DECISIONS_WITHOUT.update(saved_without)
    if takes(rules, record(tree, decisions, {"table_is": without + "-dirty"}),
             exclude="table_is"):
        fails.append("a page from a dirty without-hash was trained on")

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("self-exclusion: a retrain reuses its dump minus its own directory -- "
          "refused for a model that feeds its features, for a refined record, "
          "and for a without skew; unstamped dumps fall back to the full check.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
