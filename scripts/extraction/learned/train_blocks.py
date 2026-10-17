#!/usr/bin/env python3
"""Train the block classifier of #599, with the curve, the stop and the calibration.

    train_blocks.py --features ~/Datasets/.../train-*.jsonl.gz \
                    --rows ~/Datasets/.../alignment/train-rows.jsonl \
                    --out models/blocks --curve 5000 20000 0

WHAT IT LEARNS. One class per block, from DocLayNet's own eleven by way of the
join in `align_labels.py`: which of them a block IS, rather than whether our
heuristic would call it a heading. A block whose label the join could not
settle is NOT a training row -- it is counted and left out, because a row whose
label is a guess teaches our own cutting as if it were the page's.

THE SPLIT IS BY DOCUMENT AND SO IS THE CURVE. Pages of one document share a
running head, a measure and a house style; a page-wise split puts half a
document in the training set and the other half in the measurement, and then
the curve measures memory. Documents are drawn by a seeded hash, and the
subsets are NESTED -- 5k is inside 20k is inside all -- so that three points
are one curve rather than three experiments.

EARLY STOPPING, ON THE VALIDATION HALF AND NOT ON THE TRAINING ONE. The table
lane found overfitting without it to be the whole of its problem (#600), and a
tree model on two million rows will happily go on splitting until every leaf is
a page.

AND A CALIBRATION CHECK, because the classifier's output is used as a decision
and not as a ranking: if it says 0.9 for a heading, nine of ten such blocks
have to be headings, or a threshold somewhere else in the engine means
something other than what its author read.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import hashlib
import json
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from features import BLOCK_FEATURES, READABLE, block_vector  # noqa: E402
from document_features import (DOCUMENT_FEATURES, LOCAL_FEATURES,  # noqa: E402
                               of_document)
import training_rules  # noqa: E402

# What the model sees: the block against its page, and the block against its
# DOCUMENT. The second half exists because the first stopped paying: 35 trees
# on 1.4 million rows is a model out of signal rather than out of data, and the
# heading decision is not on the page (#583).
FEATURES = BLOCK_FEATURES + DOCUMENT_FEATURES

SEED = 599
# The fewest documents a point of the curve may rest on, whatever the page
# target: a point drawn from one document says what that document is like.
LEAST_DOCUMENTS = 40
# The classes the engine has to decide between. `picture` is left out: we emit
# nothing for one, so no block of ours can carry it and a class with no rows
# would only distort the others' weights.
CLASSES = ["text", "section_header", "list_item", "caption", "title",
           "page_header", "page_footer", "footnote", "formula", "table"]


def documents_of(paths: list[pathlib.Path]) -> dict[str, int]:
    """Every document in the dump, with the number of pages it has."""
    pages: dict[str, int] = collections.Counter()
    for path in paths:
        for record in read_jsonl(path):
            pages[record.get("document") or record["page_id"]] += 1
    return pages


def drawn(pages: dict[str, int], how_many: int) -> set[str]:
    """Documents in a seeded order until they hold `how_many` PAGES.

    THE CURVE IS IN PAGES AND THE SPLIT IS BY DOCUMENT, which is not a
    contradiction but the whole point. DocLayNet's training split is 69 375
    pages in 2 568 documents, so a curve counted in documents has no room in it
    -- 5 000 and 20 000 are both "all of them" -- while a curve counted in
    pages would, if it drew pages, put half a document in the training set and
    the other half in the measurement. So documents are drawn whole, in the
    order of their hash, until the pages they carry reach the point.

    Nested by construction: the order is the hash and not the count, so the
    documents of the 5 000-page point are inside the 20 000-page one.
    """
    order = sorted(pages, key=lambda name: hashlib.sha256(
        f"{SEED}:{name}".encode()).hexdigest())
    if how_many <= 0:
        return set(order)
    # AND NEVER FEWER THAN `LEAST_DOCUMENTS`. The set has documents of one page
    # and documents of nine hundred, so the first draw of a small point can be
    # a single enormous report -- and then the point measures that report
    # rather than that many pages. The real curve's 5 000-page point came back
    # as ONE document of five pages beside a validation set of 136 660 rows,
    # which is what sent this rule looking.
    taken, so_far = set(), 0
    for name in order:
        if so_far >= how_many and len(taken) >= LEAST_DOCUMENTS:
            break
        taken.add(name)
        so_far += pages[name]
    return taken


def half_of(name: str, share: float = 0.15) -> bool:
    """Whether a document is in the validation half, by its own hash."""
    digest = hashlib.sha256(f"{SEED}:validation:{name}".encode()).hexdigest()
    return int(digest[:8], 16) / 0xFFFFFFFF < share


def read_jsonl(path: pathlib.Path) -> list[dict]:
    # split("\n"), not splitlines(): JSON may carry U+2028 raw (#598).
    if path.suffix == ".gz":
        with gzip.open(path, "rt") as lines:
            return [json.loads(line) for line in lines if line.strip()]
    return [json.loads(line) for line in path.read_text().split("\n") if line.strip()]


def labels_of(path: pathlib.Path) -> dict[tuple[str, int], str]:
    """The class the join gave each block, by page and block index."""
    out: dict[tuple[str, int], str] = {}
    for row in read_jsonl(path):
        if row.get("row") != "block" or not row.get("label"):
            continue
        out[(row["page_id"], row["i"])] = row["label"]
    return out


def matrix(paths: list[pathlib.Path], labels: dict, keep: set[str],
           with_document: bool = True, only_local: bool = False,
           second_generation: str | None = None,
           engine_skew_is_fine: str | None = None):
    """The rows of the documents in `keep`, as features and classes."""
    import numpy as np
    features, classes, groups = [], [], []
    pages_of: dict[str, set] = collections.defaultdict(set)
    # WHERE EACH DOCUMENT CAME FROM, kept because the manifest has to say it:
    # the leakage guard reads a training manifest's documents and asks the
    # dataset register whether that source may be trained on at all.
    source_of: dict[str, str] = {}
    unlabelled = 0
    # THE DOCUMENT'S OWN PAGES TOGETHER. The second half of the vector is about
    # the document -- how many sizes stand above this block, how many blocks
    # share its type -- so a document's pages are gathered before any of its
    # rows is built. The dump is in shard order and a document does not cross a
    # shard, so this holds one document at a time rather than the split.
    held: dict[str, list] = collections.defaultdict(list)
    seen: set[str] = set()
    split_documents: set[str] = set()
    for path in paths:
        for record in read_jsonl(path):
            if record.get("schema") not in READABLE:
                raise SystemExit(f"{path}: schema {record.get('schema')!r}")
            # A DUMP OF A SPLIT WE MEASURE ON IS REFUSED, and so is a dump made
            # with the models on, and so is a dump another engine segmented:
            # see `training_rules` for how each was learnt (#602, codex #1983,
            # #574).
            training_rules.only_the_train_split(record, str(path), second_generation)
            training_rules.not_its_own_output(record, str(path), second_generation)
            training_rules.same_engine(record, str(path), engine_skew_is_fine)
            document = record.get("document") or record["page_id"]
            if keep is not None and document not in keep:
                continue
            source_of.setdefault(document, str(record.get("source") or ""))
            held[document].append(record)
        for document, pages in list(held.items()):
            # A DOCUMENT THAT CROSSES A SHARD would have its document features
            # computed on half of itself, twice, and nothing would say so.
            if document in seen:
                split_documents.add(document)
            seen.add(document)
            of_the_document = of_document(pages) if with_document else {}
            for at_page, record in enumerate(pages):
                pages_of[document].add(record["page_id"])
                for block in record.get("blocks") or []:
                    label = labels.get((record["page_id"], block.get("i")))
                    if label is None or label not in CLASSES:
                        unlabelled += 1
                        continue
                    extra = of_the_document.get(
                        ((record["page_id"], at_page), block.get("i")), [])
                    if only_local:
                        extra = extra[:len(LOCAL_FEATURES)]
                    features.append(block_vector(block) + list(extra)
                                    if with_document else block_vector(block))
                    classes.append(CLASSES.index(label))
                    groups.append(document)
            del held[document]
    if split_documents:
        print(f"[train] {len(split_documents)} document(s) cross a shard and "
              "had their document features computed on a part; the first is "
              f"{sorted(split_documents)[0]}")
    return (np.asarray(features, dtype="float32"),
            np.asarray(classes, dtype="int32"), groups, unlabelled, pages_of,
            source_of)


def calibration(probabilities, truth, bins: int = 10) -> list[dict]:
    """What a probability of 0.9 was worth, in bins of a tenth.

    Reported per bin rather than as one number, because a model can be right on
    average and wrong everywhere: over-confident where it is sure and
    under-confident where it is not averages to nothing.
    """
    import numpy as np
    said = probabilities.max(axis=1)
    guessed = probabilities.argmax(axis=1)
    rows = []
    for at in range(bins):
        low, high = at / bins, (at + 1) / bins
        inside = (said >= low) & (said < high if at + 1 < bins else said <= 1.0)
        if not inside.any():
            continue
        rows.append({"from": round(low, 2), "to": round(high, 2),
                     "blocks": int(inside.sum()),
                     "said": round(float(said[inside].mean()), 4),
                     "were": round(float((guessed[inside] == truth[inside]).mean()), 4)})
    return rows


def train_one(whole, pages, how_many, rounds, leaves, jobs, patience, names,
              binary=None, lopsided_is_fine=None):
    """One point of the curve: train on `how_many` documents, judge on the rest.

    The dump is read ONCE and the points are cut out of it in memory: three
    points over 72 shards would otherwise be six passes over sixty thousand
    pages, and the passes cost more than the training does.
    """
    import lightgbm as lgb
    import numpy as np
    features, classes, groups, left_out, _pages, _sources = whole
    chosen = drawn(pages, how_many)
    training = np.array([name in chosen and not half_of(name) for name in groups])
    validation = np.array([name in chosen and half_of(name) for name in groups])
    # NO DOCUMENT MAY SPEAK FOR THE JUDGING HALF, which picks the tree count
    # and the threshold: one MySQL manual was 34.1 per cent of it (#602). The
    # cap is the fix and the refusal after it is what says the fix worked.
    training = training_rules.capped(groups, training, training_rules.PER_DOCUMENT)
    validation = training_rules.capped(groups, validation,
                                       training_rules.PER_DOCUMENT_JUDGING)
    training_rules.refuse_a_lopsided_split("training", groups, training,
                                           allow=bool(lopsided_is_fine))
    training_rules.refuse_a_lopsided_split("judging", groups, validation,
                                           allow=bool(lopsided_is_fine))
    x_train, y_train = features[training], classes[training]
    x_valid, y_valid = features[validation], classes[validation]
    # ONE CLASS PER LANE, AND SO ONE MODEL PER LANE. A multiclass model is
    # ten sets of trees in one file -- 2.2 MB for this one, over #598's budget
    # before it is even generated as Rust -- and `learned::refine` switches a
    # single class per lane anyway. Measured: a model let loose on every class
    # wins the heading axis 0.060 and loses the list axis 0.194. So the model
    # this lane lands answers one question, `is this block a section header`,
    # and the multiclass one stays the instrument that compares classes.
    if binary is not None:
        y_train = (y_train == binary).astype("int32")
        y_valid = (y_valid == binary).astype("int32")
    model = lgb.train(
        {"objective": "binary" if binary is not None else "multiclass",
         **({} if binary is not None else {"num_class": len(CLASSES)}),
         "learning_rate": 0.1, "num_leaves": leaves, "seed": SEED,
         "num_threads": jobs, "verbose": -1, "deterministic": True,
         "force_row_wise": True},
        lgb.Dataset(x_train, label=y_train, feature_name=names),
        num_boost_round=rounds,
        valid_sets=[lgb.Dataset(x_valid, label=y_valid, feature_name=names)],
        callbacks=[lgb.early_stopping(patience, verbose=False)])
    probabilities = model.predict(x_valid, num_iteration=model.best_iteration)
    if binary is not None:
        probabilities = np.column_stack([1.0 - probabilities, probabilities])
        guessed = (probabilities[:, 1] >= 0.5).astype("int32")
        classes_here = ["not " + CLASSES[binary], CLASSES[binary]]
    else:
        guessed = probabilities.argmax(axis=1)
        classes_here = CLASSES
    per_class = {}
    for index, name in enumerate(classes_here):
        truth = (y_valid == index)
        said = (guessed == index)
        hit = int((truth & said).sum())
        precision = hit / max(1, int(said.sum()))
        recall = hit / max(1, int(truth.sum()))
        per_class[name] = {
            "regions": int(truth.sum()),
            "precision": round(precision, 4), "recall": round(recall, 4),
            "f1": round(0.0 if precision + recall == 0 else
                        2 * precision * recall / (precision + recall), 4)}
    return {"documents": len({n for n in groups if n in chosen and not half_of(n)}),
            "pages": sum(count for name, count in pages.items()
                         if name in chosen and not half_of(name)),
            "validation_documents": len({n for n in groups if n in chosen and half_of(n)}),
            "rows": int(x_train.shape[0]), "validation_rows": int(x_valid.shape[0]),
            "rows_without_a_label": left_out,
            "trees": model.best_iteration,
            "accuracy": round(float((guessed == y_valid).mean()), 4),
            "per_class": per_class,
            "calibration": calibration(probabilities, y_valid)}, model


def write_model_dir(where: pathlib.Path, model, point: dict, whole, names,
                    args) -> None:
    """The model, its manifest and its parity rows, as the exporter reads them.

    The parity rows are a sample of the REAL validation rows rather than made-up
    ones: a fixture of invented numbers proves that Rust adds the trees the same
    way, and a fixture of real pages proves it on the values the engine actually
    produces -- including the NaN a first block has for `prev_size_ratio` and a
    last block for `next_x_frac`, which is where our headings sit.
    """
    import lightgbm as lgb
    import numpy as np
    features, classes, groups, _left_out, pages, sources = whole
    where.mkdir(parents=True, exist_ok=True)
    (where / "model.json").write_text(
        json.dumps(model.dump_model(), indent=1, sort_keys=True) + "\n")
    # The first and last block of a page have a NaN neighbour; a plain head of
    # the matrix would be all first blocks, so the sample is spread.
    step = max(1, len(features) // 500)
    sample = features[::step][:500]
    margins = model.predict(sample, raw_score=True, num_threads=1,
                            num_iteration=model.best_iteration)
    (where / "parity.json").write_text(json.dumps({
        "features": names,
        "rows": [[None if math.isnan(v) else float(v) for v in row]
                 for row in sample.tolist()],
        "margins": [float(m) for m in np.atleast_1d(margins)],
    }, indent=0) + "\n")
    manifest = {
        "task": f"block_is_{args.binary}" if args.binary else "block_class",
        "schema": "pdfluent.features/2",
        "features": names,
        "seed": SEED,
        "trees": point["trees"],
        "leaves": args.leaves,
        "patience": args.patience,
        # THE THREAD COUNT IS PART OF THE RECIPE. `deterministic` and
        # `force_row_wise` make a run repeatable for a GIVEN number of threads;
        # change the count and the sums happen in another order, and the trees
        # come out the same to ten decimal places and different in bytes. T4
        # found that on #602: 36 of 296 trees differed byte for byte with every
        # score identical, so a retrain by anyone following the documented
        # recipe produced a file nobody could match to the checked-in one.
        "threads": args.jobs,
        # THE CAPS ARE PART OF THE RECIPE TOO: the judging half chooses the
        # tree count, so the same dump capped differently is a different model.
        "per_document": training_rules.PER_DOCUMENT,
        "per_document_judging": training_rules.PER_DOCUMENT_JUDGING,
        "lightgbm": lgb.__version__,
        "rows": point["rows"],
        "validation_rows": point["validation_rows"],
        "rows_without_a_label": point["rows_without_a_label"],
        # ONE ROW PER DOCUMENT, not a bare name: the leakage guard matches a
        # training document against the rig by sha, page id or name, and asks
        # the dataset register whether its SOURCE may be trained on. A list of
        # strings crashed it (#599), which is the failure mode a guard is least
        # able to afford.
        "documents": [{"document": name, "source": sources.get(name, ""),
                       "split": "validation" if half_of(name) else "train"}
                      for name in sorted({name for name in groups})],
        # WHERE THE EXPORTER PUT IT, and the threshold it was exported with:
        # the reproducibility guard re-exports the model and compares, and
        # without these two it cannot find the file or make the same bytes.
        "rust": args.rust,
        "threshold": args.threshold,
        "pages": sum(len(seen) for seen in pages.values()),
        "per_class": point["per_class"],
        "calibration": point["calibration"],
    }
    # Present only when the dump was segmented by another tree and the run
    # said why: a model that learnt another engine's blocks says so (#574).
    skew = training_rules.engine_skew_manifest()
    if skew is not None:
        manifest["engine_skew"] = skew
    (where / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
    print(f"[train] model, manifest and parity rows written to {where}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--features", nargs="+", required=True, type=pathlib.Path)
    ap.add_argument("--rows", required=True, type=pathlib.Path)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--curve", nargs="+", type=int, default=[5000, 20000, 0],
                    help="PAGES per point, drawn as whole documents; 0 means "
                         "every document")
    ap.add_argument("--rounds", type=int, default=400)
    ap.add_argument("--leaves", type=int, default=63)
    ap.add_argument("--jobs", type=int, default=10)
    ap.add_argument("--second-generation", default=None, metavar="REASON",
                    help="train on a dump made with the learned pass ON, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--lopsided-is-fine", default=None, metavar="REASON",
                    help="train although five documents carry more than a "
                         "fifth of a half after the cap, and say why")
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--rust", default=None,
                    help="where export_rust.py puts this model in the engine, "
                         "recorded in the manifest so the reproducibility "
                         "guard can re-export and compare")
    ap.add_argument("--threshold", type=float, default=None,
                    help="the probability the engine ships this model at, "
                         "recorded with it for the same reason")
    ap.add_argument("--patience", type=int, default=25,
                    help="rounds without improvement before stopping; the same "
                         "for every point of a curve, or the points differ by "
                         "where the stop happened rather than by the data")
    ap.add_argument("--local", action="store_true",
                    help="the block features plus the block-local additions, "
                         "without the ones that describe the document: the "
                         "test of whether document-scoped features let a model "
                         "recognise the document instead of the block")
    ap.add_argument("--model-dir", type=pathlib.Path, default=None,
                    help="write the model, its manifest and its parity rows "
                         "the way the exporter and the reproducibility test "
                         "read them, from the LAST point of the curve")
    ap.add_argument("--binary", default=None, choices=CLASSES,
                    help="train ONE class against the rest, which is what a "
                         "lane switches on and what fits the exporter and the "
                         "size budget of #598")
    ap.add_argument("--plain", action="store_true",
                    help="the block features alone, without the document ones: "
                         "the control for whether they pay")
    args = ap.parse_args()

    labels = labels_of(args.rows)
    print(f"[train] {len(labels)} labelled block(s); reading the dump once")
    names = (BLOCK_FEATURES if args.plain else
             BLOCK_FEATURES + LOCAL_FEATURES if args.local else FEATURES)
    whole = matrix(args.features, labels, keep=None,
                   with_document=not args.plain,
                   only_local=args.local,
                   second_generation=args.second_generation,
                   engine_skew_is_fine=args.engine_skew_is_fine)
    documents = {name: len(pages) for name, pages in whole[4].items()}
    print(f"[train] {len(documents)} document(s), {sum(documents.values())} page(s), "
          f"{len(whole[1])} labelled row(s), {whole[3]} row(s) the join could not "
          "label")
    args.out.mkdir(parents=True, exist_ok=True)
    curve = []
    last = None
    for how_many in args.curve:
        point, model = train_one(whole, documents, how_many, args.rounds,
                                 args.leaves, args.jobs, args.patience, names,
                                 CLASSES.index(args.binary) if args.binary else None,
                                 args.lopsided_is_fine)
        curve.append(point)
        print(f"[train] {point['documents']} documents, {point['pages']} pages, "
              f"{point['rows']} rows, "
              f"{point['trees']} trees: accuracy {point['accuracy']}, "
              f"heading F1 {point['per_class']['section_header']['f1']}")
        model.save_model(str(args.out / f"blocks-{how_many or 'all'}.txt"),
                         num_iteration=model.best_iteration)
        last = (model, point)
    if args.model_dir and last is not None:
        write_model_dir(args.model_dir, last[0], last[1], whole, names, args)
    (args.out / "curve.json").write_text(json.dumps(curve, indent=1) + "\n")
    print(f"[train] curve written to {args.out / 'curve.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
