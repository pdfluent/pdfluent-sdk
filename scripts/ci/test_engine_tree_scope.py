#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""THE TREE is the segmentation plus the feature files, on both sides (#574).

`training_rules.tree_hash` and `crates/pdf-learned/build.rs` hash the same
pairs the same way: everything under `crates/pdf-engine/src` outside
`src/learned/`, plus the learned/ feature files both files list. And
`training_rules.decisions_hash` and build.rs hash everything under
`src/learned/`, models included: what the dumped model outputs depend on.
Seven checks on the real HEAD tree:

- a change in learned/mod.rs (decision code) leaves the tree equal;
- a change in learned/features.rs changes the tree;
- build.rs and training_rules.py list the same feature files;
- every `pdf_engine::learned::<module>` that crates/pdf-learned/src/lib.rs
  names is either listed or excluded with a written reason -- so a new
  feature file the dump calls must be added to the list, not pass silently;
- a change in learned/models/ changes the decisions but not the tree;
- a change in learned/mod.rs changes the decisions too (the whole of
  learned/ is in, not just the models);
- a change outside learned/ changes the tree but not the decisions.

That the two sides hash identically for this tree is held by
`the_stamp_is_the_serving_tree` and `the_decisions_stamp_is_the_serving_decisions`
in pdf-learned, which compare the stamped binary against `serving_tree()` and
`serving_decisions()`; hashing is compared there, scope here.
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
LIB_RS = REPO / "crates" / "pdf-learned" / "src" / "lib.rs"
ENGINE = "crates/pdf-engine/src"
OTHER_BLOB = "0" * 40


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


def build_rs_list() -> list[str]:
    text = BUILD_RS.read_text(encoding="utf-8")
    found = re.search(r"LEARNED_FEATURE_FILES[^=]*=\s*\[(.*?)\];", text, re.S)
    if not found:
        raise AssertionError("LEARNED_FEATURE_FILES is not in build.rs")
    return re.findall(r'"([^"]+)"', found.group(1))


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
    base = rules.tree_hash(pairs)
    base_decisions = rules.decisions_hash(pairs)

    decision = f"{ENGINE}/learned/mod.rs"
    if not any(p == decision for p, _ in pairs):
        fails.append(f"{decision} is not on this tree; the decision-code case is void")
    else:
        if rules.tree_hash(with_blob(pairs, decision, OTHER_BLOB)) != base:
            fails.append("a change in learned/mod.rs changed the hash: "
                         "decision code is out of the tree")
        if rules.decisions_hash(with_blob(pairs, decision, OTHER_BLOB)) == base_decisions:
            fails.append("a change in learned/mod.rs left the decisions equal: "
                         "the whole of learned/ is in the decisions")
    feature = f"{ENGINE}/learned/features.rs"
    if not any(p == feature for p, _ in pairs):
        fails.append(f"{feature} is not on this tree; the feature-file case is void")
    else:
        if rules.tree_hash(with_blob(pairs, feature, OTHER_BLOB)) == base:
            fails.append("a change in learned/features.rs left the hash equal: "
                         "feature files are in the tree")
    model = next((p for p, _ in pairs
                  if p.startswith(f"{ENGINE}/learned/models/")), None)
    if model is None:
        fails.append(f"no file under {ENGINE}/learned/models/ is on this tree; "
                     "the model-weights case is void")
    else:
        if rules.decisions_hash(with_blob(pairs, model, OTHER_BLOB)) == base_decisions:
            fails.append(f"a change in {model} left the decisions equal: "
                         "model weights are in the decisions")
        if rules.tree_hash(with_blob(pairs, model, OTHER_BLOB)) != base:
            fails.append(f"a change in {model} changed the tree: "
                         "model weights are out of the tree")
    outside = next((p for p, _ in pairs
                    if not p.startswith(f"{ENGINE}/learned/")), None)
    if outside is None:
        fails.append(f"no file outside {ENGINE}/learned/ is on this tree; "
                     "the segmentation case is void")
    else:
        if rules.tree_hash(with_blob(pairs, outside, OTHER_BLOB)) == base:
            fails.append(f"a change in {outside} left the tree equal: "
                         "the segmentation is in the tree")
        if rules.decisions_hash(with_blob(pairs, outside, OTHER_BLOB)) != base_decisions:
            fails.append(f"a change in {outside} changed the decisions: "
                         "the segmentation is out of the decisions")

    try:
        rust = sorted(build_rs_list())
    except AssertionError as e:
        fails.append(str(e))
        rust = []
    python = sorted(rules.LEARNED_FEATURE_FILES)
    if rust != python:
        fails.append("build.rs and training_rules.py list different feature files: "
                     f"rust={rust} python={python}")

    names = set(re.findall(r"learned::([A-Za-z_][A-Za-z0-9_]*)",
                           LIB_RS.read_text(encoding="utf-8")))
    if not names:
        fails.append("no learned:: names in pdf-learned/src/lib.rs; the coverage case is void")
    for name in sorted(names):
        if f"{name}.rs" in rules.LEARNED_FEATURE_FILES:
            continue
        if name in rules.LEARNED_EXCLUDED and rules.LEARNED_EXCLUDED[name].strip():
            continue
        fails.append(f"pdf-learned/src/lib.rs names pdf_engine::learned::{name}, "
                     "which is neither a listed feature file nor excluded with "
                     "a written reason")
    for name in sorted(rules.LEARNED_EXCLUDED):
        if not re.search(rf"learned::{re.escape(name)}\b",
                         LIB_RS.read_text(encoding="utf-8")):
            fails.append(f"LEARNED_EXCLUDED names {name}, which lib.rs no longer "
                         "names: the reason is stale")

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("engine-tree: decision code leaves the tree equal, a feature file changes it, "
          "both sides list the same files, every learned module the dump names is "
          "listed or excluded with a reason, and the decisions move with learned/ "
          "alone -- models in, segmentation out.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
