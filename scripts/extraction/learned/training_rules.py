#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What every trainer here refuses, in one place (#602).

Five rules, each of which was learnt by breaking it:

WHICH SPLIT. The features directory holds shards of every split and
`train-*.jsonl.gz` is one character away from `*.jsonl.gz`. One glob trained a
model on 2 781 validation pages, 696 of which were in the sample the model was
then compared on -- and nothing failed. The numbers simply came out flattering,
and the feature under test was document-scoped, which is exactly what that
flatters.

WHICH DUMP. A dump made with the learned pass ON carries the models' own
decisions as block kinds, so a model trained on it learns its own output. The
dump says so and this refuses it.

WHOSE DOCUMENTS. One MySQL manual was 34.1 per cent of the half that chooses
when training stops and where the threshold goes; the top five documents were
43.7 per cent of it. T5 found the same document at 51 per cent of their list
items. So no document may speak for more than its share: the rows are capped
per document, and after the cap the trainer REFUSES to run if the top five still
carry more than a fifth of a split. The cap is the fix and the refusal is what
says the fix worked -- a cap with a bug in it would otherwise be invisible.

WHICH ENGINE. The blocks of a dump are one tree's segmentation verdicts, and a
model trained on them is read by the serving tree's. When the two trees differ
the model learns blocks the engine will never show it -- a train/serving skew
that reads as a weak model and never says so. So every dump record names the
engine tree that segmented it, and a record from another tree is refused unless
`--engine-skew-is-fine` gives a reason, which the manifest keeps (#574).

WHICH DECISIONS. A stacked model trains not on blocks but on another model's
dumped verdicts -- the table judge on `list_signal`, the table-or-not model on
`p_heading_pass`. A landing that touches only models or decisions changes those
values without changing the tree, which is the same skew one level up and
silent for the same reason. So every dump record also names the decisions that
wrote its verdicts, and a stacked trainer refuses a record whose decisions are
not the serving ones unless `--decisions-skew-is-fine` gives a reason, which
the manifest keeps beside the engine's (#574).

WHOSE DECISIONS, MINUS THE MODEL'S OWN. A retrain of `table_is` changes the
serving decisions, which would refuse the dump the retrain itself needs -- a
re-dump for a change that cannot have touched a dumped value, because the
model's own output reaches no refine-off record its trainer reads. So the
record also names `decisions_without`, one hash per self-excludable model over
the decisions minus that model's directory, and a trainer whose exclusion is
sound compares that hash instead (#574).
"""
from __future__ import annotations

import collections
import hashlib
import pathlib
import subprocess
import sys

# At most this many labelled rows from any one document in the JUDGING half --
# T5's number, because a defect measured twice in two lanes should not be fixed
# two ways.
PER_DOCUMENT_JUDGING = 500

# AND NO CAP ON TRAINING, because the measurement said so. The judging half was
# 43.7 per cent five documents and one manual was 34.1 of it; the TRAINING half
# of the same split was 5.8 per cent, which is no concentration at all. Capping
# it anyway cost 420 000 of 1 467 000 rows -- 29 per cent of the corpus, taken
# from the largest documents, which are large because they are long and not
# because they are over-represented -- and the heading axis fell 0.0119 for it
# (#602). The refusal below is what decides whether training needs a cap; it is
# not assumed.
PER_DOCUMENT = None

# Above this share for the top five documents, a split is one document's opinion.
LOPSIDED = 0.20
HOW_MANY_AT_THE_TOP = 5


def only_the_train_split(record: dict, where: str, second_generation: str | None) -> None:
    """Refuse a page from a split the axis is measured on."""
    source = str(record.get("source") or "")
    if source and "/train" not in source and not second_generation:
        raise SystemExit(
            f"{where}: a page from {source!r}. A trainer reads the TRAIN split; "
            "validation and test are what the axis is measured on, and a model "
            "that has seen them cannot be compared with one that has not. Pass "
            "only train shards, or --second-generation with a reason.")


def not_its_own_output(record: dict, where: str, second_generation: str | None) -> None:
    """Refuse a dump made with the learned pass on."""
    said = str(record.get("refine") or "off").strip()
    if said != "off" and not second_generation:
        raise SystemExit(
            f"{where}: a record dumped with the learned pass ON ({said!r}). Its "
            "kinds are the models' own decisions; training on them teaches a "
            "model its own output. Dump with refine off, or pass "
            "--second-generation with a reason.")


# THE ENGINE TREE (#574). sha256 over the sorted (path, blob-sha) pairs of
# `git ls-tree -r HEAD crates/pdf-engine/src`, filtered to what a dump's
# values depend on: everything outside src/learned/ (the segmentation), plus
# the feature files below. The dump runs with the learned pass OFF, but it
# still writes values computed in learned/ -- block features, line vectors,
# table pairs, style, drawings -- so those files are in and the decision code
# (mod.rs, the models, the judges) is out: a decision-only landing changes no
# dumped value and reuses old dumps. One pair per entry, `path\0blob\0`,
# paths in byte order -- that byte layout is the contract with build.rs.
LEARNED_FEATURE_FILES = (
    "alignment.rs",
    "drawings.rs",  # Not on this tree yet; listed so its landing changes the hash.
    "features.rs",
    "lines.rs",
    "pairs.rs",
    # The image arm (/12). Its values also read the renderer
    # (crates/pdf-render/src, through render_page), which this tree does not
    # hash: a renderer-only landing shifts line_image without changing the
    # stamp. Widening the tree re-stamps every dump, so it is a decision for
    # the training lanes, not a side effect of this file (#574).
    "raster.rs",
    "splits.rs",
    "style.rs",  # Not on this tree yet either.
    "vector_proposals.rs",
)

# What `pdf_engine::learned::<name>` in crates/pdf-learned/src/lib.rs is NOT a
# feature file, and why it stays out. test_engine_tree_scope.py refuses a name
# in neither this map nor the list: a new feature module the dump calls must
# be listed, not pass silently.
LEARNED_EXCLUDED = {
    # In learned/mod.rs, the decision module the tree excludes: orchestration
    # over the features above plus model weights, so a decision-only landing
    # reuses the corpus. Two of them are measurement-only besides.
    "Switches": "decision module; measurement-only (only dump_pdf_refined takes it)",
    "refine_with_images": "decision module; measurement-only (see Switches)",
    "document_facts": "decision module; facts over the blocks beside it",
    "heading_probabilities": "decision module; the dumped p_heading verdict",
    "line_class_probs": "decision module; the dumped line verdicts",
    "list_signal": "decision module; the dumped list verdict",
    "cell_hints": "decision module; the dumped heading_pass/p_heading_pass verdict",
    "tables": "decision module; the dumped judge verdict (judge_regions, REJECTED), its CellHint record, and the final-table names",
    "SWITCHES": "decision module; the shipped switch set the dump reads the judge's verdict with",
    "PageConfidence": "decision module; the dumped page doubt (/17), a verdict of the pass",
    "page_confidence": "decision module; the dumped page doubt (/17), a verdict of the pass",
    "page_confidence_with_switches": "decision module; the page doubt under named switches, a verdict",
    "final_tables_for_dump": "decision module; the dumped final tables, a verdict of the pass",
}

_ENGINE_PREFIX = "crates/pdf-engine/src/"
_ENGINE_LEARNED = _ENGINE_PREFIX + "learned/"


def _tree_keeps(path: str) -> bool:
    """Whether repo-relative `path`, as ls-tree prints it, is in the tree."""
    if not path.startswith(_ENGINE_PREFIX):
        return False
    if path.startswith(_ENGINE_LEARNED):
        rest = path[len(_ENGINE_LEARNED):]
        return "/" not in rest and rest in LEARNED_FEATURE_FILES
    return True


def _hash_over(pairs: list[tuple[str, str]], keeps) -> str:
    """sha256 over `path\\0blob\\0` per included pair, paths in byte order."""
    kept = sorted((p, b) for p, b in pairs if keeps(p))
    h = hashlib.sha256()
    for path, blob in kept:
        h.update(path.encode("utf-8"))
        h.update(b"\0")
        h.update(blob.encode("utf-8"))
        h.update(b"\0")
    return h.hexdigest()


def tree_hash(pairs: list[tuple[str, str]]) -> str:
    """THE TREE for `git ls-tree` (path, blob-sha) pairs: sha256 over
    `path\\0blob\\0` per included pair, paths in byte order. build.rs
    (crates/pdf-learned) hashes the same bytes; test_engine_tree_scope.py
    holds the two lists and the two hashes equal."""
    return _hash_over(pairs, _tree_keeps)


# THE DECISIONS (#574). The same sha256 layout over everything under
# `crates/pdf-engine/src/learned/`, models included: what the dumped MODEL
# outputs (`list_signal`, `line_class_probs`, `p_heading`) depend on. A
# decision-only landing changes no block and reuses old dumps for the block
# trainers -- but it changes these values, so the stacked trainers refuse a
# record whose decisions are not the serving ones.
def _decisions_keeps(path: str) -> bool:
    """Whether repo-relative `path`, as ls-tree prints it, is in the decisions."""
    return path.startswith(_ENGINE_LEARNED)


def decisions_hash(pairs: list[tuple[str, str]]) -> str:
    """THE DECISIONS for `git ls-tree` (path, blob-sha) pairs: sha256 over
    `path\\0blob\\0` per pair under `src/learned/`, paths in byte order.
    build.rs hashes the same bytes; the pdf-learned stamp test holds the two
    hashes equal for this tree."""
    return _hash_over(pairs, _decisions_keeps)


# SELF-EXCLUDABLE MODELS (#574). A model a trainer may exclude from the
# decisions it is compared against: retraining it then reuses the dump the
# retrain itself needs. Maps the model's name -- the key of
# `record["engine"]["decisions_without"]` and of `same_decisions(exclude=...)`
# -- to the directory it actually ships in under
# `crates/pdf-engine/src/learned/models/`. One place, with the reads and the
# producers below; build.rs (crates/pdf-learned) holds the same map, and
# test_self_exclusion.py refuses drift between the two.
SELF_EXCLUDABLE = {
    "table_is": "table_is",
}

# THE DUMP FIELDS EACH STACKED TRAINER READS: the record keys (top level, or
# per block) the trainer's features come from. Only trainers that call
# `same_decisions` are listed: a trainer that never compares decisions never
# excludes from them. From each trainer's own code, and the test refuses a
# trainer that passes `exclude=` without an entry here.
TRAINER_READS = {
    # `feats` reads the builder's `table_cell` blocks and each cell's
    # `p_heading_pass`; `tables_of` and `label` read the same blocks.
    "train_table_is.py": ["blocks", "p_heading_pass"],
    # `features` reads the blocks (through `table_proposals.page_lines`), the
    # dumped `table_proposals`, and each line's `list_signal` (`l["list"]`).
    "train_table_judge.py": ["blocks", "table_proposals", "list_signal"],
}

# THE MODELS THAT PRODUCE EACH DUMPED FIELD, by the directory they ship in
# under `crates/pdf-engine/src/learned/models/`. From the engine code that
# writes a refine-off record (`pdf_learned::dump_pdf`), not from names:
#
# - `blocks` come from `extract_blocks_and_rules_before_learning`, before any
#   model has run; `table_proposals` from the heuristic detectors
#   (`text::table_proposals`, `alignment`, `splits`, `vector_proposals`).
# - `p_heading_pass`/`heading_pass` come from `cell_hints` -> `decide_page` ->
#   `hint_of`: the heading branch only, which with the graph network on
#   averages `line_gnn`/`line_gnn_v2` with `heading_lines`, and without it
#   reads `heading_lines` alone. The list and caption verdicts `decide_page`
#   also computes never reach the hint.
# - `p_heading` is `heading_probabilities`: the line model alone.
# - `judged` is `judge_regions` -> `judge::verdict_traced`, which asks
#   `table_judge` whether and `verifier::choose_traced` (`table_verifier`)
#   which, over lines carrying `list_signal` (`list_item`, `list_opens`, and
#   the `line_classes` probabilities both read).
# - `confidence` is `page_confidence` over `decide_page_traced`: the heading
#   probabilities behind the decision plus the network's classes and links.
# - `line_class_probs` is `line_class_probs`:
#   `heading_lines::page_class_probabilities`, the `line_classes` model.
# - `list_signal` is `list_signal`: `list_item` and `list_opens`, over the
#   `line_classes` probabilities the block model reads.
#
# `caption_lines`, `table_region`, the boundary and bridge models and
# `table_is` itself produce no dumped field: the refine step reads them, and
# the dump runs with it off. A model may be self-excluded for a trainer only
# if it produces none of the fields that trainer reads.
FIELD_PRODUCERS = {
    "blocks": [],
    "table_proposals": [],
    "p_heading_pass": ["heading_lines", "line_gnn", "line_gnn_v2"],
    "heading_pass": ["heading_lines", "line_gnn", "line_gnn_v2"],
    "p_heading": ["heading_lines"],
    "judged": ["table_judge", "table_verifier", "list_item", "list_opens",
               "line_classes"],
    "confidence": ["heading_lines", "line_gnn", "line_gnn_v2"],
    "line_class_probs": ["line_classes"],
    "list_signal": ["list_item", "list_opens", "line_classes"],
}


def exclusion_is_sound(trainer: str, model: str) -> bool:
    """Whether `model`'s own output can reach no dumped value `trainer` reads.

    The declared form of the exclusion property: true when the model produces
    none of the fields the trainer reads. Unknown trainers and unknown fields
    are unsound -- a trainer that passes `exclude=` must be listed in
    `TRAINER_READS`, and every field it reads in `FIELD_PRODUCERS` -- so an
    undeclared read cannot pass silently. test_self_exclusion.py holds every
    `exclude=` call site to this.
    """
    reads = TRAINER_READS.get(trainer)
    if reads is None:
        return False
    for field in reads:
        producers = FIELD_PRODUCERS.get(field)
        if producers is None or model in producers:
            return False
    return True


def decisions_without_hash(pairs: list[tuple[str, str]], name: str) -> str:
    """THE DECISIONS minus one model's directory, for `git ls-tree` pairs.

    The same sha256 layout as `decisions_hash` over `src/learned/` minus
    `models/<dir>/`, where `<dir>` is what `name` ships in
    (`SELF_EXCLUDABLE`). build.rs stamps the same bytes per model; the
    pdf-learned stamp test holds them equal for this tree.
    """
    try:
        directory = SELF_EXCLUDABLE[name]
    except KeyError:
        raise ValueError(
            f"{name!r} is not self-excludable: SELF_EXCLUDABLE holds "
            f"{sorted(SELF_EXCLUDABLE)}") from None
    prefix = f"{_ENGINE_LEARNED}models/{directory}/"

    def keeps(path: str) -> bool:
        return path.startswith(_ENGINE_LEARNED) and not path.startswith(prefix)

    return _hash_over(pairs, keeps)


_SERVING_TREE: str | None = None
_SERVING_DECISIONS: str | None = None
_SERVING_DECISIONS_WITHOUT: dict[str, str] = {}
_ENGINE_SKEW: dict | None = None
_ENGINE_SKEW_WARNED = False
_DECISIONS_SKEW: dict | None = None
_DECISIONS_SKEW_WARNED = False


def _ls_tree() -> list[tuple[str, str]]:
    """The (path, blob-sha) pairs of `crates/pdf-engine/src` at HEAD."""
    root = pathlib.Path(__file__).resolve().parents[3]
    # `env=sealed_env()`: inside a git hook, GIT_DIR and GIT_WORK_TREE
    # point at another repository, and git then ignores the directory it
    # was pointed at. Same seal as the scripts/ci tests (gitenv gate).
    sys.path.insert(0, str(root / "scripts" / "ci"))
    from fixture_env import sealed_env
    out = subprocess.run(
        ["git", "-C", str(root), "ls-tree", "-r", "HEAD",
         "crates/pdf-engine/src"],
        capture_output=True, text=True, check=True, env=sealed_env())
    pairs = []
    for line in out.stdout.splitlines():
        # "100644 blob <sha>\t<path>"
        meta, _, path = line.partition("\t")
        fields = meta.split()
        if len(fields) == 3 and fields[1] == "blob" and path:
            pairs.append((path, fields[2]))
    if not pairs:
        raise ValueError("empty engine tree")
    return pairs


def serving_tree() -> str:
    """THE TREE of the checkout this runs in (see LEARNED_FEATURE_FILES).

    Asked once per run, not once per record: the trainers call `same_engine`
    for every page of every shard. "unknown" when git is missing or this is
    not a checkout, which is also what a binary built without git stamps. A
    dump from a dirty tree stamps "<tree>-dirty", which never equals this:
    re-dump from the commit, or override with a reason.
    """
    global _SERVING_TREE
    if _SERVING_TREE is None:
        try:
            _SERVING_TREE = tree_hash(_ls_tree())
        except Exception:
            # Not a checkout, or no git: the same "unknown" build.rs stamps.
            _SERVING_TREE = "unknown"
    return _SERVING_TREE


def serving_decisions() -> str:
    """THE DECISIONS of the checkout this runs in: what the dumped model
    outputs depend on. "unknown" where `serving_tree` is, for the same
    reason; a dump from a dirty learned/ stamps "<decisions>-dirty", which
    never equals this."""
    global _SERVING_DECISIONS
    if _SERVING_DECISIONS is None:
        try:
            _SERVING_DECISIONS = decisions_hash(_ls_tree())
        except Exception:
            _SERVING_DECISIONS = "unknown"
    return _SERVING_DECISIONS


def same_engine(record: dict, where: str, override: str | None) -> None:
    """Refuse a record segmented by another engine tree.

    A record without "engine" is every dump from before the guard, and it is
    refused like any other skew: its blocks are not known to be this tree's.
    An "unknown" tree on either side is refused the same way: it agrees with
    nothing, not even another "unknown".
    """
    global _ENGINE_SKEW, _ENGINE_SKEW_WARNED
    engine = record.get("engine")
    dump = engine.get("tree") if isinstance(engine, dict) else None
    serving = serving_tree()
    if dump == "unknown" or serving == "unknown":
        pass
    elif dump is not None and dump == serving:
        return
    if override:
        if not _ENGINE_SKEW_WARNED:
            print(f"[train] WARNING: training on engine tree {dump!r} while "
                  f"this tree serves {serving!r}: the blocks of this dump "
                  "are not those of the engine that will read the model. "
                  f"Reason: {override}")
            _ENGINE_SKEW_WARNED = True
        # The FIRST skew wins: one run, one reason, one pair of trees in the
        # manifest. A run over two skewed dumps is still refused nowhere and
        # warned once -- the manifest says which skew was let through first.
        if _ENGINE_SKEW is None:
            _ENGINE_SKEW = {"reason": override,
                            "dump": dump if dump is not None else "unknown",
                            "serving": serving}
        return
    said = dump if dump is not None else "none (the dump predates the engine guard)"
    raise SystemExit(
        f"{where}: a record from engine tree {said!r}, but this tree serves "
        f"{serving!r}. The segmentation changed under the dump: the blocks "
        "of this dump are not those of the engine that will read the model. "
        "Re-dump with this tree, or pass --engine-skew-is-fine with a reason.")


def engine_skew_manifest() -> dict | None:
    """What the manifest records under "engine_skew": the reason, the dump
    tree and the serving tree -- or None when the trees agreed and there is
    nothing to record."""
    return dict(_ENGINE_SKEW) if _ENGINE_SKEW is not None else None


def serving_decisions_without(name: str) -> str:
    """THE DECISIONS minus one model's directory, of this checkout.

    What `serving_decisions` is for the full `src/learned/`, for the tree
    minus `models/<dir>/` where `<dir>` is what `name` ships in. Asked once
    per run per model, like `serving_decisions`; "unknown" where it is, for
    the same reason.
    """
    if name not in _SERVING_DECISIONS_WITHOUT:
        try:
            _SERVING_DECISIONS_WITHOUT[name] = decisions_without_hash(
                _ls_tree(), name)
        except Exception:
            # Not a checkout, or no git: the same "unknown" build.rs stamps.
            # An unknown NAME raises ValueError out of `decisions_without_hash`
            # too, and lands here as well -- but `same_decisions` refuses such
            # a name before ever asking, so this path only serves the checkout.
            _SERVING_DECISIONS_WITHOUT[name] = "unknown"
    return _SERVING_DECISIONS_WITHOUT[name]


def same_decisions(record: dict, where: str, override: str | None,
                   exclude: str | None = None) -> None:
    """Refuse a record whose dumped verdicts another tree's decisions wrote.

    The stacked trainers' guard, with the same rules as `same_engine`: only a
    trainer that reads a dumped MODEL output calls it. A record without
    "decisions" is every dump from before the guard, and it is refused like
    any other skew: its verdicts are not known to be this tree's. An "unknown"
    on either side is refused the same way: it agrees with nothing, not even
    another "unknown".

    With `exclude=name`, the trainer compares the decisions WITHOUT the named
    model's own directory instead: a retrain of that model then reuses the
    dump its own retrain needs. The exclusion is refused -- SystemExit naming
    the reason -- when the name is not in `SELF_EXCLUDABLE`, or when the
    record was dumped with the learned pass on, whose blocks the model already
    touched. A record from before the stamp (no `decisions_without[name]`)
    falls back to the full check, as today.
    """
    global _DECISIONS_SKEW, _DECISIONS_SKEW_WARNED
    engine = record.get("engine")
    if exclude is not None and exclude not in SELF_EXCLUDABLE:
        raise SystemExit(
            f"{where}: cannot train excluding {exclude!r}: it is not "
            f"self-excludable (SELF_EXCLUDABLE holds "
            f"{sorted(SELF_EXCLUDABLE)}). A model may be excluded only when "
            "its own output cannot reach any dumped value its trainer reads; "
            "compare the full decisions, or pass --decisions-skew-is-fine "
            "with a reason.")
    if exclude is not None:
        said = str(record.get("refine") or "off").strip()
        if said != "off":
            raise SystemExit(
                f"{where}: cannot train excluding {exclude!r}: this record "
                f"was dumped with the learned pass ON ({said!r}), and its "
                "blocks are decisions the excluded model already touched. "
                "The exclusion holds only for refine-off records; re-dump "
                "with refine off, or pass --decisions-skew-is-fine with a "
                "reason.")
        without = engine.get("decisions_without") if isinstance(engine, dict) else None
        stamped = without.get(exclude) if isinstance(without, dict) else None
        if stamped is not None:
            serving = serving_decisions_without(exclude)
            if stamped != "unknown" and serving != "unknown" and stamped == serving:
                return
            if override:
                if not _DECISIONS_SKEW_WARNED:
                    print(f"[train] WARNING: training on decisions without "
                          f"{exclude} {stamped!r} while this tree serves "
                          f"{serving!r}: the dumped verdicts of this dump are "
                          "not those of the decisions that will read the model. "
                          f"Reason: {override}")
                    _DECISIONS_SKEW_WARNED = True
                # The FIRST skew wins: one run, one reason, one pair of hashes
                # in the manifest, as with the engine skew above -- plus which
                # model's directory was excluded from the comparison.
                if _DECISIONS_SKEW is None:
                    _DECISIONS_SKEW = {"reason": override,
                                       "dump": stamped,
                                       "serving": serving,
                                       "exclude": exclude}
                return
            raise SystemExit(
                f"{where}: a record from decisions without {exclude} "
                f"{stamped!r}, but this tree serves {serving!r}. The models "
                "changed under the dump: the verdicts of this dump are not "
                "those of the decisions that will read the model. Re-dump "
                "with this tree, or pass --decisions-skew-is-fine with a "
                "reason.")
        # No `decisions_without[name]`: a dump from before the stamp. The full
        # check below applies, as it does without an exclusion.
    dump = engine.get("decisions") if isinstance(engine, dict) else None
    serving = serving_decisions()
    if dump == "unknown" or serving == "unknown":
        pass
    elif dump is not None and dump == serving:
        return
    if override:
        if not _DECISIONS_SKEW_WARNED:
            print(f"[train] WARNING: training on decisions {dump!r} while "
                  f"this tree serves {serving!r}: the dumped verdicts of this "
                  "dump are not those of the decisions that will read the model. "
                  f"Reason: {override}")
            _DECISIONS_SKEW_WARNED = True
        # The FIRST skew wins: one run, one reason, one pair of hashes in the
        # manifest, as with the engine skew above.
        if _DECISIONS_SKEW is None:
            _DECISIONS_SKEW = {"reason": override,
                               "dump": dump if dump is not None else "unknown",
                               "serving": serving}
        return
    said = dump if dump is not None else "none (the dump predates the decisions guard)"
    raise SystemExit(
        f"{where}: a record from decisions {said!r}, but this tree serves "
        f"{serving!r}. The models changed under the dump: the verdicts "
        "of this dump are not those of the decisions that will read the model. "
        "Re-dump with this tree, or pass --decisions-skew-is-fine with a reason.")


def decisions_skew_manifest() -> dict | None:
    """What the manifest records under "decisions_skew": the reason, the dump
    decisions and the serving decisions -- or None when they agreed and there
    is nothing to record. Under an exclusion the hashes are the without-hashes,
    and "exclude" names the model left out of the comparison."""
    return dict(_DECISIONS_SKEW) if _DECISIONS_SKEW is not None else None


def capped(documents: list[str], mask, limit: int | None):
    """`mask` with at most `limit` rows kept per document, in row order.

    Deterministic rather than sampled: a random draw here would make two runs of
    one curve differ by the draw instead of by the data.
    """
    if limit is None:
        return mask
    seen: dict[str, int] = {}
    out = mask.copy()
    for at, (name, keep) in enumerate(zip(documents, mask)):
        if not keep:
            continue
        seen[name] = seen.get(name, 0) + 1
        if seen[name] > limit:
            out[at] = False
    return out


def share_of_the_top(documents: list[str], mask) -> tuple[float, list[tuple[str, int]]]:
    """What the five largest documents carry of a split, and which they are."""
    counts = collections.Counter(
        name for name, keep in zip(documents, mask) if keep)
    total = sum(counts.values())
    top = counts.most_common(HOW_MANY_AT_THE_TOP)
    return (sum(n for _d, n in top) / total if total else 0.0), top


def refuse_a_lopsided_split(name: str, documents: list[str], mask,
                            allow: bool = False) -> None:
    """Print what the top five carry of this split, and refuse if it is too much."""
    share, top = share_of_the_top(documents, mask)
    print(f"[train] {name}: the top {len(top)} document(s) carry {share:.1%}")
    for document, n in top:
        print(f"[train]   {n:7d}  {document[:56]}")
    if share > LOPSIDED and not allow:
        raise SystemExit(
            f"[train] REFUSED: {name} is {share:.1%} five documents, over the "
            f"{LOPSIDED:.0%} this allows. A split that one document speaks for "
            "chooses the tree count and the threshold on that document: "
            "`refman-8.0-en.pdf` was 34.1 per cent of one half and 51 per cent "
            "of another lane's items (#602). Cap harder, or pass "
            "--lopsided-is-fine with a reason.")
