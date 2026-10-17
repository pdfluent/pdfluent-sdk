#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Train the LINE class model from DocLayNet train (#574, #632).

    $VENV/bin/python scripts/extraction/learned/train_heading_lines.py \
        --features ~/Datasets/pdfluent-training/doclaynet/features \
        --rows ~/Datasets/pdfluent-training/doclaynet/alignment/train-rows-ordered.jsonl

  line_classes   per LINE: other, heading, list or table -- the classes the
                 structured axes score. `section_header` and `title` are a
                 heading, as the heading axis folds them; `list_item` a list;
                 `table` a table; every other region other. One model and not
                 three because the three lanes read one row (#574): the
                 heading lane decides from `p[1]`, the list model reads `p[2]`
                 as a feature, and the table judge proposes runs of `p[3]`.

                 THE CLASS ORDER IS FROZEN (`CLASSES`); a model reads its
                 output by index too.

THE BINARY `heading_lines` model that decides headings was trained by this
script's binary form (master da7aa418a); its manifest records the recipe. This
form trains the class model beside it, which decides nothing and is measured
worse at deciding headings (per document 0.8070 against 0.7673).

WHY A LINE AND NOT A BLOCK. The block model decides a block we already cut, so
a heading glued to the paragraph under it, or a paragraph set in the heading's
type, is decided wrong before the model sees it. #632 measured the other way on
the same proxy: per-line labels with the neighbouring lines as context read
0.757 on headings against 0.684 for the shipped block model, and this model --
which reads only what the engine already computes with a parity test -- 0.777.

WHAT IT READS, and nothing new: the line's own `LINE_FEATURES` (the list
lane's, held to Rust by `list_lines.py --self-test` and the parity test), the
block's whole vector as the shipped block model reads it (`BLOCK_FEATURES`
plus the six `LOCAL_FEATURES` that say what the document's body face and type
are -- without them manuals lost 0.15 on the heading axis), and for the two lines above and the
two below on the page -- in reading order, across blocks -- twelve of their line
features and whether they sit in the same block. See `vector`.

THE SPLIT is `train_lists.third`, by document, so the lanes split alike, and
every rule in `training_rules` holds: train shards only, no dump made with the
learned pass on, no dump another engine tree segmented, the judging part
capped per document, and a part that five documents carry more than a fifth of
is refused.

PAGES PER DOCUMENT are capped too (`--pages-per-document`), deterministically
by page hash: a 900-page manual is otherwise a third of the lines, and the
whole train split does not fit in the Mac's memory as line rows.
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
import training_rules  # noqa: E402
from features import BLOCK_FEATURES, block_vector  # noqa: E402
from document_features import LOCAL_FEATURES, of_document  # noqa: E402
from list_lines import LINE_FEATURES, _box, _size, heads_on_page, line_vectors, prev_colon  # noqa: E402
from align_labels import lines_of  # noqa: E402
from train_lists import SEED, third, write_dump  # noqa: E402

NAME = "line_classes"
CLASSES = ["other", "heading", "list", "table"]
CLASS_OF = {"section_header": 1, "title": 1, "list_item": 2, "table": 3}
# How far the context reaches, in lines, each way.
REACH = 2
# The line features a neighbour contributes, by name.
CONTEXT: list[str] = [
    "marker_bullet", "marker_ordered", "first_token_chars", "indent_em",
    "prev_ends_terminal", "width_frac", "size_ratio", "bold", "gap_above_em",
    "block_is_list", "block_is_table_cell", "max_gap_em",
]
CONTEXT_AT = [LINE_FEATURES.index(n) for n in CONTEXT]
OFFSETS = [o for o in range(-REACH, REACH + 1) if o]
FEATURES: list[str] = (
    list(LINE_FEATURES)
    + [f"of_block_{n}" for n in BLOCK_FEATURES + LOCAL_FEATURES]
    + [f"{'up' if o < 0 else 'down'}{abs(o)}_{n}" for o in OFFSETS for n in CONTEXT + ["same_block"]]
)
# Per CLASS: four classes are four sets of trees, and 80 each is what the
# 15 MB budget holds beside the binary `heading_lines` model, which still
# decides headings (~5.7 KB a tree). The classes are a SIGNAL for the list model
# and the table judge; measured on the test part, 100 per class reads heading
# 0.773, list 0.820, table 0.905 per line, 400 per class 0.814, 0.854, 0.925.
MAX_TREES = 80
PARAMS = {
    "objective": "multiclass", "num_class": 4, "num_leaves": 31, "max_depth": 8, "learning_rate": 0.1,
    "min_data_in_leaf": 50, "feature_fraction": 0.8, "seed": SEED,
    "deterministic": True, "force_row_wise": True, "num_threads": 4,
    "verbosity": -1,
}


def page_lines(record: dict, document: dict | None = None,
               at_page: int = 0) -> list[tuple[int, int, list[float], list[float]]]:
    """Every line of the page in reading order: (block i, line k, line vector,
    block vector) -- blocks in the page's order, lines in `lines_of`'s.

    `document` is `of_document` over the document's pages, and `at_page` this
    page's place among them; its local part completes the block vector."""
    words = record.get("words") or []
    blocks = record.get("blocks") or []
    heads = heads_on_page(blocks, words)
    out = []
    for at, block in enumerate(blocks):
        vectors = line_vectors(block, words, record.get("body_size") or 1.0, heads,
                               prev_colon(blocks, at))
        extra = (document or {}).get(((record.get("page_id"), at_page), block.get("i")), [])
        extra = list(extra[:len(LOCAL_FEATURES)])
        whole = block_vector(block) + extra + [math.nan] * (len(LOCAL_FEATURES) - len(extra))
        for k, v in enumerate(vectors):
            out.append((block["i"], k, v, whole))
    return out


PICTURE_FEATURES = ["picture_distance_em", "picture_above", "picture_below", "picture_under"]
NEAR_EM = 3.0


def picture_features(bbox: list[float], size: float, images: list) -> list[float]:
    """Where a line stands against the page's images, as
    `learned::captions::picture_features` measures it (#636)."""
    if not images:
        return [math.nan, 0.0, 0.0, 0.0]
    x0, y0, x1, y1 = bbox
    em = size if size > 0 else 1.0
    best, above, below, under = math.inf, 0.0, 0.0, 0.0
    for a0, b0, a1, b1 in images:
        dx = max(a0 - x1, x0 - a1, 0.0)
        dy = max(b0 - y1, y0 - b1, 0.0)
        best = min(best, math.hypot(dx, dy) / em)
        across = min(x1, a1) - max(x0, a0) > 0
        if across and 0.0 <= b0 - y1 <= NEAR_EM * em:
            above = 1.0
        if across and 0.0 <= y0 - b1 <= NEAR_EM * em:
            below = 1.0
        if dx == 0 and dy == 0:
            under = 1.0
    return [best, above, below, under]


def page_boxes(record: dict) -> list[tuple[list[float], float]]:
    """Each line's box and median size, in `page_lines`' order."""
    words = record.get("words") or []
    return [(_box(line), _size(line))
            for block in record.get("blocks") or [] for line in lines_of(block, words)]


def vector(lines: list, at: int) -> list[float]:
    """The line at `at` with its block and its neighbours, in `FEATURES` order."""
    block, _k, own, whole = lines[at]
    row = list(own) + list(whole)
    for off in OFFSETS:
        j = at + off
        if 0 <= j < len(lines):
            other = lines[j]
            row += [other[2][c] for c in CONTEXT_AT] + [1.0 if other[0] == block else 0.0]
        else:
            row += [math.nan] * (len(CONTEXT_AT) + 1)
    return row


def records(features: pathlib.Path, second_generation: str | None,
            engine_skew_is_fine: str | None = None):
    for path in sorted(features.glob("train-*.jsonl.gz")):
        with gzip.open(path, "rt", encoding="utf-8") as f:
            for text in f:
                r = json.loads(text)
                training_rules.only_the_train_split(r, path.name, second_generation)
                training_rules.not_its_own_output(r, path.name, second_generation)
                training_rules.same_engine(r, path.name, engine_skew_is_fine)
                yield r


def page_rank(page_id: str) -> str:
    return hashlib.sha256(f"{SEED}:page:{page_id}".encode()).hexdigest()


def chosen_pages(features: pathlib.Path, per_document: int, second_generation,
                 engine_skew_is_fine: str | None = None) -> set[str]:
    pages = collections.defaultdict(list)
    for r in records(features, second_generation, engine_skew_is_fine):
        pages[r.get("document") or r["pdf_sha256"]].append(r["page_id"])
    return {p for ids in pages.values() for p in sorted(ids, key=page_rank)[:per_document]}


def line_labels(rows: pathlib.Path, pages: set[str]) -> dict:
    """The join's label per line of the chosen pages; a line with no region is
    left out, because its truth is unknown."""
    out = collections.defaultdict(dict)
    with rows.open(encoding="utf-8") as f:
        for text in f:
            if '"row": "line"' not in text:
                continue
            r = json.loads(text)
            if r["page_id"] not in pages or r.get("region") is None:
                continue
            out[r["page_id"]][(r["i"], r["line"])] = (r["label"], r["region"])
    return out


def self_test() -> int:
    """The vector is as wide as its names, and a neighbour off the page is NaN."""
    line = [float(i) for i in range(len(LINE_FEATURES))]
    whole = [0.5] * (len(BLOCK_FEATURES) + len(LOCAL_FEATURES))
    lines = [(0, 0, line, whole), (0, 1, line, whole), (1, 0, line, whole)]
    fails = []
    for at in range(3):
        if len(vector(lines, at)) != len(FEATURES):
            fails.append(f"line {at}: {len(vector(lines, at))} values, {len(FEATURES)} names")
    first = vector(lines, 0)
    up1 = FEATURES.index("up1_same_block")
    down1 = FEATURES.index("down1_same_block")
    down2 = FEATURES.index("down2_same_block")
    if not math.isnan(first[up1]):
        fails.append("the first line has a line above it")
    if first[down1] != 1.0 or first[down2] != 0.0:
        fails.append("same_block does not follow the blocks")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    print(f"heading-lines: {len(FEATURES)} features" if not fails else "", end="")
    return 1 if fails else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=pathlib.Path)
    ap.add_argument("--rows", type=pathlib.Path)
    ap.add_argument("--models", type=pathlib.Path, default=HERE / "models")
    ap.add_argument("--pages-per-document", type=int, default=40)
    ap.add_argument("--second-generation", default=None, metavar="REASON")
    ap.add_argument("--lopsided-is-fine", default=None, metavar="REASON")
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--heading", action="store_true",
                    help="train the BINARY heading model (`heading_lines`, the one "
                         "that decides) instead of the class model")
    ap.add_argument("--oof", type=pathlib.Path, default=None, metavar="OUT.jsonl.gz",
                    help="the class model only: also write every line's class "
                         "probabilities for every page of the train shards, OUT OF "
                         "FOLD on the train third, for the list model to stack on "
                         "(#574); give --models a scratch directory with it")
    ap.add_argument("--single-page-share", type=float, default=0.0,
                    help="the share of documents whose document facts are taken "
                         "from each page ALONE, as the engine sees a one-page PDF. "
                         "0.5 won on the one-page proxy and LOST on the rig "
                         "(c7c43fb38, reverted in 6dec3d61a), so it is off")
    ap.add_argument("--breaks", action="store_true",
                    help="train the BINARY break model (`line_breaks`): does a new "
                         "region begin at this line, inside its block -- the cut a "
                         "heading glued to its paragraph needs (#638)")
    ap.add_argument("--caption", action="store_true",
                    help="train the BINARY caption model (`caption_lines`): the "
                         "heading row plus where the line stands against the "
                         "page's pictures (#636)")
    args = ap.parse_args()
    global NAME, MAX_TREES, FEATURES
    binary = args.heading or args.caption or args.breaks
    if args.breaks:
        NAME, MAX_TREES = "line_breaks", 400
    if args.caption:
        NAME, MAX_TREES = "caption_lines", 400
        FEATURES = FEATURES + PICTURE_FEATURES
    if binary:
        if args.heading:
            NAME, MAX_TREES = "heading_lines", 400
        PARAMS.pop("num_class", None)
        PARAMS["objective"] = "binary"
    if args.self_test:
        return self_test()
    if not args.features or not args.rows:
        ap.error("--features and --rows, or --self-test")

    import lightgbm as lgb
    import numpy as np

    pages = chosen_pages(args.features, args.pages_per_document, args.second_generation,
                         args.engine_skew_is_fine)
    print(f"[{NAME}] {len(pages)} page(s), at most {args.pages_per_document} per document")
    labels = line_labels(args.rows, pages)
    chunks, ys, parts, docs = [], [], [], []

    def take(document: str, held: list[dict]) -> None:
        # THE DOCUMENT FACTS FROM EVERY PAGE OF IT, as the engine sees them;
        # the rows only from the chosen pages.
        #
        # OR FROM THE PAGE ALONE, for a share of the documents. The dump fills
        # `repeats_in_document` over a document's pages, but a one-page PDF --
        # every page the rig measures, and every page of the validation proxy
        # -- shows the engine one page, where a running head repeats nowhere.
        # Trained only the first way, the model learnt running heads from their
        # repetition and promoted them on the rig: 55 of the 85 false headings
        # on its financial reports (#574). Trained both ways, it has to find
        # the head on the page as well.
        alone = int(hashlib.sha256(f"{SEED}:alone:{document}".encode()).hexdigest()[:8], 16) \
            / 0xFFFFFFFF < args.single_page_share
        facts = None if alone else of_document(held)
        for at_page, r in enumerate(held):
            if r["page_id"] not in pages:
                continue
            said = labels.get(r["page_id"], {})
            if alone:
                r = dict(r, blocks=[dict(b, repeats_in_document=1 if (b.get("text") or "").strip() else 0)
                                    for b in r.get("blocks") or []])
                lines = page_lines(r, of_document([r]), 0)
            else:
                lines = page_lines(r, facts, at_page)
            # A CAPTION ROW NEEDS THE PAGE'S PICTURES: a shard not yet given
            # `images` (schema /9) would teach "no picture anywhere" as a fact.
            if args.caption and "images" not in r:
                continue
            boxes = page_boxes(r) if args.caption else []
            rows, keep = [], []
            for at, (block, k, _v, _w) in enumerate(lines):
                said_here = said.get((block, k))
                if said_here is None:
                    continue
                truth, region = said_here
                if args.breaks:
                    # ONLY INSIDE A BLOCK, from its second line: a break at a
                    # block's first line is the heuristics' own cut already.
                    above = said.get((block, k - 1)) if k > 0 else None
                    if above is None:
                        continue
                    rows.append(vector(lines, at))
                    keep.append(int(region != above[1]))
                    continue
                row = vector(lines, at)
                if args.caption:
                    box, size = boxes[at]
                    row += picture_features(box, size, r.get("images") or [])
                    keep.append(int(truth == "caption"))
                else:
                    cls = CLASS_OF.get(truth, 0)
                    keep.append(int(cls == 1) if binary else cls)
                rows.append(row)
            if not rows:
                continue
            chunks.append(np.asarray(rows, dtype=np.float32))
            ys.extend(keep)
            parts.extend([third(document)] * len(keep))
            docs.extend([document] * len(keep))

    # A DOCUMENT AT A TIME: the dump is in shard order and a document rarely
    # crosses a shard; one that does has its facts computed on each part, as
    # `train_blocks.py` does.
    current, held = None, []
    for r in records(args.features, args.second_generation,
                     args.engine_skew_is_fine):
        document = r.get("document") or r["pdf_sha256"]
        if document != current and held:
            take(current, held)
            held = []
        current = document
        held.append(r)
    if held:
        take(current, held)
    x = np.concatenate(chunks)
    y = np.asarray(ys, dtype=np.int32)
    part = np.asarray(parts)
    tr, va, te = part == "train", part == "validation", part == "test"
    va = training_rules.capped(docs, va, training_rules.PER_DOCUMENT_JUDGING)
    va = np.asarray(va)
    training_rules.refuse_a_lopsided_split("training", docs, tr, allow=bool(args.lopsided_is_fine))
    training_rules.refuse_a_lopsided_split("judging", docs, va, allow=bool(args.lopsided_is_fine))
    print(f"[{NAME}] rows: train {int(tr.sum())}, validation {int(va.sum())}, test {int(te.sum())}; "
          + (f"{int(y.sum())} positive line(s)" if binary else
           f"{int((y == 1).sum())} heading, {int((y == 2).sum())} list, {int((y == 3).sum())} table line(s)"))
    booster = lgb.train(PARAMS, lgb.Dataset(x[tr], label=y[tr], feature_name=FEATURES),
                        num_boost_round=MAX_TREES,
                        valid_sets=[lgb.Dataset(x[va], label=y[va])],
                        callbacks=[lgb.early_stopping(50, verbose=False)])
    trees = booster.best_iteration or MAX_TREES

    def f1(prob, truth, threshold):
        pred = prob >= threshold
        tp = int((pred & truth).sum()); fp = int((pred & ~truth).sum()); fn = int((~pred & truth).sum())
        p = tp / (tp + fp) if tp + fp else 0.0; r_ = tp / (tp + fn) if tp + fn else 0.0
        return {"precision": round(p, 4), "recall": round(r_, 4),
                "f1": round(2 * p * r_ / (p + r_), 4) if p + r_ else 0.0}

    # THE HEADING DECISION IS A THRESHOLD ON p[1], chosen on validation, as the
    # binary model's was: argmax would let three other classes vote it down.
    pv = booster.predict(x[va], num_iteration=trees, num_threads=1)
    pv = pv if binary else pv[:, 1]
    threshold = max(((f1(pv, y[va] == 1, t)["f1"], round(float(t), 2))
                     for t in np.arange(0.10, 0.91, 0.05)))[1]
    full = booster.predict(x[te], num_iteration=trees, num_threads=1)
    pt = full if binary else full[:, 1]
    per_class = {}
    for c, name in enumerate([] if binary else CLASSES):
        per_class[name] = f1((full.argmax(1) == c).astype(float), y[te] == c, 0.5)
    out = args.models / NAME
    out.mkdir(parents=True, exist_ok=True)
    dump = booster.dump_model(num_iteration=trees)
    dump["tree_info"] = dump["tree_info"][:trees * (1 if binary else len(CLASSES))]
    write_dump(dump, out / "model.json.gz")
    rng = np.random.default_rng(SEED)
    sample = rng.choice(len(x), size=min(300, len(x)), replace=False)
    sample.sort()
    margins = booster.predict(x[sample], num_iteration=trees, raw_score=True, num_threads=1)
    (out / "parity.json").write_text(json.dumps({
        "features": FEATURES,
        "rows": [[None if math.isnan(v) else float(v) for v in row] for row in x[sample].tolist()],
        "margins": [float(m) for m in margins] if binary else [[float(v) for v in m] for m in margins],
    }) + "\n")
    documents = sorted(set(docs))
    manifest = {
        "task": NAME,
        "rust": f"crates/pdf-engine/src/learned/models/{NAME}",
        "schema": "pdfluent.features/1",
        "features": FEATURES,
        "seed": SEED, "params": PARAMS, "trees": trees,
        "lightgbm": lgb.__version__,
        **({"threshold": threshold} if binary else {"classes": CLASSES, "heading_threshold": threshold}),
        "single_page_share": args.single_page_share,
        "pages_per_document": args.pages_per_document,
        "per_document_judging": training_rules.PER_DOCUMENT_JUDGING,
        "rows": {"train": int(tr.sum()), "validation": int(va.sum()), "test": int(te.sum())},
        "positives": {name: int((y == c).sum()) for c, name in enumerate(CLASSES)},
        "test": {"heading": f1(pt, y[te] == 1, threshold), "argmax": per_class},
        "documents": [{"source": "doclaynet-v1.2/train", "document": doc, "split": third(doc)}
                      for doc in documents],
    }
    skew = training_rules.engine_skew_manifest()
    if skew is not None:
        manifest["engine_skew"] = skew
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
    print(f"{NAME}: {trees} trees, threshold {threshold}, test {f1(pt, y[te] == 1, threshold)}"
          + (f", argmax {per_class}" if per_class else ""))
    if args.oof and not binary:
        out_of_fold(args, booster, trees, x, y, tr, docs, pages)
    return 0


def fold_of(document: str) -> int:
    return int(hashlib.sha256(f"{SEED}:fold:{document}".encode()).hexdigest()[:8], 16) % 3


def out_of_fold(args, booster, trees, x, y, tr, docs, pages) -> None:
    """Every line's class probabilities, as the list model may stack on them (#574).

    A LINE OF THE TRAIN THIRD gets them from a model that did not see its
    document -- three folds by document inside that third -- because that
    third is where `list_item` trains too, and a probability the class model
    fitted on those very lines would teach the list model to trust it more
    than it can be trusted on a page it has never seen. A line of the other
    two thirds gets them from the model this run just fitted, which is the
    shipped one (the check below says whether it is)."""
    import gzip
    import lightgbm as lgb
    import numpy as np
    shipped = json.loads((HERE / "models" / NAME / "parity.json").read_text())
    rows = np.asarray([[np.nan if v is None else v for v in r] for r in shipped["rows"]], dtype=np.float32)
    drift = float(np.abs(booster.predict(rows, num_iteration=trees, raw_score=True, num_threads=1)
                         - np.asarray(shipped["margins"])).max())
    print(f"[{NAME}] this fit against the shipped model's parity rows: max |margin| difference {drift:.2e}")
    if drift > 1e-6:
        raise SystemExit(f"[{NAME}] this run does not reproduce the shipped model; refusing to "
                         "write probabilities the engine would not compute")
    folds = np.asarray([fold_of(d) for d in docs])
    params = {k: v for k, v in PARAMS.items()}
    models = {}
    for k in range(3):
        keep = tr & (folds != k)
        models[k] = lgb.train(params, lgb.Dataset(x[keep], label=y[keep], feature_name=FEATURES),
                              num_boost_round=trees)
        print(f"[{NAME}] fold {k}: {int(keep.sum())} rows", flush=True)
    written = 0
    with gzip.open(args.oof, "wt", encoding="utf-8") as f:
        def emit(document: str, held: list[dict]) -> None:
            nonlocal written
            alone = int(hashlib.sha256(f"{SEED}:alone:{document}".encode()).hexdigest()[:8], 16) \
                / 0xFFFFFFFF < args.single_page_share
            facts = None if alone else of_document(held)
            model = models[fold_of(document)] if third(document) == "train" else booster
            for at_page, r in enumerate(held):
                if alone:
                    r = dict(r, blocks=[dict(b, repeats_in_document=1 if (b.get("text") or "").strip() else 0)
                                        for b in r.get("blocks") or []])
                    lines = page_lines(r, of_document([r]), 0)
                else:
                    lines = page_lines(r, facts, at_page)
                if not lines:
                    continue
                p = model.predict(np.asarray([vector(lines, at) for at in range(len(lines))], dtype=np.float32),
                                  num_iteration=trees, num_threads=4)
                f.write(json.dumps({"page_id": r["page_id"],
                                    "lines": [[b, k] for b, k, _v, _w in lines],
                                    "p": [[round(float(v), 5) for v in row] for row in p]}) + "\n")
                written += 1
        current, held = None, []
        for r in records(args.features, args.second_generation,
                         args.engine_skew_is_fine):
            document = r.get("document") or r["pdf_sha256"]
            if document != current and held:
                emit(current, held)
                held = []
            current = document
            held.append(r)
        if held:
            emit(current, held)
    print(f"[{NAME}] {written} page(s) of class probabilities to {args.oof}")


if __name__ == "__main__":
    sys.exit(main())
