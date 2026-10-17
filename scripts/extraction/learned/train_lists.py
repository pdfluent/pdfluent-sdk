#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Train the two list models from DocLayNet train (#599).

    $VENV/bin/python scripts/extraction/learned/train_lists.py \
        --features ~/Datasets/pdfluent-training/doclaynet/features \
        --rows ~/Datasets/pdfluent-training/doclaynet/alignment/train-rows.jsonl

  list_opens  per LINE: does a new list item open here? Label: the line opens a
              `list_item` region (T3's join, `opens` decided over the page).
  list_item   per BLOCK: is this block a list item? Label: the region the block
              belongs to is a `list_item`. Input: the block vector and the
              vector of its first line.

WHAT IS EXCLUDED, AND WHY IT IS NOT A NEGATIVE. A line or block the join could
not give to one region (`region: null`) is left out and counted. Its truth is
unknown, and calling it "not an item" would teach the model that on exactly the
lines where our own cut is wrong (T3).

A DUMP MADE WITH THE MODELS ON IS REFUSED. Once `learned::refine` runs, a
block's kind is the model's decision, and a model trained on it learns its own
output -- which reads as progress. The dump writes `refine`; a record with
`refine: "on"` stops this script unless `--second-generation` says why (T3).
So does a record another engine tree segmented, unless `--engine-skew-is-fine`
says why -- its blocks are not those of the engine that will read the model,
and the reason is kept in the manifest (#574).

HOW THE SPLIT IS MADE. By document, 70/15/15 on a hash of the document name, so
no document is on both sides. Training stops early on the validation part, and
the decision threshold is chosen there too; the test part is only read to
report. None of it is the rig: the dump skips every rig document, and the
leakage guard checks the manifest again.

WHAT IT WRITES, per model, into scripts/extraction/learned/models/<name>/:
  model.json.gz   LightGBM's own dump, gzipped (the repository keeps files
                  under 1 MB, #599), written deterministically.
  manifest.json   the documents trained on (by name; the pages as a digest),
                  the split, the threshold, the test scores.
  parity.json     a sample of rows with LightGBM's raw margins, for the Rust
                  parity test.
and the Rust into crates/pdf-engine/src/learned/models/<name>/ by export_rust.py.
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

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(HERE))
from features import BLOCK_FEATURES, block_vector  # noqa: E402
from list_lines import LINE_FEATURES, heads_on_page, line_vectors, prev_colon  # noqa: E402
import training_rules  # noqa: E402

SEED = 599
MODELS = {
    # Trees capped so the generated file stays inside the budget (#599: at
    # most 5 MB and 5,000 comparisons per prediction).
    # Trained and reported, not shipped: no operator page makes it fire, and a
    # decision nobody can show turning red with its switch off does not ship
    # (#599). It ships with a real page as fixture.
    "list_opens": {"max_trees": 450, "ship": True},
    # THE THRESHOLD IS PINNED, END TO END. With the line class model's list
    # probability among its inputs (#574, arm d) the block-level choice on
    # validation reads 0.55; through the whole pass on #632's proxy (169 pages
    # with lists) that threshold gains 0.007, while 0.40 gains 0.020 per page
    # and per document (0.35 the same, 0.45 +0.017, 0.65 -0.003): demotion of
    # a confirmed run and promotion of a paragraph use one number, and the
    # block-level pick cannot see the first.
    "list_item": {"max_trees": 600, "ship": True, "threshold": 0.40},
    # A heading swallowed by the block under it (#602): 18,813 of DocLayNet
    # train's 118,590 section_header regions, 15.9 per cent. The rule proposed
    # for it reached F1 11.2 and its decisive clause was false by construction,
    # so it is a model -- the same action and the same line features as
    # `list_opens`, which is the boundary this repeats.
    # Trained and reported, NOT shipped since #574: `heading_lines` answers the
    # same boundary per line, and the 2.56 MB it took the budget needed.
    "heading_ends": {"max_trees": 450, "ship": False},
}
PARAMS = {
    "objective": "binary", "num_leaves": 31, "max_depth": 8, "learning_rate": 0.05,
    "min_data_in_leaf": 100, "lambda_l2": 5.0, "feature_fraction": 0.8,
    "bagging_fraction": 0.8, "bagging_freq": 1, "seed": SEED,
    "deterministic": True, "force_row_wise": True, "num_threads": 4,
    "verbosity": -1,
}


# What export_rust.py reads of a tree, and nothing else. LightGBM's dump also
# carries gains, counts and internal values per node; they describe the
# training, not the model, and they are what pushes a 500-tree dump over the
# repository's 1 MB per file (#599).
KEPT = {"split_feature", "threshold", "decision_type", "default_left",
        "missing_type", "left_child", "right_child", "leaf_value"}


def slim(node: dict) -> dict:
    out = {k: v for k, v in node.items() if k in KEPT}
    for child in ("left_child", "right_child"):
        if child in out:
            out[child] = slim(out[child])
    return out


def slim_dump(dump: dict) -> dict:
    out = {"feature_names": dump["feature_names"],
           "tree_info": [{"tree_structure": slim(t["tree_structure"])} for t in dump["tree_info"]]}
    # A MULTICLASS MODEL SAYS HOW MANY CLASSES IT HAS, or the exporter reads
    # its interleaved trees as one class and sums them all (#574).
    if dump.get("num_class", 1) > 1:
        out["num_class"] = dump["num_class"]
    return out


def write_dump(dump: dict, path: pathlib.Path) -> None:
    """The slimmed dump, gzipped with no timestamp and sorted keys: a retrain on
    the same data is the same bytes."""
    with open(path, "wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="") as gz:
        gz.write((json.dumps(slim_dump(dump), sort_keys=True) + "\n").encode())


def third(document: str) -> str:
    h = int(hashlib.sha256(document.encode()).hexdigest(), 16) % 100
    return "train" if h < 70 else ("validation" if h < 85 else "test")


def read_labels(rows: pathlib.Path):
    """`opens` per line for the list model, the raw label per line for the
    heading boundary, and the block label for the block model.

    THE RAW LABEL IS KEPT because a boundary is a fact about TWO lines: a
    heading boundary falls where the region changes and one side of the change
    is a `section_header`. A bool per line cannot say that, and deriving it
    here rather than in the caller keeps the one place that reads the join.
    """
    lines, labels = collections.defaultdict(dict), collections.defaultdict(dict)
    blocks = collections.defaultdict(dict)
    excluded = collections.Counter()
    with rows.open(encoding="utf-8") as f:
        for text in f:
            r = json.loads(text)
            kind = r.get("row")
            if r.get("region") is None:
                excluded[kind] += 1
                continue
            if kind == "line":
                lines[r["page_id"]][(r["i"], r["line"])] = r["label"] == "list_item" and bool(r["opens"])
                labels[r["page_id"]][(r["i"], r["line"])] = r["label"]
            elif kind == "block":
                blocks[r["page_id"]][r["i"]] = r["label"] == "list_item"
    return lines, labels, blocks, excluded


def heading_boundary(labels: dict, block: int, line: int) -> bool | None:
    """Does a new block open at `line` because a heading begins or ends there?

    `None` where the join could not label one of the two lines: its truth is
    unknown, and calling it "no boundary" would teach the model that on exactly
    the lines where our own cut is wrong (T3's rule for the list models, and
    the same reasoning here).

    ONE DIRECTION ONLY: the heading is ABOVE the cut. Both directions occur --
    measured over DocLayNet train (#602), 83.8 per cent of swallowed headings
    are a block that BEGINS with one, and 6.0 per cent have the heading further
    down -- and it is tempting to label them together, since both are "a new
    block opens at line N". They are not one action. What the apply step does
    with the answer is promote the part ABOVE the cut to a heading, which is
    right for the first shape and wrong for the second; a model that had
    learned both would confidently promote a paragraph. So the 6.0 per cent is
    left out and counted, not folded in, and the name says which direction this
    is.
    """
    if line == 0:
        return False
    here = labels.get((block, line))
    above = labels.get((block, line - 1))
    if here is None or above is None:
        return None
    if not (above == "section_header" and here != "section_header"):
        return False
    # AND THE HEADING MUST OPEN THE BLOCK, which reading only the line above
    # cannot tell. On `text, section_header, text` the pair at line 2 looks
    # exactly like the pair at line 1 of `section_header, text`, and the apply
    # step promotes everything ABOVE the cut -- so a model taught on the first
    # shape promotes a paragraph with a heading under it (codex, #1983). This
    # is the 6.0 per cent the docstring above says is left out, and until now
    # it was being folded in as a positive.
    if any(labels.get((block, at)) != "section_header" for at in range(line)):
        return None
    return True


def self_test() -> int:
    """The label predicates, on the shapes they are wrong about when wrong.

    A training label is not visible in anything the model does afterwards: a
    wrong one costs a fraction of a point on a curve nobody can attribute, so
    the shapes are pinned here rather than argued about later.
    """
    HEAD, BODY = "section_header", "text"
    bad = 0

    def check(name, got, want):
        nonlocal bad
        if got is not want:
            bad += 1
            print(f"FAIL {name}: {got!r}, wanted {want!r}")
        else:
            print(f"ok   {name}")

    def lines(*kinds):
        return {(0, at): kind for at, kind in enumerate(kinds)}

    check("a heading that opens the block is a boundary",
          heading_boundary(lines(HEAD, BODY), 0, 1), True)
    check("and two heading lines still open it",
          heading_boundary(lines(HEAD, HEAD, BODY), 0, 2), True)
    # THE ONE THIS TEST EXISTS FOR. Reading only the line above cannot tell
    # `text, section_header, text` from `section_header, text`, and the apply
    # step promotes everything above the cut -- so this shape taught as a
    # positive teaches the model to promote a paragraph (codex, #1983).
    check("a heading further down is left out, not labelled",
          heading_boundary(lines(BODY, HEAD, BODY), 0, 2), None)
    check("two body lines are not a boundary",
          heading_boundary(lines(BODY, BODY), 0, 1), False)
    check("a line the join could not label is unknown",
          heading_boundary({(0, 0): HEAD}, 0, 1), None)
    check("the first line of a block opens nothing",
          heading_boundary(lines(HEAD, BODY), 0, 0), False)

    # THE OTHER DIRECTION, and the two must not agree about the same line: one
    # says the piece above is a heading, the other says the piece below is, and
    # a line both of them called a boundary would be a heading on both sides.
    check("a heading that begins under body text is a boundary",
          heading_begins(lines(BODY, HEAD), 0, 1), True)
    check("and the line where that heading ends is not one of these",
          heading_begins(lines(BODY, HEAD, BODY), 0, 2), False)
    check("which is where the other predicate speaks",
          heading_boundary(lines(BODY, HEAD, BODY), 0, 2), None)
    check("a block that opens with a heading needs no cut here",
          heading_begins(lines(HEAD, BODY), 0, 0), False)
    check("and its second line is not a heading beginning",
          heading_begins(lines(HEAD, BODY), 0, 1), False)
    check("an unknown line above is unknown here too",
          heading_begins({(0, 1): HEAD}, 0, 1), None)

    print(("FAIL" if bad else "OK") + f": {12 - bad} of 12 label shapes")
    return 1 if bad else 0


def heading_begins(labels: dict, block: int, line: int) -> bool | None:
    """Does a new block open at `line` because a heading BEGINS there?

    REFUSED ONCE AND TRIED AGAIN (#602), which is the whole story of this
    predicate and the reason the first reckoning is kept below.

    THE CASE FOR IT. After `heading_ends` has run, 335 swallowed headings are
    still buried on 1 200 validation pages, and 156 of them are a heading that
    is not the top of its block -- 111 in the middle, 45 at the bottom. Worth
    0.040 on the heading axis by oracle repair, and 86 per cent of them have
    whole lines of their own, so a line-level cut can reach them.

    WHAT HAPPENED. Trained the same way as `heading_ends` on the same rows and
    the same features, it found 1 061 positives against that model's 10 351 --
    the shape is 1.1 per cent of the blocks that hold a heading line in
    DocLayNet train, however large it looms in what is LEFT after the common
    shape is repaired. On its own test it reads precision 0.55 at recall 0.64;
    run through the engine over those 1 200 pages it cut 17 times where 134
    reachable cases stand, and the heading axis moved -0.0018 / -0.0003 /
    +0.0002 / -0.0002 at thresholds 0.05 / 0.15 / 0.30 / 0.50. Of the 45 cuts
    it makes at its loosest, 3 land on a heading the annotation marks.

    WHY IT WAS THE EXAMPLES AND NOT THE FEATURES: the same line features
    separate this shape well enough in training (recall 0.64), and the model
    then found an eighth of it on unseen pages. That is a model out of
    EXAMPLES, not one that cannot see. The lever named then was the join --
    22.5 per cent of these blocks had no label at all on the line above the
    heading -- and it turned out to be the line GROUPING underneath it: two
    columns at one height were one line, so the line above a buried heading
    belonged to no region and carried no label.

    AND WITH THE COLUMNS SPLIT IT WAS REFUSED AGAIN, which is the second
    reckoning this docstring promised. The same rows now give 4 528 positives
    instead of 1 061 and the model is better at its own task -- precision 0.553
    -> 0.637, F1 0.594 -> 0.619 -- and on the axis it is worth -0.0002, -0.0001
    and +0.0001 at thresholds 0.25, 0.40 and 0.55, run through the engine with
    `heading_ends` already on.

    SO THE DIAGNOSIS WAS RIGHT AND THE CURE WAS NOT. Examples were what it
    lacked; four times as many made it a better model and not a useful one. The
    cuts it adds are real boundaries that the annotation does not mark as
    headings -- 89 new headings over 1 200 pages of which 18 are marked -- and
    no threshold separates the two populations. Whoever reaches for this next
    should be asking what a heading buried in the middle of a block IS to a
    reader, not how to find the line it starts on; we can find that line.

    The same correction is worth +0.0149 to `heading_ends`, which is trained on
    the same rows and did not need a new action at all.

    The mirror of `heading_boundary`, and the other half of one shape: a
    heading buried in a block has a line where it starts and a line where it
    ends, and the two are different actions on different lines. This one marks
    the line the heading's first line IS, so the piece BELOW the cut is the
    heading; that one marks the line after its last, so the piece ABOVE is.

    Together they cut a heading out of the middle of a block, and neither of
    them has to know about the other: each says where a boundary is, and what
    the piece between two boundaries IS stays the class model's question.

    NOT AT LINE 0. A block that begins with a heading needs no cut here --
    `heading_ends` is the action for it -- and a "boundary" at the first line
    of a block is not a boundary at all.

    `None` where the join could not label one of the two lines, for the same
    reason as everywhere else in this file: its truth is unknown, and calling
    it "no boundary" teaches the model that on exactly the lines where our own
    cut is wrong.
    """
    if line == 0:
        return False
    here = labels.get((block, line))
    above = labels.get((block, line - 1))
    if here is None or above is None:
        return None
    return here == "section_header" and above != "section_header"


def records(features: pathlib.Path, second_generation: str | None,
            engine_skew_is_fine: str | None = None):
    for path in sorted(features.glob("train-*.jsonl.gz")):
        with gzip.open(path, "rt", encoding="utf-8") as f:
            for text in f:
                r = json.loads(text)
                # ANYTHING THAT IS NOT "off", not the one word "on". The dump
                # stamps the switch SET it ran (`--refine heading,heading_ends`
                # writes exactly that), so a check for "on" passed every named
                # set straight through -- which is the whole class of dump this
                # guard exists for (codex, #1983).
                said = str(r.get("refine") or "off").strip()
                if said != "off" and not second_generation:
                    raise SystemExit(
                        f"{path.name}: a record dumped with the learned pass ON ({said!r}). Its "
                        "kinds are the models' own decisions; training on them teaches a model "
                        "its own output. Dump with refine off, or pass --second-generation with "
                        "a reason.")
                # AND A DUMP ANOTHER ENGINE SEGMENTED: the blocks of this dump
                # are not those of the engine that will read the model (#574).
                training_rules.same_engine(r, path.name, engine_skew_is_fine)
                yield r


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=pathlib.Path)
    ap.add_argument("--rows", type=pathlib.Path)
    ap.add_argument("--models", type=pathlib.Path, default=HERE / "models")
    ap.add_argument("--rust", type=pathlib.Path, default=REPO / "crates/pdf-engine/src/learned/models")
    ap.add_argument("--second-generation", default=None, metavar="REASON")
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--self-test", action="store_true",
                    help="the label predicates, on the shapes they are wrong "
                         "about when wrong; reads no dump")
    ap.add_argument("--threads", type=int, default=None,
                    help="override LightGBM's thread count. NOT the fix for "
                         "the segfault on this Mac -- `setup_venv.sh` is, and "
                         "it refuses to run with KMP_DUPLICATE_LIB_OK set "
                         "because that hides the crash rather than removing "
                         "its cause. Run every training through "
                         "~/.cache/pdfluent/learned-venv/bin/python")
    ap.add_argument("--line-classes", type=pathlib.Path, default=None, metavar="OOF.jsonl.gz",
                    help="`train_heading_lines.py --oof`'s file: the line class "
                         "model's list probability for every line, out of fold on "
                         "the train third. With it `list_item` reads three more "
                         "columns, the mean, the largest and the first line's "
                         "(#574, arm d)")
    ap.add_argument("--only", default=None, metavar="NAME",
                    help="train just this model. Without it every model here "
                         "is retrained and rewritten, and a shipped model that "
                         "changes because a library moved under it is a change "
                         "nobody asked for")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.features or not args.rows:
        ap.error("--features and --rows, or --self-test")
    wanted = {args.only: MODELS[args.only]} if args.only else MODELS
    if args.only and args.only not in MODELS:
        raise SystemExit(f"--only {args.only}: not one of {', '.join(MODELS)}")

    import lightgbm as lgb
    import numpy as np

    params = dict(PARAMS)
    if args.threads:
        params["num_threads"] = args.threads

    line_labels, raw_labels, block_labels, excluded = read_labels(args.rows)
    # THE LINE MODEL'S LIST PROBABILITY, per page and line (#574). Index 2 is
    # `list` in the class model's frozen order.
    p_list: dict[str, dict[tuple[int, int], float]] = {}
    if args.line_classes:
        with gzip.open(args.line_classes, "rt", encoding="utf-8") as f:
            for text in f:
                r = json.loads(text)
                p_list[r["page_id"]] = {(b, k): p[2] for (b, k), p in zip(r["lines"], r["p"])}
    data = {name: {"x": [], "y": [], "part": [], "doc": []} for name in MODELS}
    pages_by_document: dict[str, list[str]] = collections.defaultdict(list)
    for r in records(args.features, args.second_generation,
                    args.engine_skew_is_fine):
        document = r.get("document") or r["pdf_sha256"]
        pages_by_document[document].append(r["pdf_sha256"])
        part = third(document)
        ll, bl = line_labels.get(r["page_id"], {}), block_labels.get(r["page_id"], {})
        rl = raw_labels.get(r["page_id"], {})
        heads = heads_on_page(r["blocks"], r["words"])
        for at, b in enumerate(r["blocks"]):
            vectors = line_vectors(b, r["words"], r.get("body_size") or 1.0, heads, prev_colon(r["blocks"], at))
            for k, v in enumerate(vectors):
                if (b["i"], k) in ll:
                    d = data["list_opens"]
                    d["x"].append(v); d["y"].append(int(ll[(b["i"], k)])); d["part"].append(part); d["doc"].append(document)
                # NEGATIVES FROM EVERY BLOCK, not only from the blocks that
                # hold a swallowed heading. Measured the other way the same
                # rule read 86.8 per cent precision instead of 6.0 -- fifteen
                # times, from the choice of where the negatives come from
                # (#602).
                cut = heading_boundary(rl, b["i"], k)
                if cut is not None:
                    d = data["heading_ends"]
                    d["x"].append(v); d["y"].append(int(cut)); d["part"].append(part); d["doc"].append(document)
            if b["i"] in bl and b.get("kind") in ("paragraph", "list_item"):
                d = data["list_item"]
                row = block_vector(b) + (vectors[0] if vectors else [math.nan] * len(LINE_FEATURES))
                if args.line_classes:
                    said = p_list.get(r["page_id"], {})
                    ps = [said[(b["i"], k)] for k in range(len(vectors)) if (b["i"], k) in said]
                    row += [sum(ps) / len(ps), max(ps), ps[0]] if ps else [math.nan] * 3
                d["x"].append(row)
                d["y"].append(int(bl[b["i"]])); d["part"].append(part); d["doc"].append(document)

    names = {"list_opens": LINE_FEATURES,
             "heading_ends": LINE_FEATURES,
             "list_item": BLOCK_FEATURES + [f"first_line_{n}" for n in LINE_FEATURES]
             + (["line_p_list_mean", "line_p_list_max", "line_p_list_first"] if args.line_classes else [])}
    for name, spec in wanted.items():
        d = data[name]
        x = np.asarray(d["x"], dtype=np.float64); y = np.asarray(d["y"], dtype=np.int32)
        part = np.asarray(d["part"])
        tr, va, te = part == "train", part == "validation", part == "test"
        booster = lgb.train(params, lgb.Dataset(x[tr], label=y[tr], feature_name=names[name]),
                            num_boost_round=spec["max_trees"],
                            valid_sets=[lgb.Dataset(x[va], label=y[va])],
                            callbacks=[lgb.early_stopping(50, verbose=False)])
        trees = booster.best_iteration or spec["max_trees"]

        def f1(prob, truth, threshold):
            pred = prob >= threshold
            tp = int((pred & truth).sum()); fp = int((pred & ~truth).sum()); fn = int((~pred & truth).sum())
            p = tp / (tp + fp) if tp + fp else 0.0; r_ = tp / (tp + fn) if tp + fn else 0.0
            return {"precision": round(p, 4), "recall": round(r_, 4),
                    "f1": round(2 * p * r_ / (p + r_), 4) if p + r_ else 0.0}

        pv = booster.predict(x[va], num_iteration=trees, num_threads=1)
        threshold = spec.get("threshold") or max(((f1(pv, y[va].astype(bool), t)["f1"], round(float(t), 2))
                         for t in np.arange(0.10, 0.91, 0.05)))[1]
        pt = booster.predict(x[te], num_iteration=trees, num_threads=1)
        if not spec["ship"]:
            print(f"{name} (not shipped): {trees} trees, threshold {threshold}, "
                  f"test {f1(pt, y[te].astype(bool), threshold)}")
            continue
        out = args.models / name
        out.mkdir(parents=True, exist_ok=True)
        dump = booster.dump_model(num_iteration=trees)
        dump["tree_info"] = dump["tree_info"][:trees]
        write_dump(dump, out / "model.json.gz")
        rng = np.random.default_rng(SEED)
        sample = rng.choice(len(x), size=min(300, len(x)), replace=False)
        sample.sort()
        margins = booster.predict(x[sample], num_iteration=trees, raw_score=True, num_threads=1)
        (out / "parity.json").write_text(json.dumps({
            "features": names[name],
            "rows": [[None if math.isnan(v) else v for v in row] for row in x[sample].tolist()],
            "margins": [float(m) for m in margins],
        }) + "\n")
        documents = sorted(set(d["doc"]))
        manifest = {
            "task": name,
            "rust": f"crates/pdf-engine/src/learned/models/{name}",
            "schema": "pdfluent.features/1",
            "features": names[name],
            "seed": SEED, "params": params, "trees": trees,
            "lightgbm": lgb.__version__,
            "threshold": threshold,
            "rows": {"train": int(tr.sum()), "validation": int(va.sum()), "test": int(te.sum())},
            "positives": int(y.sum()),
            "excluded_without_a_region": {k: int(v) for k, v in excluded.items()},
            "test": {"model": f1(pt, y[te].astype(bool), threshold)},
            "pages_sha256_digest": hashlib.sha256("\n".join(sorted(
                s for doc in documents for s in pages_by_document[doc])).encode()).hexdigest(),
            "documents": [{"source": "doclaynet-v1.2/train", "document": doc, "split": third(doc)}
                          for doc in documents],
        }
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        (out / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
        print(f"{name}: {trees} trees, threshold {threshold}, test {f1(pt, y[te].astype(bool), threshold)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
