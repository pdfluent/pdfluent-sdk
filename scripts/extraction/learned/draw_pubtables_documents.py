#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""An era-balanced PubTables train sample, stratified over the PMC id (#638).

The PubTables train and eval document lists split by TIME: the train list sits
at PMC 0.49M-6.45M, mostly 2015-2019, the eval list at 1.06M-4.55M, 2005-2015.
Every model trained on the train list has learned newer layouts and is
measured on older ones -- is-table v2 refuses a 2010 crystallography table
family of which the train list holds 0 documents. The fetch script takes the
first N documents of the annotation tarball in file order, which is how one
era ended up on each side.

So this draws --count documents STRATIFIED over the PMC id, in buckets of
250,000 ids across the full candidate range, an equal share per bucket. The
new list ADDS to pubtables_train_documents.txt: every document of the eval
list, of the strat-300 set (by page's document), of the rig, and of both
train lists is refused by name, and the EVAL list stays frozen.

--from-id N and --to-id M draw only PMC ids N-M inclusive, stratified over
the buckets inside the range, with the range named in the header. Without
them the draw covers the whole range, as before.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import pubtables_cells  # noqa: E402
import pubtables_splits as splits  # noqa: E402

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
TARBALL = STORE / "PubTables-1M-PDF_Annotations.tar.gz"
EVAL_LIST = HERE.parent / "pubtables_eval_documents.txt"
TRAIN_LIST = HERE.parent / "pubtables_train_documents.txt"
STRAT300 = HERE.parent / "pubtables_eval_strat300.txt"
ERA_LIST = HERE.parent / "pubtables_train_documents_era.txt"
RIG = HERE.parents[3] / "corpus" / "structured" / "PUBTABLES1M.json"
OUT = HERE.parent / "pubtables_train_documents_era.txt"


def rig_documents(path: pathlib.Path) -> set[str]:
    """Every document the rig holds any page of: training on one is training
    on its own exam, by document and not by page."""
    splits.require_file(path, "the rig manifest")
    listed = json.loads(path.read_text(encoding="utf-8"))
    return {page["document"] for page in listed.get("pages", [])
            if page.get("document")}


def strat_documents(path: pathlib.Path) -> set[str]:
    """Every document the strat-300 set holds a page of: the set names pages,
    and training on another page of a measure document is still training on
    the measure, by document and not by page."""
    return {splits.document_of(page) for page in splits.read_id_set(path)}


def in_range(number: int, from_id: int | None, to_id: int | None) -> bool:
    """Whether a PMC number sits inside the inclusive id range: a side with
    no bound holds the whole range on that side."""
    if from_id is not None and number < from_id:
        return False
    if to_id is not None and number > to_id:
        return False
    return True


def candidates_of(tarball: pathlib.Path, refused_eval: set[str],
                  refused_rig: set[str], refused_train: set[str],
                  refused_strat: set[str] | None = None,
                  refused_era: set[str] | None = None,
                  from_id: int | None = None, to_id: int | None = None,
                  ) -> tuple[list[str], dict[str, int]]:
    """The tarball's drawable documents, in file order, and the refusals.

    A candidate has at least one table in the TRAINING split -- the check
    `fetch_training_pages.py` makes -- and a table above `pubtables_cells`'
    floor, and sits inside the inclusive id range. The name refusals run
    first, then the range, then the split and the floor. Everything else is
    refused, and counted by reason.
    """
    splits.require_file(tarball, "the annotation tarball")
    refused_strat = refused_strat or set()
    refused_era = refused_era or set()
    candidates, refused = [], {}
    with tarfile.open(tarball, "r|gz") as tar:
        for member in tar:
            if not member.name.endswith("_tables.json"):
                continue
            pmcid = member.name.split("_")[0]
            number = splits.pmc_number(pmcid)
            if number is None:
                refused["not a PMC document"] = refused.get(
                    "not a PMC document", 0) + 1
                continue
            if pmcid in refused_eval:
                refused["the eval list"] = refused.get("the eval list", 0) + 1
                continue
            if pmcid in refused_strat:
                refused["the strat-300 set"] = refused.get(
                    "the strat-300 set", 0) + 1
                continue
            if pmcid in refused_rig:
                refused["the rig"] = refused.get("the rig", 0) + 1
                continue
            if pmcid in refused_train:
                refused["the existing train list"] = refused.get(
                    "the existing train list", 0) + 1
                continue
            if pmcid in refused_era:
                refused["the era train list"] = refused.get(
                    "the era train list", 0) + 1
                continue
            if not in_range(number, from_id, to_id):
                refused["outside the id range"] = refused.get(
                    "outside the id range", 0) + 1
                continue
            handle = tar.extractfile(member)
            if handle is None:
                continue
            try:
                annotation = json.load(handle)
            except json.JSONDecodeError:
                refused["unreadable annotation"] = refused.get(
                    "unreadable annotation", 0) + 1
                continue
            if not any(t.get("split") == "train" for t in annotation):
                refused["not the training split"] = refused.get(
                    "not the training split", 0) + 1
                continue
            if not pubtables_cells.tables_by_page(annotation):
                refused["no table above the floor"] = refused.get(
                    "no table above the floor", 0) + 1
                continue
            candidates.append(pmcid)
    return candidates, refused


def header(count: int, seed: int, candidates: int,
           report: dict[int, tuple[int, int]], refused: dict[str, int],
           from_id: int | None = None, to_id: int | None = None) -> str:
    """The draw, named: the seed, the range, the buckets and their counts."""
    if from_id is None and to_id is None:
        across = "across the full candidate range"
    elif from_id is not None and to_id is not None:
        across = f"across PMC {from_id}-{to_id}"
    elif from_id is not None:
        across = f"across PMC {from_id} and above"
    else:
        across = f"across PMC up to {to_id}"
    lines = [
        f"# PubTables era-balanced train sample: {count} PMC ids, one per "
        "line, in id order.",
        f"# Drawn --seed {seed} over the PMC id in "
        f"{splits.BUCKET:,}-id buckets {across} "
        f"({candidates} candidates): an equal share per bucket, a bucket "
        "with too few giving what it has and the shortfall going to the "
        "others in id order.",
        "# Refused by name: every document of pubtables_eval_documents.txt, "
        "of the rig (corpus/structured/PUBTABLES1M.json) and of "
        "pubtables_train_documents.txt, which this list adds to -- no eval "
        "document trains (#638).",
        "# Refusals: " + (", ".join(
            f"{n} {why}" for why, n in sorted(refused.items()))
            if refused else "none") + ".",
        "# Buckets (PMC range: drawn/candidates):",
    ]
    for bucket in sorted(report):
        lo, hi = bucket * splits.BUCKET, (bucket + 1) * splits.BUCKET - 1
        drawn, cands = report[bucket]
        lines.append(f"#   PMC {lo}-{hi}: {drawn}/{cands}")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--annotations", default=str(TARBALL))
    parser.add_argument("--eval-list", default=str(EVAL_LIST))
    parser.add_argument("--train-list", default=str(TRAIN_LIST))
    parser.add_argument("--strat-list", default=str(STRAT300))
    parser.add_argument("--era-list", default=str(ERA_LIST))
    parser.add_argument("--rig", default=str(RIG))
    parser.add_argument("--count", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=638)
    parser.add_argument("--from-id", type=int, default=None)
    parser.add_argument("--to-id", type=int, default=None)
    parser.add_argument("--out", default=str(OUT))
    args = parser.parse_args(argv)

    if (args.from_id is not None and args.to_id is not None
            and args.from_id > args.to_id):
        raise SystemExit(
            f"--from-id {args.from_id} is above --to-id {args.to_id}: the "
            "range holds no id, so the draw would be empty.")

    refused_eval = splits.read_id_set(pathlib.Path(args.eval_list))
    refused_train = splits.read_id_set(pathlib.Path(args.train_list))
    refused_era = splits.read_id_set(pathlib.Path(args.era_list))
    refused_strat = strat_documents(pathlib.Path(args.strat_list))
    refused_rig = rig_documents(pathlib.Path(args.rig))
    print(f"refusing {len(refused_eval)} eval, {len(refused_strat)} "
          f"strat-300, {len(refused_rig)} rig, {len(refused_train)} train "
          f"and {len(refused_era)} era documents by name",
          flush=True)
    found, refused = candidates_of(pathlib.Path(args.annotations),
                                   refused_eval, refused_rig, refused_train,
                                   refused_strat, refused_era,
                                   args.from_id, args.to_id)
    drawn, report = splits.stratified_draw(found, args.count, args.seed)
    if len(drawn) < args.count:
        print(f"only {len(drawn)} candidates for --count {args.count}: "
              "the list holds what there is", file=sys.stderr)
    out = pathlib.Path(args.out)
    out.write_text(header(len(drawn), args.seed, len(found), report, refused,
                          args.from_id, args.to_id)
                   + "".join(pmcid + "\n" for pmcid in drawn),
                   encoding="utf-8")
    print(f"drew {len(drawn)} of {len(found)} candidates over "
          f"{len(report)} buckets: {out}", flush=True)
    print("refused:", refused, flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
