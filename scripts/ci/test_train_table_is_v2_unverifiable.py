#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The final-table trainer can leave out unverifiable PubTables negatives (#638).

`scripts/extraction/learned/train_table_is_v2.py
--pubtables-unverifiable-negatives` is `keep` (the default, today's training
rows, byte-identical) or `drop`: with `drop`, a PubTables row that
`label_pubtables` labels 0 and whose box has zero overlap area with every
truth table box on its page is left out before the cap -- touching counts as
overlap, so only a strictly disjoint box goes. DocLayNet rows are never
affected: that truth is exhaustive per page.

- with `keep`, rows identical to today on a small synthetic dump: every
  synthetic PubTables row stays, in order, whatever its box;
- with `drop`, a disjoint PubTables negative is gone, a partially
  overlapping one stays, a positive stays, an edge-touching negative stays,
  and a DocLayNet negative far from any table stays (the DocLayNet path
  never drops: main passes 0 dropped for it);
- the training log prints, per source, rows before / dropped / after and
  the dropped share of PubTables negatives;
- `--oof-out` rows carry the option's value in a header line and a sidecar,
  so an OOF file says which rule produced it;
- the model's manifest records the option
  (`pubtables_unverifiable_negatives`).

And six mutations: the keep default, the keep short-circuit, the drop
filter, the touching check, the dropped-share counting, and the OOF header
value are each taken out of a copy of the trainer, and the cased input then
has to read or say otherwise -- which proves each is what the test
observes, and not something else in the file.
No lightgbm on the gate: everything here runs before any model is fitted.
"""
from __future__ import annotations

import importlib.util
import json
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
TRAINER = LEARNED / "train_table_is_v2.py"

FLAG = '"--pubtables-unverifiable-negatives"'
DEFAULT_KEEP = '"--pubtables-unverifiable-negatives", default="keep",'
CHOICES = 'choices=("keep", "drop"),'
KEEP_CALL = "keep_pubtables_row(y, t[\"bbox\"], truth_boxes,"
LOG_CALL = "unverifiable_log_lines("
LOG_PRINT = "for log_line in unverifiable_log_lines("
MANIFEST_KEY = '"pubtables_unverifiable_negatives": args.pubtables_unverifiable_negatives'
OOF_CALL = "write_oof_provenance(args.oof_out, args.pubtables_unverifiable_negatives)"
UNVERIFIABLE_DEF = "def is_unverifiable_pubtables_negative(bbox, truth_boxes):"
TOUCHES_DEF = "def _touches(a, b):"
KEEP_DEF = "def keep_pubtables_row(label, bbox, truth_boxes, unverifiable):"

HEIGHT = 100.0
TRUTH = {"name": "PMC123-p0000.pdf", "document": "PMC123",
         "blocks": [{"kind": "table", "bbox": [10, 10, 50, 30]}]}
# The truth box in dump coordinates (the label_pubtables y-flip).
FLIPPED = [10, 70, 50, 90]
POSITIVE = [10, 70, 50, 90]
PARTIAL = [45, 75, 65, 85]
DISJOINT = [70, 10, 90, 30]
TOUCHING = [50, 70, 70, 90]


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


def _labels(module):
    """The synthetic boxes labelled, as main labels them."""
    out = {}
    for name, box in (("positive", POSITIVE), ("partial", PARTIAL),
                      ("disjoint", DISJOINT), ("touching", TOUCHING)):
        y, _ = module.label_pubtables({"bbox": box}, TRUTH, HEIGHT)
        out[name] = y
    return out


def check_source_wiring(source: str) -> list[str]:
    fails = []
    for call, name in ((FLAG, "--pubtables-unverifiable-negatives"),
                       (DEFAULT_KEEP, "the keep default"),
                       (CHOICES, "the keep/drop choices"),
                       (UNVERIFIABLE_DEF, "the unverifiable check"),
                       (TOUCHES_DEF, "the touching check"),
                       (KEEP_DEF, "the keep/drop gate"),
                       (KEEP_CALL, "the PubTables drop filter"),
                       (LOG_CALL, "the per-source log lines"),
                       (LOG_PRINT, "the printed per-source log"),
                       (MANIFEST_KEY, "the option in the manifest"),
                       (OOF_CALL, "the OOF provenance write")):
        if call not in source:
            fails.append(f"{name} ({call!r}) is not in train_table_is_v2.py")
    return fails


def check_flag(module) -> list[str]:
    fails = []
    parser = module.make_parser()
    if parser.get_default("pubtables_unverifiable_negatives") != "keep":
        fails.append("--pubtables-unverifiable-negatives does not default "
                     "to keep")
    parsed = parser.parse_args(["--dump", "d.jsonl",
                                "--pubtables-unverifiable-negatives", "drop"])
    if parsed.pubtables_unverifiable_negatives != "drop":
        fails.append("--pubtables-unverifiable-negatives drop did not parse")
    parsed = parser.parse_args(["--dump", "d.jsonl"])
    if parsed.pubtables_unverifiable_negatives != "keep":
        fails.append("the flag without a value is not keep")
    return fails


def check_boxes(module) -> list[str]:
    """The synthetic dump labels as the cases need."""
    fails = []
    boxes = module.pubtables_table_boxes(TRUTH, HEIGHT)
    if boxes != [FLIPPED]:
        fails.append(f"the truth boxes flipped to {boxes}; "
                     f"expected {[FLIPPED]}")
    labels = _labels(module)
    if labels != {"positive": 1, "partial": 0, "disjoint": 0,
                  "touching": 0}:
        fails.append(f"the synthetic boxes labelled {labels}; expected "
                     "positive 1 and the other three 0")
    if module._overlap(DISJOINT, FLIPPED) != 0:
        fails.append("the disjoint box overlaps the truth box")
    if module._overlap(PARTIAL, FLIPPED) <= 0:
        fails.append("the partial box does not overlap the truth box")
    if module._overlap(TOUCHING, FLIPPED) != 0:
        fails.append("the touching box has overlap area: it should touch "
                     "with zero area")
    if not module._touches(TOUCHING, FLIPPED):
        fails.append("the touching box does not touch the truth box")
    if module._touches(DISJOINT, FLIPPED):
        fails.append("the disjoint box touches the truth box")
    if not module._touches(POSITIVE, FLIPPED):
        fails.append("the positive box does not touch the truth box")
    return fails


def check_keep_and_drop(module) -> list[str]:
    fails = []
    boxes = module.pubtables_table_boxes(TRUTH, HEIGHT)
    rows = [(1, POSITIVE), (0, PARTIAL), (0, DISJOINT), (0, TOUCHING)]
    # WITH KEEP, every row stays, in order -- today's rows, byte-identical.
    kept = [r for r in rows
            if module.keep_pubtables_row(r[0], r[1], boxes, "keep")]
    if kept != rows:
        fails.append(f"with keep, {len(kept)} of {len(rows)} rows stayed; "
                     "expected all four in order")
    # WITH DROP, the disjoint negative is gone and the rest stay.
    kept = [r for r in rows
            if module.keep_pubtables_row(r[0], r[1], boxes, "drop")]
    if kept != [(1, POSITIVE), (0, PARTIAL), (0, TOUCHING)]:
        fails.append("with drop, the kept rows are not positive, partial "
                     f"and touching: {kept}")
    if module.is_unverifiable_pubtables_negative(DISJOINT, boxes) is not True:
        fails.append("the disjoint negative is not unverifiable")
    for name, box in (("positive", POSITIVE), ("partial", PARTIAL),
                      ("touching", TOUCHING)):
        if module.is_unverifiable_pubtables_negative(box, boxes) is not False:
            fails.append(f"the {name} box is unverifiable: it should stay")
    # A DocLayNet negative far from any table stays: the DocLayNet path
    # never drops -- main passes 0 dropped for it, and the log helper
    # reports what it is passed.
    far = module.is_unverifiable_pubtables_negative(DISJOINT, boxes)
    if far is not True:
        fails.append("the far box would not even drop as PubTables: the "
                     "DocLayNet case no longer shows what it must")
    lines = module.unverifiable_log_lines("drop", 4, 0, 4, 4, 1, 3, 3)
    if "0 dropped" not in lines[0] or not lines[0].startswith("doclaynet"):
        fails.append(f"the DocLayNet log line is not 4/0/4: {lines[0]!r}")
    return fails


def check_log(module) -> list[str]:
    fails = []
    lines = module.unverifiable_log_lines("drop", 10, 0, 10, 20, 5, 15, 8)
    if len(lines) != 2:
        return [f"the log is {len(lines)} lines; expected 2 (one per source)"]
    if lines[0] != "doclaynet rows: 10 before, 0 dropped, 10 after":
        fails.append(f"the DocLayNet line is {lines[0]!r}")
    if lines[1] != ("pubtables rows (drop): 20 before, 5 dropped, 15 after "
                    "(5 of 8 negatives, 62.5%)"):
        fails.append(f"the PubTables line is {lines[1]!r}")
    lines = module.unverifiable_log_lines("keep", 3, 0, 3, 7, 0, 7, 4)
    if lines[1] != ("pubtables rows (keep): 7 before, 0 dropped, 7 after "
                    "(0 of 4 negatives, 0.0%)"):
        fails.append(f"the keep line is {lines[1]!r}")
    lines = module.unverifiable_log_lines("drop", 0, 0, 0, 0, 0, 0, 0)
    if lines[1] != ("pubtables rows (drop): 0 before, 0 dropped, 0 after "
                    "(0 of 0 negatives, 0.0%)"):
        fails.append(f"the empty line divides by zero or misreports: "
                     f"{lines[1]!r}")
    return fails


def check_oof(module) -> list[str]:
    fails = []
    if module.oof_sidecar_path("out.jsonl") != "out.jsonl.meta.json":
        fails.append("the sidecar is not beside the OOF file")
    with tempfile.TemporaryDirectory() as tmp:
        oof = pathlib.Path(tmp) / "oof.jsonl"
        module.write_oof_out(oof, ["d0", "d1"], ["p0", "p1"],
                             [[0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 2.0, 2.0]],
                             [1, 0], [0.9, 0.1], [0, 1], [0.8, 0.2])
        before = oof.read_text(encoding="utf-8").splitlines()
        if len(before) != 2:
            fails.append(f"the OOF file holds {len(before)} rows; expected 2")
            return fails
        module.write_oof_provenance(str(oof), "drop")
        after = oof.read_text(encoding="utf-8").splitlines()
        if len(after) != 3:
            fails.append(f"with provenance the OOF file holds {len(after)} "
                         "lines; expected a header plus 2 rows")
            return fails
        try:
            header = json.loads(after[0])
        except ValueError:
            fails.append(f"the header line is not JSON: {after[0][:60]}")
            return fails
        if header != {"pubtables_unverifiable_negatives": "drop"}:
            fails.append(f"the header carries {header}; expected the drop "
                         "rule")
        if after[1:] != before:
            fails.append("the rows after the header are not the rows as "
                         "written")
        sidecar = pathlib.Path(module.oof_sidecar_path(str(oof)))
        if not sidecar.is_file():
            fails.append("the sidecar beside the OOF file was not written")
        elif sidecar.read_text(encoding="utf-8").splitlines() != [after[0]]:
            fails.append("the sidecar does not carry the header line")
        module.write_oof_provenance(str(oof), "keep")
        header = json.loads(oof.read_text(encoding="utf-8").splitlines()[0])
        if header != {"pubtables_unverifiable_negatives": "keep"}:
            fails.append(f"the keep header carries {header}")
    return fails


def check_mutations(source: str) -> list[str]:
    fails = []
    # THE KEEP DEFAULT taken out defaults to drop.
    mutant = load(source.replace(DEFAULT_KEEP,
                                 '"--pubtables-unverifiable-negatives", default="drop",'),
                  "train_table_is_v2_unverifiable_mutant_default")
    if mutant.make_parser().get_default("pubtables_unverifiable_negatives") == "keep":
        fails.append("with the keep default taken out, the flag still "
                     "defaults to keep -- something else points it there, "
                     "and this test is not testing the default")
    # THE KEEP SHORT-CIRCUIT taken out drops the disjoint row even on keep.
    mutant = load(source.replace('if unverifiable == "keep":',
                                 'if unverifiable == "KEEP-NEVER":'),
                  "train_table_is_v2_unverifiable_mutant_keep")
    boxes = mutant.pubtables_table_boxes(TRUTH, HEIGHT)
    if mutant.keep_pubtables_row(0, DISJOINT, boxes, "keep"):
        fails.append("with the keep short-circuit taken out, keep still "
                     "keeps the disjoint negative -- something else keeps "
                     "it, and this test is not testing the gate")
    # THE DROP FILTER taken out keeps the disjoint row even on drop.
    mutant = load(source.replace("return not is_unverifiable_pubtables_negative(bbox, truth_boxes)",
                                 "return True"),
                  "train_table_is_v2_unverifiable_mutant_drop")
    boxes = mutant.pubtables_table_boxes(TRUTH, HEIGHT)
    if not mutant.keep_pubtables_row(0, DISJOINT, boxes, "drop"):
        fails.append("with the drop filter taken out, drop still drops the "
                     "disjoint negative -- something else drops it, and "
                     "this test is not testing the filter")
    # THE TOUCHING CHECK taken out drops the edge-touching negative.
    mutant = load(source.replace("return all(_overlap(bbox, t) == 0 and not _touches(bbox, t)",
                                 "return all(_overlap(bbox, t) == 0"),
                  "train_table_is_v2_unverifiable_mutant_touch")
    boxes = mutant.pubtables_table_boxes(TRUTH, HEIGHT)
    if mutant.keep_pubtables_row(0, TOUCHING, boxes, "drop"):
        fails.append("with the touching check taken out, the touching "
                     "negative still stays -- something else keeps it, and "
                     "this test is not testing the check")
    # THE DROPPED-SHARE COUNTING taken out reports 0.0% for 5 of 8.
    mutant = load(source.replace("pubtables_dropped / pubtables_negatives",
                                 "0.0"),
                  "train_table_is_v2_unverifiable_mutant_share")
    line = mutant.unverifiable_log_lines("drop", 10, 0, 10, 20, 5, 15, 8)[1]
    if "62.5%" in line:
        fails.append("with the share counting taken out, the log still "
                     "reports 62.5% -- something else counts it, and this "
                     "test is not testing the counting")
    # THE OOF HEADER VALUE taken out writes keep where drop goes.
    mutant = load(source.replace('{"pubtables_unverifiable_negatives": unverifiable}',
                                 '{"pubtables_unverifiable_negatives": "keep"}'),
                  "train_table_is_v2_unverifiable_mutant_oof")
    with tempfile.TemporaryDirectory() as tmp:
        oof = pathlib.Path(tmp) / "oof.jsonl"
        mutant.write_oof_out(oof, ["d0"], ["p0"], [[0.0, 0.0, 1.0, 1.0]],
                             [0], [0.1], [0], [0.2])
        mutant.write_oof_provenance(str(oof), "drop")
        header = json.loads(oof.read_text(encoding="utf-8").splitlines()[0])
        if header == {"pubtables_unverifiable_negatives": "drop"}:
            fails.append("with the header value taken out, drop still "
                         "writes drop -- something else writes it, and this "
                         "test is not testing the header")
    return fails


def check_all() -> list[str]:
    source = TRAINER.read_text(encoding="utf-8")
    fails = check_source_wiring(source)
    if fails:
        return fails
    real = load(source, "train_table_is_v2_real_unverifiable")
    fails.extend(check_flag(real))
    fails.extend(check_boxes(real))
    fails.extend(check_keep_and_drop(real))
    fails.extend(check_log(real))
    fails.extend(check_oof(real))
    fails.extend(check_mutations(source))
    return fails


def test_flag_defaults_to_keep():
    source = TRAINER.read_text(encoding="utf-8")
    wiring = check_source_wiring(source)
    assert wiring == [], wiring
    real = load(source, "train_table_is_v2_pytest_flag")
    fails = check_flag(real)
    assert fails == [], fails


def test_synthetic_boxes_label_as_needed():
    real = load(TRAINER.read_text(encoding="utf-8"),
                "train_table_is_v2_pytest_boxes")
    fails = check_boxes(real)
    assert fails == [], fails


def test_keep_is_identical_and_drop_filters():
    real = load(TRAINER.read_text(encoding="utf-8"),
                "train_table_is_v2_pytest_keepdrop")
    fails = check_keep_and_drop(real)
    assert fails == [], fails


def test_log_reports_counts():
    real = load(TRAINER.read_text(encoding="utf-8"),
                "train_table_is_v2_pytest_log")
    fails = check_log(real)
    assert fails == [], fails


def test_oof_carries_the_rule():
    real = load(TRAINER.read_text(encoding="utf-8"),
                "train_table_is_v2_pytest_oof")
    fails = check_oof(real)
    assert fails == [], fails


def test_mutations_are_red():
    fails = check_mutations(TRAINER.read_text(encoding="utf-8"))
    assert fails == [], fails


def main() -> int:
    fails = check_all()
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("table-is-v2 trainer: --pubtables-unverifiable-negatives keeps "
          "(the default, today's rows) or drops the disjoint PubTables "
          "negatives before the cap -- the partial, the positive and the "
          "touching stay, DocLayNet never drops, the log reports per-source "
          "before/dropped/after and the dropped share, and the OOF file and "
          "the manifest say which rule produced them.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
