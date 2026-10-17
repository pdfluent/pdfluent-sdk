#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables eval strat-300 set is drawn, and only drawn, from eval pages (#638).

`draw_pubtables_eval_strat.py` draws --count pages from the eval list's
documents whose pages are in the fetch manifest, stratified over the PMC id
in 250,000-id buckets, at most one page per document. Six cases:

- on a synthetic eval list and manifest the draw takes eval pages only, one
  per document, deterministic for the seed, with a header naming the draw,
  the seed, the inputs, the buckets and their counts;
- buckets with ample documents split the count equally, and a short bucket's
  shortfall goes to the others in id order;
- a missing eval list or manifest refuses the run;
- the committed `pubtables_eval_strat300.txt` holds 300 page names, one page
  per document, its buckets recomputed matching its header;
- the committed set is disjoint from every PubTables train list in the repo,
  and inside the eval list when it is checked in;
- a synthetic train list disjoint from the eval list stays disjoint from the
  draw.

And two mutations: the eval guard and the stratified draw are each taken out
of a copy of the draw, and a non-eval page is then drawn and the bucket
counts then gone -- which proves each is what put them there.
"""
from __future__ import annotations

import importlib.util
import json
import pathlib
import re
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
DRAW = LEARNED / "draw_pubtables_eval_strat.py"
STRAT = LEARNED / "pubtables_eval_strat300.txt"
EVAL = LEARNED / "pubtables_eval_documents.txt"
sys.path.insert(0, str(LEARNED))

EVAL_GUARD = 'if page["document"] not in eval_docs:'
STRAT_CALL = ("drawn_docs, report = splits.stratified_draw(sorted(chosen), "
              "args.count,\n"
              "                                                args.seed)")
PAGE_NAME = re.compile(r"PMC\d+-p\d+\.pdf$")


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


def bucket_of(pmcid: str) -> int:
    return int(pmcid[3:]) // 250_000


def document_of(name: str) -> str:
    return name[:-4].rsplit("-p", 1)[0]


def pages_for(doc: str, count: int) -> list[dict]:
    return [{"name": f"{doc}-p{n:04d}.pdf", "document": doc, "page_no": n}
            for n in range(count)]


def read_drawn(out: pathlib.Path) -> tuple[list[str], list[str]]:
    names, comments = [], []
    for raw in out.read_text(encoding="utf-8").splitlines():
        if raw.startswith("#"):
            comments.append(raw)
        elif raw.strip():
            names.append(raw.strip())
    return names, comments


def main() -> int:
    import draw_pubtables_eval_strat as real
    source = DRAW.read_text(encoding="utf-8")
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        # Three buckets: 9, 8 and 9 eval documents with 2, 1 and 3 pages,
        # two non-eval documents with pages, and a disjoint train list.
        eval_docs = ([f"PMC{100001 + n}" for n in range(9)]
                     + [f"PMC{300001 + n}" for n in range(8)]
                     + [f"PMC{800001 + n}" for n in range(9)])
        eval_list = tmp / "eval.txt"
        eval_list.write_text("# fixture\n" + "".join(
            d + "\n" for d in eval_docs), encoding="utf-8")
        train_list = tmp / "train.txt"
        train_list.write_text("PMC900001\nPMC900002\n", encoding="utf-8")
        manifest_pages = []
        for n, doc in enumerate(eval_docs):
            manifest_pages.extend(pages_for(doc, 2 if n % 3 == 0 else
                                            (1 if n % 3 == 1 else 3)))
        manifest_pages.extend(pages_for("PMC400001", 2))
        manifest_pages.extend(pages_for("PMC400002", 1))
        manifest = tmp / "MANIFEST.json"
        manifest.write_text(json.dumps({"pages": manifest_pages}),
                            encoding="utf-8")

        def run(module, where, count, seed=638):
            return module.main(["--eval-list", str(eval_list),
                                "--manifest", str(manifest),
                                "--count", str(count), "--seed", str(seed),
                                "--out", str(where)])

        out = tmp / "strat.txt"
        if run(real, out, 12) != 0:
            fails.append("the draw did not exit 0")
        names, comments = read_drawn(out)
        header = "\n".join(comments)
        again = tmp / "strat2.txt"
        run(real, again, 12)
        if out.read_bytes() != again.read_bytes():
            fails.append("the same seed drew different bytes")
        if "--seed 638" not in header or "Buckets" not in header:
            fails.append("the header does not name the seed and the buckets")
        if str(eval_list) not in header or str(manifest) not in header:
            fails.append("the header does not name its inputs")
        if len(names) != 12 or len({document_of(n) for n in names}) != 12:
            fails.append("the draw is not one page per document")
        if any(document_of(n) in ("PMC400001", "PMC400002") for n in names):
            fails.append("a non-eval page was drawn")
        if any(not PAGE_NAME.fullmatch(n) for n in names):
            fails.append("a drawn name is not a page name")
        shares = {}
        for name in names:
            bucket = bucket_of(document_of(name))
            shares[bucket] = shares.get(bucket, 0) + 1
        if shares != {0: 4, 1: 4, 3: 4}:
            fails.append(f"the shares are not equal: {shares}")
        if {document_of(n) for n in names} & {"PMC900001", "PMC900002"}:
            fails.append("the draw is not disjoint from the train list")

        # The shortfall: bucket 0 holds 1 document for a share of 4.
        thin_eval = [d for d in eval_docs if not d.startswith("PMC10000")
                     or d == "PMC100001"]
        thin_list = tmp / "thin_eval.txt"
        thin_list.write_text("".join(d + "\n" for d in thin_eval),
                             encoding="utf-8")
        thin_out = tmp / "thin.txt"
        real.main(["--eval-list", str(thin_list),
                   "--manifest", str(manifest),
                   "--count", "12", "--seed", "638",
                   "--out", str(thin_out)])
        thin_names, _ = read_drawn(thin_out)
        shares = {}
        for name in thin_names:
            bucket = bucket_of(document_of(name))
            shares[bucket] = shares.get(bucket, 0) + 1
        if shares != {0: 1, 1: 6, 3: 5}:
            fails.append(f"the shortfall did not go in id order: {shares}")

        # Fail closed.
        try:
            real.main(["--eval-list", str(tmp / "no-eval.txt"),
                       "--manifest", str(manifest),
                       "--count", "2", "--out", str(tmp / "x.txt")])
            fails.append("a missing eval list did not refuse the run")
        except SystemExit:
            pass
        try:
            real.main(["--eval-list", str(eval_list),
                       "--manifest", str(tmp / "no-manifest.json"),
                       "--count", "2", "--out", str(tmp / "x.txt")])
            fails.append("a missing manifest did not refuse the run")
        except SystemExit:
            pass

        for guard, name in ((EVAL_GUARD, "the eval guard"),
                            (STRAT_CALL, "the stratified draw")):
            if guard not in source:
                fails.append(f"{name} is not in draw_pubtables_eval_strat.py; "
                             "the mutation cannot be made")
        if not fails:
            mutant = load(source.replace(EVAL_GUARD, "if False:"),
                          "strat_mutant_eval")
            where = tmp / "mut_eval.txt"
            run(mutant, where, 30)
            got, _ = read_drawn(where)
            if not any(document_of(n) in ("PMC400001", "PMC400002")
                       for n in got):
                fails.append("with the eval guard taken out, no non-eval "
                             "page is drawn -- something else guards them, "
                             "and this test is not testing the guard")
            mutant = load(source.replace(
                STRAT_CALL,
                "drawn_docs, report = (sorted(random.Random(args.seed)"
                ".sample(sorted(chosen), min(args.count, len(chosen)))), {})"),
                "strat_mutant_strat")
            where = tmp / "mut_strat.txt"
            run(mutant, where, 12)
            _, mut_comments = read_drawn(where)
            if any(c.startswith("#   PMC ") for c in mut_comments):
                fails.append("with the stratified draw taken out, the "
                             "bucket counts are still written -- something "
                             "else writes them, and this test is not "
                             "testing the draw")

    # The committed set.
    if not STRAT.is_file():
        fails.append("pubtables_eval_strat300.txt is not committed")
        names, comments = [], []
    else:
        names, comments = read_drawn(STRAT)
    header = "\n".join(comments)
    if len(names) != 300 or len(set(names)) != 300:
        fails.append(f"the committed set holds {len(set(names))} page names, "
                     "want 300")
    if len({document_of(n) for n in names}) != len(names):
        fails.append("the committed set names two pages of one document")
    if any(not PAGE_NAME.fullmatch(n) for n in names):
        fails.append("the committed set names something that is not a page")
    if "--seed 638" not in header or "FOR MEASURING ONLY" not in header:
        fails.append("the committed header does not name the draw")
    buckets = {}
    for comment in comments:
        found = re.fullmatch(r"#   PMC (\d+)-(\d+): (\d+)/(\d+)", comment)
        if found:
            lo, hi, drawn, cands = (int(g) for g in found.groups())
            buckets[lo // 250_000] = (drawn, cands, lo, hi)
    if sum(d for d, _c, _lo, _hi in buckets.values()) != len(names):
        fails.append("the committed header's buckets do not add up to the "
                     "set")
    for bucket, (drawn, cands, lo, hi) in buckets.items():
        if (lo, hi) != (bucket * 250_000, (bucket + 1) * 250_000 - 1):
            fails.append(f"bucket {bucket} covers the wrong range")
            continue
        if cands < drawn:
            fails.append(f"bucket {bucket} drew more than its candidates")
    recomputed = {}
    for name in names:
        bucket = bucket_of(document_of(name))
        recomputed[bucket] = recomputed.get(bucket, 0) + 1
    for bucket, (drawn, _c, _lo, _hi) in buckets.items():
        if recomputed.get(bucket, 0) != drawn:
            fails.append(f"bucket {bucket}'s header count is not the set's")
    strat_docs = {document_of(n) for n in names}
    for train in sorted(LEARNED.glob("pubtables_*train*.txt")):
        train_docs = {line.strip() for line in
                      train.read_text(encoding="utf-8").splitlines()
                      if line.strip() and not line.startswith("#")}
        if strat_docs & train_docs:
            fails.append(f"the committed set meets the train list "
                         f"{train.name}: "
                         f"{sorted(strat_docs & train_docs)[:4]}")
    if EVAL.is_file():
        eval_docs = {line.strip() for line in
                     EVAL.read_text(encoding="utf-8").splitlines()
                     if line.strip() and not line.startswith("#")}
        if not strat_docs <= eval_docs:
            fails.append("the committed set names a document outside the "
                         "eval list")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("strat-300: eval pages only, one per document, stratified over "
          "the PMC id, disjoint from every train list.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
