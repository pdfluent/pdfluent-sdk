"""Train the final-table model: is a table the pass would emit a table? (#574, #638)

    train_table_is_v2.py --dump TRAIN.jsonl [--cap 10] [--cap-negatives 10] [--model-dir DIR]

The shipped table-or-not model reviews BUILDER tables only, so it cannot see
the false tables the judge chooses from other proposers -- on the rig-like
proxy, 77 of 84 produced tables over prose, 37 with no builder under them at
all. This model reads the table the caller would get: each chosen region
after its cells are decoded, and each builder table kept, with the judge's
probability beside it (a stacked model).

PYTHON NEVER RECOMPUTES THE FEATURES: the dump's `final_tables` carry the
vectors `learned::tables::table_is_v2_features` computed, and this reads
them. `make_table_is_v2_parity.py` recomputes them on fixture pages only to
hold the two equal.

LABELS, from the dump's own truth (T2's miner rules): DocLayNet-train is
POSITIVE when the table overlaps a truth table by >= 0.5 of the smaller area
(a fragment of a table is still a table); HOST (negative) when it covers
>= 2 truth text regions (>= 0.8 of each one's words) and overlaps no truth
table by >= 0.5; anything else is left out. Every row keeps the IoU with its
best truth table, so calibration on fragments can be checked later.
PubTables-train is positive on the same overlap rule and negative when it
overlaps none (PubTables marks every table on its pages).
Train documents only, eval, rig and strat-300 guards, `same_engine` +
`same_decisions` (the features include the judge's probability), cap per
document PER CLASS: up to --cap positives and --cap-negatives negatives,
each shuffled with the seed, so the negatives survive on documents with
many real tables (#638).
After the cap each class is refused when five documents carry more than a
fifth of it.
The PubTables lists default to the checked-in files beside this script, so a
retrain runs from a checkout without absolute paths: the original train
list, and --train-lists both (the default) adds the era-balanced addition
beside it.
THE THRESHOLD is chosen out of fold on training documents: the highest of the
0.02 ... 0.60 grid that drops at most LOSS_BUDGET of the real tables. The run
also reports the OOF AUROC of the model and, for comparison, of the single
feature `run_on_share`.
When `--lopsided-is-fine` lets a lopsided set through, the run reports what
it let through: the manifest keeps the reason, and -- on every run, in the
printout and the manifest -- the per-class top-five shares after the cap and
a second out-of-fold run without the document that holds the most negatives,
with its OOF AUROC beside the main run's.
`--oof-out PATH` writes what the main run scored, for the TRAIN-side check
before any feature change: after the main out-of-fold run, one JSON line per
training row after the cap -- document, page_id, bbox, label, iou, fold,
oof -- each row with its fold's probability, from the model that never
trained on it. Without the flag nothing is written.
`--pubtables-unverifiable-negatives drop` leaves out a PubTables negative
whose box touches no truth table on its page, before the cap: the PubTables
truth is not exhaustive per page, so such a row is unverifiable (#638).
`keep` (the default) trains on today's rows, byte-identical.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations
import argparse, collections, json, math, pathlib, random, sys
HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[2] / "ci"))
import align_labels as al  # noqa: E402
import pubtables_splits as splits  # noqa: E402
import training_rules  # noqa: E402
from features import SCHEMA  # noqa: E402

SEED = 20260930
LOSS_BUDGET = 0.01
# Out of fold on the TRAIN documents only, as before, but wider: fine below
# 0.10 where the 1 % loss budget binds, in steps of 0.02 from there to 0.60.
# The shipped model chose the old grid's top (0.15), so the grid bound the
# choice rather than the budget (#638).
THRESHOLDS = (0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.10) + tuple(
    round(0.10 + 0.02 * i, 2) for i in range(1, 26))
# In `TABLE_IS_V2_NAMES` order: the first fifteen are v1's, the rest the
# final-table additions. Labels only -- the vectors come from the dump.
NAMES = ['rows', 'cols', 'cells', 'numeric', 'words_cell', 'col_words_max', 'col_words_min',
         'heading_share', 'p_head_max', 'p_head_mean', 'first_col_p_head', 'empty', 'height_share',
         'wordy_share', 'first_col_words', 'words_filled_median', 'words_filled_mean',
         'figure_share_filled', 'sentence_share', 'lines_median', 'run_on_share', 'row_overlap',
         'column_start_spread', 'judge_p', 'from_builder', 'grid_rows', 'grid_columns',
         'filled_share', 'has_rulings']

TEXT = {"text", "section_header", "list_item", "caption", "footnote", "title"}

# PubTables ships its document lists beside the data, and the eval list must
# never reach a trainer -- not even through a swapped argument. Refused by
# file NAME, always: no content check can tell a train list from an eval one.
# The strat-300 measure set is refused the same way: it is for measuring
# only, and a model that has seen it cannot be measured on it (#638).
PUBTABLES_EVAL_DOCUMENTS = "pubtables_eval_documents.txt"
PUBTABLES_STRAT300 = "pubtables_eval_strat300.txt"

# The PubTables split, beside this script: the train documents a retrain
# reads, and the eval documents it must never read (#638). Defaults, so a
# retrain runs from a checkout without absolute paths.
TRAIN_LIST = HERE.parent / "pubtables_train_documents.txt"
EVAL_LIST = HERE.parent / "pubtables_eval_documents.txt"
# The era-balanced addition to the train list, drawn stratified over the PMC
# id by `draw_pubtables_documents.py` and committed beside the original list
# (#638). Not drawn yet: until it lands, the default run trains on the
# original list and says so on stderr.
ERA_LIST = HERE.parent / "pubtables_train_documents_era.txt"
# The strat-300 measure set, beside this script: 300 eval pages for measuring
# only, which no trainer reads. `main` refuses any training input naming one
# of them, before LightGBM loads (#638).
STRAT300 = HERE.parent / "pubtables_eval_strat300.txt"

# Said once on stderr when the default run finds no era list: the run trains
# on the original list rather than refusing, but never silently.
ERA_MISSING_NOTE = "era train list missing: training on the original list only"


def read_document_list(path):
    """The documents named in a list file, one per line.

    Lines starting with '#' are comments and skipped, as are blank lines. A
    file named `pubtables_eval_documents.txt` is refused by name: eval
    documents never train. So is a file named `pubtables_eval_strat300.txt`:
    the measure set never trains either.
    """
    if pathlib.Path(path).name == PUBTABLES_EVAL_DOCUMENTS:
        raise SystemExit(f"refusing the eval documents list: {path}")
    if pathlib.Path(path).name == PUBTABLES_STRAT300:
        raise SystemExit(
            f"refusing the strat-300 measure list: {path}. That set is for "
            "measuring only; a model that has seen it cannot be measured on it.")
    out = set()
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            out.add(line)
    return out


def read_eval_documents(path):
    """The documents named in the eval list file: same format as a train
    list, read without the name refusal -- this IS the eval list."""
    out = set()
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            out.add(line)
    return out


def read_train_documents(documents_path, era_path, which):
    """The PubTables train documents for this run: original, era, or both.

    `which` is --train-lists: "original" reads the original list only, "era"
    the era-balanced addition only, "both" (the default) their union. The
    era list is not drawn yet: when "both" finds it missing, the run trains
    on the original list and says ERA_MISSING_NOTE on stderr, once -- never
    refused, never silent. An explicit "era" without the file is refused: it
    asked for a list that is not there.
    """
    if which == "original":
        return read_document_list(documents_path)
    if which == "era":
        if not pathlib.Path(era_path).is_file():
            raise SystemExit(
                f"the era train list is not there: {era_path}. It is drawn "
                "later and committed as data; train --train-lists original "
                "until it lands.")
        return read_document_list(era_path)
    if which != "both":
        raise SystemExit(
            f"unknown train lists choice: {which!r} "
            "(original, era, or both)")
    original = read_document_list(documents_path)
    if not pathlib.Path(era_path).is_file():
        print(ERA_MISSING_NOTE, file=sys.stderr)
        return original
    return original | read_document_list(era_path)


def refuse_strat_pages(truth_path, dump_paths=(), documents=()):
    """Refuse a training run whose inputs read the strat-300 set (#638).

    By page AND by document: the set names pages, and a model that trains on
    another page of a measure document has still seen the measure. The truth
    and every dump are refused for naming a strat page or a page of a strat
    document, and the train documents for naming a strat document -- each
    with the page or document named. A missing measure set refuses the run
    rather than training as if the set were empty.
    """
    strat = splits.read_id_set(STRAT300)
    strat_docs = {splits.document_of(page) for page in strat}
    if truth_path:
        for page in json.loads(pathlib.Path(truth_path).read_text()):
            name = page.get("name")
            if name in strat or page.get("document") in strat_docs:
                raise SystemExit(
                    f"a strat-300 page in the training truth: {name}. "
                    "That set is for measuring only; a model that has seen it "
                    "cannot be measured on it.")
    for dump in dump_paths:
        if not dump:
            continue
        for raw in open(dump):
            record = json.loads(raw)
            page_id = record.get("page_id")
            if page_id in strat or record.get("document") in strat_docs:
                raise SystemExit(
                    f"a strat-300 page in the training dump: "
                    f"{page_id or record.get('document')}. That set is for "
                    "measuring only; a model that has seen it cannot be "
                    "measured on it.")
    overlap = set(documents) & strat_docs
    if overlap:
        raise SystemExit(
            f"the train documents list names {len(overlap)} strat-300 "
            f"documents ({', '.join(sorted(overlap)[:5])}): that set is for "
            "measuring only; a model that has seen it cannot be measured "
            "on it.")


def _area(b):
    return max(0.0, b[2] - b[0]) * max(0.0, b[3] - b[1])


def _overlap(a, b):
    return max(0.0, min(a[2], b[2]) - max(a[0], b[0])) * max(0.0, min(a[3], b[3]) - max(a[1], b[1]))


def _overlap_of_smaller(a, b):
    smaller = min(_area(a), _area(b))
    return _overlap(a, b) / smaller if smaller > 0 else 0.0


def _iou(a, b):
    inter = _overlap(a, b)
    union = _area(a) + _area(b) - inter
    return inter / union if union > 0 else 0.0


def _touches(a, b):
    """True when the boxes share any point, including an edge or a corner.

    `_overlap` is zero for edge-touching boxes, but a PubTables negative
    that touches a truth table is verifiable -- a fragment or an overgrown
    box, not a far-away one -- so touching counts as overlap (#638).
    """
    return min(a[2], b[2]) >= max(a[0], b[0]) and min(a[3], b[3]) >= max(a[1], b[1])


def _contains(b, x, y):
    return b[0] <= x <= b[2] and b[1] <= y <= b[3]


def _coverage(region, table, words):
    """How much of a region's words lie inside a table, (covered, in_region).

    The region's words are the page's words whose centre lies in it; covered
    are those whose centre also lies inside the table's box. A region with no
    words is never covered.
    """
    in_region = covered = 0
    for w in words:
        cx = (w["bbox"][0] + w["bbox"][2]) / 2
        cy = (w["bbox"][1] + w["bbox"][3]) / 2
        if _contains(region, cx, cy):
            in_region += 1
            if _contains(table, cx, cy):
                covered += 1
    return covered, in_region


def label_doclaynet(record, table):
    """(label, best_iou) or None when the table is left out.

    POSITIVE (1) when it overlaps a truth table by >= 0.5 of the smaller
    area; HOST (0) when it covers >= 2 truth text regions (>= 0.8 of each
    one's words) and overlaps no truth table by >= 0.5; else None.
    """
    try:
        regions = al.regions_of(record)
    except (Exception, SystemExit):
        return None
    bbox = table["bbox"]
    best = 0.0
    touches = False
    for r in regions:
        if al.label_of(r) == "table":
            best = max(best, _iou(bbox, r["bbox"]))
            if _overlap_of_smaller(bbox, r["bbox"]) >= 0.5:
                touches = True
    if touches:
        return 1, best
    words = record.get("words") or []
    covered = 0
    for r in regions:
        if al.label_of(r) in TEXT:
            has, total = _coverage(r["bbox"], bbox, words)
            if total > 0 and has / total >= 0.8:
                covered += 1
    if covered >= 2:
        return 0, best
    return None


def label_pubtables(table, truth, height):
    """(label, best_iou): positive on the same overlap rule, else negative."""
    bbox = table["bbox"]
    boxes = [[t["bbox"][0], height - t["bbox"][3], t["bbox"][2], height - t["bbox"][1]]
             for t in truth.get("blocks") or [] if t.get("kind") == "table"]
    best = max((_iou(bbox, b) for b in boxes), default=0.0)
    touches = any(_overlap_of_smaller(bbox, b) >= 0.5 for b in boxes)
    return (1 if touches else 0), best


def pubtables_table_boxes(truth, height):
    """The truth table boxes in dump coordinates (the label_pubtables y-flip).

    The same conversion the labeller uses, so the unverifiable check judges
    the same boxes the label did.
    """
    return [[t["bbox"][0], height - t["bbox"][3], t["bbox"][2], height - t["bbox"][1]]
            for t in truth.get("blocks") or [] if t.get("kind") == "table"]


def is_unverifiable_pubtables_negative(bbox, truth_boxes):
    """True when the box has zero overlap area with every truth table box.

    `_overlap` == 0 for all -- and touching counts as overlap, so an
    edge-touching box is verifiable: only a strictly disjoint box is
    unverifiable. The caller has already asked `label_pubtables`: positives
    overlap by >= 0.5 and never reach here as dropped rows. DocLayNet rows
    never reach here at all -- that truth is exhaustive per page (#638).
    """
    return all(_overlap(bbox, t) == 0 and not _touches(bbox, t)
               for t in truth_boxes)


def keep_pubtables_row(label, bbox, truth_boxes, unverifiable):
    """True when a labelled PubTables row stays in the training rows.

    `keep` trains on today's rows, byte-identical. `drop` leaves out a row
    `label_pubtables` labels 0 whose box is unverifiable -- zero overlap
    area with every truth table box, touching none. Positives and partially
    overlapping negatives stay, whatever the option.
    """
    if unverifiable == "keep":
        return True
    if label != 0:
        return True
    return not is_unverifiable_pubtables_negative(bbox, truth_boxes)


def vector(table):
    """The dump's vector, `null` (a missing judge probability) as NaN."""
    return [float("nan") if v is None else float(v) for v in table["features"]]


def negative_cap(cap, cap_negatives):
    """Negatives kept per document: --cap-negatives, or --cap when not passed."""
    return cap if cap_negatives is None else cap_negatives


def cap_rows(rows, cap, cap_negatives, rng):
    """Up to `cap` positives and `cap_negatives` negatives, each class shuffled.

    A flat cap draws ten rows from a document regardless of class, so a
    document with many real tables keeps almost only positives and the few
    hosts it has are cut. Rows are (features, label, iou, ...): the cap only
    looks at the label and keeps the rest of each row -- the page id and box
    the OOF report needs -- untouched. (#638)
    """
    pos = [r for r in rows if r[1] == 1]
    neg = [r for r in rows if r[1] != 1]
    rng.shuffle(pos)
    rng.shuffle(neg)
    return pos[:cap] + neg[:cap_negatives]


def auroc(labels, scores):
    """The area under the ROC curve, by ranks with ties averaged.

    NaN scores are dropped with their labels. None when one class is missing,
    which is a training set with nothing to rank, not a zero.
    """
    pairs = [(float(s), int(lab)) for s, lab in zip(scores, labels) if s == s]
    n_pos = sum(lab for _, lab in pairs)
    n_neg = len(pairs) - n_pos
    if n_pos == 0 or n_neg == 0:
        return None
    pairs.sort(key=lambda p: p[0])
    ranked = 0.0
    at = 0
    while at < len(pairs):
        end = at
        while end < len(pairs) and pairs[end][0] == pairs[at][0]:
            end += 1
        rank = (at + 1 + end) / 2.0
        ranked += rank * sum(lab for _, lab in pairs[at:end])
        at = end
    return (ranked - n_pos * (n_pos + 1) / 2.0) / (n_pos * n_neg)


def refuse_lopsided_classes(documents, labels, allow=False):
    """Refuse a training set five documents speak for, per class (#602, #638).

    The cap is per class, so the concentration check is too: the top five
    documents of the positives and of the negatives each carry at most a fifth
    of their class. The rule itself is `training_rules`' -- this is the
    per-class call of it, after the cap.
    """
    for name, wanted in (("positives", 1), ("negatives", 0)):
        mask = [lab == wanted for lab in labels]
        training_rules.refuse_a_lopsided_split(name, documents, mask, allow=allow)


def concentration_by_class(documents, labels):
    """Per-class top-five concentration after the cap (#638).

    {class: {"share", "top", "rows"}}: the share of the class's rows its five
    largest documents hold, those five (document, rows) pairs, and the class
    size. Counted by `training_rules.share_of_the_top`, so the report names
    exactly what the guard judged. Positive is label 1, negative label 0 --
    the only labels the labellers return.
    """
    out = {}
    for name, wanted in (("positives", 1), ("negatives", 0)):
        mask = [lab == wanted for lab in labels]
        share, top = training_rules.share_of_the_top(documents, mask)
        out[name] = {"share": share, "top": list(top),
                     "rows": sum(1 for keep in mask if keep)}
    return out


def most_negative_document(documents, labels):
    """The document holding the most negative rows, or None without negatives.

    Ties break alphabetically, so the document the second run drops is
    deterministic.
    """
    counts = collections.Counter(
        d for d, lab in zip(documents, labels) if lab != 1)
    if not counts:
        return None
    return sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[0][0]


def keep_without(documents, excluded):
    """A mask keeping every row but `excluded`'s -- both its classes."""
    return [d != excluded for d in documents]


def threshold_row(labels, scores, threshold):
    """(dropped false, false total, lost real, real total) under the threshold.

    Rows scoring below `threshold` are the refused ones. NaN scores are kept:
    a row without a score is not a row the model refused. Both threshold
    tables print from this, so the main run and the without-it run count the
    same way.
    """
    drop_false = false = lost_real = real = 0
    for lab, s in zip(labels, scores):
        if lab == 1:
            real += 1
            if s == s and s < threshold:
                lost_real += 1
        else:
            false += 1
            if s == s and s < threshold:
                drop_false += 1
    return drop_false, false, lost_real, real


def write_oof_out(path, documents, page_ids, bboxes, labels, ious, folds, oof):
    """One JSON line per training row after the cap, with its OOF score (#638).

    `documents`, `page_ids`, `bboxes`, `labels`, `ious` are the capped rows in
    order, `folds` their fold assignment and `oof` the main run's out-of-fold
    probabilities: row i carries fold i's probability, from the model that
    trained on every fold but i's. T4 checks the TRAIN side from this -- which
    rows the model refuses out of fold -- before any feature change.
    """
    with open(path, "w", encoding="utf-8") as f:
        for d, pid, box, lab, iou, fold, p in zip(
                documents, page_ids, bboxes, labels, ious, folds, oof):
            f.write(json.dumps({"document": d, "page_id": pid,
                                "bbox": list(box), "label": int(lab),
                                "iou": float(iou), "fold": int(fold),
                                "oof": float(p)}, sort_keys=True) + "\n")


def oof_sidecar_path(oof_path):
    """The sidecar beside an OOF file, saying which rule produced it."""
    return f"{oof_path}.meta.json"


def write_oof_provenance(oof_path, unverifiable):
    """Record the unverifiable-negatives rule atop and beside the OOF file.

    Prepends a header line carrying the option's value to the OOF file
    itself, and writes the same value to the sidecar beside it -- so an OOF
    file says which rule produced it either way. Rows after the header are
    unchanged: one JSON line per training row after the cap, as before.
    """
    header = json.dumps({"pubtables_unverifiable_negatives": unverifiable},
                        sort_keys=True) + "\n"
    with open(oof_path, encoding="utf-8") as f:
        rows = f.read()
    with open(oof_path, "w", encoding="utf-8") as f:
        f.write(header)
        f.write(rows)
    with open(oof_sidecar_path(oof_path), "w", encoding="utf-8") as f:
        f.write(header)


def unverifiable_log_lines(unverifiable, doclaynet_before, doclaynet_dropped,
                           doclaynet_after, pubtables_before, pubtables_dropped,
                           pubtables_after, pubtables_negatives):
    """Per source, rows before / dropped / after, and the dropped share (#638).

    Two lines, one per source. The PubTables line carries the option's value
    and the dropped share of PubTables negatives -- the share of the
    negatives the rule left out. DocLayNet rows are never dropped: that
    truth is exhaustive per page.
    """
    share = (pubtables_dropped / pubtables_negatives
             if pubtables_negatives else 0.0)
    return [
        f"doclaynet rows: {doclaynet_before} before, {doclaynet_dropped} "
        f"dropped, {doclaynet_after} after",
        f"pubtables rows ({unverifiable}): {pubtables_before} before, "
        f"{pubtables_dropped} dropped, {pubtables_after} after "
        f"({pubtables_dropped} of {pubtables_negatives} negatives, "
        f"{share:.1%})",
    ]


def make_parser():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", required=True)
    ap.add_argument("--cap", type=int, default=10)
    ap.add_argument("--cap-negatives", type=int, default=None,
                    help="negatives kept per document at most (default: --cap)")
    ap.add_argument("--lopsided-is-fine", default=None, metavar="REASON",
                    help="train although five documents carry more than a "
                         "fifth of a class after the cap, and say why")
    ap.add_argument("--oof-out", default=None, metavar="PATH",
                    help="write one JSON line per training row after the cap "
                         "(document, page_id, bbox, label, iou, fold, oof) "
                         "after the main out-of-fold run")
    ap.add_argument("--pubtables", default=None, help="a PubTables-train dump (final-tables schema)")
    ap.add_argument("--pubtables-truth", default=None, help="its ground truth (pages with table boxes)")
    ap.add_argument("--pubtables-documents", default=str(TRAIN_LIST),
                    help="the train documents list "
                         "(default: the checked-in pubtables_train_documents.txt)")
    ap.add_argument("--pubtables-era-documents", default=str(ERA_LIST),
                    help="the era-balanced addition to the train documents "
                         "(default: the checked-in pubtables_train_documents_era.txt)")
    ap.add_argument("--train-lists", default="both",
                    choices=("original", "era", "both"),
                    help="which PubTables train lists to read: the original "
                         "list, the era-balanced addition, or both (default)")
    ap.add_argument("--pubtables-unverifiable-negatives", default="keep",
                    choices=("keep", "drop"),
                    help="PubTables negatives far from any truth table: keep "
                         "them (default, today's training rows) or drop the "
                         "unverifiable ones before the cap")
    ap.add_argument("--trees", type=int, default=300)
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--save", default=None)
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--decisions-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump whose verdicts another tree's "
                         "decisions wrote, and say why; without it such a "
                         "dump is refused")
    return ap


def main() -> int:
    args = make_parser().parse_args()
    # BEFORE LightGBM loads: a refused run refuses fast, and the refusal never
    # depends on the ML stack being installed.
    allowed_pub = read_train_documents(
        args.pubtables_documents, args.pubtables_era_documents, args.train_lists)
    # A RENAMED eval list passes the name refusal above, so the checked-in
    # eval list is read too and any overlap refused by content.
    overlap = allowed_pub & read_eval_documents(EVAL_LIST)
    if overlap:
        raise SystemExit(
            f"the train documents list names {len(overlap)} eval documents "
            f"({', '.join(sorted(overlap)[:5])}): train on the train list only"
        )
    refuse_strat_pages(args.pubtables_truth, (args.dump, args.pubtables),
                       allowed_pub)
    import lightgbm as lgb
    import numpy as np
    import learned_training_uses_no_rig_document as rig_guard
    rig = {e["document"] for e in rig_guard.rig_entries(HERE.parents[3]) if e.get("document")}
    per = collections.defaultdict(list)
    doclaynet_before = doclaynet_after = 0
    pubtables_before = pubtables_dropped = pubtables_after = 0
    pubtables_negatives = 0
    for line in open(args.dump):
        r = json.loads(line)
        if not str(r.get("source", "")).endswith("/train"):
            raise SystemExit(f"a training page is not from the train split: {r.get('source')}")
        training_rules.same_engine(r, args.dump, args.engine_skew_is_fine)
        training_rules.same_decisions(r, args.dump, args.decisions_skew_is_fine)
        if r["document"] in rig:
            continue
        names = r.get("final_table_names") or []
        if names and names != NAMES:
            raise SystemExit(f"a dump with another feature order: {names[:3]}...")
        for t in r.get("final_tables") or []:
            labelled = label_doclaynet(r, t)
            if labelled is not None:
                y, iou = labelled
                doclaynet_before += 1
                doclaynet_after += 1
                per[r["document"]].append(
                    (vector(t), y, iou, r.get("page_id"), t["bbox"]))
    if args.pubtables:
        truth = {p["name"]: p for p in json.loads(pathlib.Path(args.pubtables_truth).read_text())}
        for line in open(args.pubtables):
            r = json.loads(line)
            training_rules.same_engine(r, args.pubtables, args.engine_skew_is_fine)
            training_rules.same_decisions(r, args.pubtables, args.decisions_skew_is_fine)
            page = truth.get(r.get("page_id"))
            if not page or page["document"] in rig:
                continue
            if page["document"] not in allowed_pub:
                raise SystemExit(f"a PubTables page outside the train documents: {page['document']}")
            names = r.get("final_table_names") or []
            if names and names != NAMES:
                raise SystemExit(f"a dump with another feature order: {names[:3]}...")
            truth_boxes = pubtables_table_boxes(page, float(r["height"]))
            for t in r.get("final_tables") or []:
                y, iou = label_pubtables(t, page, float(r["height"]))
                pubtables_before += 1
                if y != 1:
                    pubtables_negatives += 1
                if not keep_pubtables_row(y, t["bbox"], truth_boxes,
                                          args.pubtables_unverifiable_negatives):
                    pubtables_dropped += 1
                    continue
                pubtables_after += 1
                per[page["document"]].append(
                    (vector(t), y, iou, r.get("page_id"), t["bbox"]))
    for log_line in unverifiable_log_lines(
            args.pubtables_unverifiable_negatives, doclaynet_before, 0,
            doclaynet_after, pubtables_before, pubtables_dropped,
            pubtables_after, pubtables_negatives):
        print(log_line)
    cap_neg = negative_cap(args.cap, args.cap_negatives)
    rng = random.Random(SEED)
    before_pos = sum(1 for rows in per.values() for r in rows if r[1] == 1)
    before_neg = sum(1 for rows in per.values() for r in rows if r[1] != 1)
    x, y, docs, ious, page_ids, bboxes = [], [], [], [], [], []
    for d in sorted(per):
        for f, lab, iou, pid, box in cap_rows(per[d], args.cap, cap_neg, rng):
            x.append(f); y.append(lab); docs.append(d); ious.append(iou)
            page_ids.append(pid); bboxes.append(box)
    after_pos = sum(1 for lab in y if lab == 1)
    after_neg = len(y) - after_pos
    print(f"the cap ({args.cap} positives, {cap_neg} negatives per document): "
          f"{before_pos} real and {before_neg} false before, "
          f"{after_pos} real and {after_neg} false after")
    refuse_lopsided_classes(docs, y, allow=bool(args.lopsided_is_fine))
    report = concentration_by_class(docs, y)
    for name in ("positives", "negatives"):
        share, top, rows = report[name]["share"], report[name]["top"], report[name]["rows"]
        print(f"after the cap the top {len(top)} {name} carry {share:.1%} of {rows} {name}")
        for document, n in top:
            print(f"  {n:7d}  {document}")
    excluded = most_negative_document(docs, y)
    kept = keep_without(docs, excluded) if excluded is not None else None
    if excluded is not None:
        print(f"the second run drops {excluded}: "
              f"{len(docs) - sum(kept)} of its rows go, {sum(kept)} stay")
    x = np.asarray(x, dtype=np.float32); y = np.asarray(y); ious = np.asarray(ious)
    order = sorted(set(docs)); rng.shuffle(order)
    fold = {d: i % 3 for i, d in enumerate(order)}
    f = np.asarray([fold[d] for d in docs])
    params = {"objective": "binary", "num_leaves": 15, "max_depth": 5, "learning_rate": 0.05,
              "min_data_in_leaf": 30, "seed": SEED, "deterministic": True, "force_row_wise": True,
              "num_threads": 4, "verbosity": -1}
    oof = np.zeros(len(y))
    for k in range(3):
        m = lgb.train(params, lgb.Dataset(x[f != k], label=y[f != k], feature_name=NAMES), args.trees)
        oof[f == k] = m.predict(x[f == k], num_threads=1)
    if args.oof_out:
        write_oof_out(args.oof_out, docs, page_ids, bboxes, y, ious, f, oof)
        write_oof_provenance(args.oof_out, args.pubtables_unverifiable_negatives)
    # THE SECOND RUN drops the document that holds the most negatives -- all
    # its rows, both classes -- on the same folds and the same seed, so its
    # AUROC and threshold table say what the model is without its largest
    # host. Rows of a fold with nothing left to train or score keep NaN,
    # which the AUROC drops and the threshold table counts as kept.
    keep = np.asarray(kept) if kept is not None else None
    oof_without = None
    if excluded is not None:
        oof_without = np.full(len(y), float("nan"))
        for k in range(3):
            tr, te = keep & (f != k), keep & (f == k)
            if tr.sum() == 0 or te.sum() == 0:
                continue
            m = lgb.train(params, lgb.Dataset(x[tr], label=y[tr], feature_name=NAMES), args.trees)
            oof_without[te] = m.predict(x[te], num_threads=1)
    real, false = y == 1, y == 0
    print(f"{len(y)} final tables from {len(order)} documents: {real.sum()} real, {false.sum()} false")
    if excluded is not None:
        print(f"without {excluded}: {int(keep.sum())} final tables")
    allowed = []
    for t in THRESHOLDS:
        drop_false, n_false, lost_real, n_real = threshold_row(y, oof, t)
        lost = lost_real / max(1, n_real)
        line = (f"  < {t:.2f}: drops {drop_false} false ({drop_false / max(1, n_false):.2f}), "
                f"loses {lost_real} real ({lost:.4f})")
        if excluded is not None:
            w_drop, w_false, w_lost, w_real = threshold_row(y[keep], oof_without[keep], t)
            w_lost_share = w_lost / max(1, w_real)
            line += (f" | without {excluded}: drops {w_drop} false "
                     f"({w_drop / max(1, w_false):.2f}), loses {w_lost} real ({w_lost_share:.4f})")
        print(line)
        if lost <= LOSS_BUDGET:
            allowed.append(t)
    chosen = max(allowed)
    print(f"chosen threshold {chosen:.2f}", flush=True)
    auc_model = auroc(y, oof)
    auc_single = auroc(y, x[:, NAMES.index("run_on_share")])
    auc_without = auroc(y[keep], oof_without[keep]) if excluded is not None else None

    def show(v):
        return f"{v:.4f}" if v is not None else "n/a"

    without = f", without {excluded} {show(auc_without)}" if excluded is not None else ""
    print(f"OOF AUROC: model {show(auc_model)}, run_on_share {show(auc_single)}{without}")
    # Fragments kept for calibration: at the chosen threshold, how many of
    # the kept and refused positives overlap their truth table loosely.
    for bar in (0.7, 0.5):
        frag = real & (ious < bar)
        print(f"  positives with IoU < {bar}: {frag.sum()}, refused {(oof[frag] < chosen).sum()}")
    final = lgb.train(params, lgb.Dataset(x, label=y, feature_name=NAMES), args.trees)
    if args.save:
        final.save_model(args.save)
    if args.model_dir:
        import model_dir
        manifest = {
            "task": "table_is_v2", "schema": SCHEMA, "seed": SEED,
            "corpus": f"final tables: doclaynet train and pubtables-1m train at schema {SCHEMA}",
            "documents": [{"document": d, "source": "pubtables-1m/train" if d.startswith("PMC") else "doclaynet-v1.2/train", "split": "train"} for d in order],
            "rows": {"train": int(len(x)), "positives": int(real.sum()),
                     "negatives": int(false.sum())},
            "cap_per_document": args.cap, "cap_negatives_per_document": cap_neg,
            "pubtables_unverifiable_negatives": args.pubtables_unverifiable_negatives,
            "oof_auroc": auc_model,
            "threshold": chosen, "trees": int(final.num_trees()), "params": params,
            "rust": "crates/pdf-engine/src/learned/models/table_is_v2",
        }
        # What the override let through: the per-class top-five shares after
        # the cap, both AUROCs, and the reason itself when one was given.
        manifest["lopsided"] = {
            name: {"share": info["share"], "rows": info["rows"],
                   "top": [{"document": d, "rows": n} for d, n in info["top"]]}
            for name, info in report.items()
        }
        if excluded is not None:
            manifest[f"oof_auroc_without_{excluded}"] = auc_without
        if args.lopsided_is_fine:
            manifest["lopsided_reason"] = args.lopsided_is_fine
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        skew = training_rules.decisions_skew_manifest()
        if skew is not None:
            manifest["decisions_skew"] = skew
        model_dir.write(args.model_dir, final, x, NAMES, manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
