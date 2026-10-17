#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The era-balanced PubTables train draw is stratified, closed and deterministic (#638).

`draw_pubtables_documents.py` draws --count documents over the PMC id in
250,000-id buckets, an equal share per bucket, refusing by name every document
of the eval list, of the strat-300 set (by page's document), of the rig and
of both train lists. --from-id N and --to-id M (inclusive, default the whole
range) draw only candidates inside the range, stratified over the buckets
inside it, with the range named in the header. Nine cases on a synthetic
annotation tarball:

- the same seed draws the same bytes twice, in id order, with a header naming
  the draw, the seed, the buckets and their counts;
- buckets with ample candidates split the count equally;
- a bucket with too few candidates gives what it has and the shortfall goes
  to the others in id order;
- no eval, strat-300, rig or train document is ever drawn, whatever split
  and floor it carries;
- a document without a train table, and one with no table above the cell
  floor, is refused and counted;
- a missing refusal list, rig manifest or tarball refuses the run -- a draw
  without its refusals would put eval documents in a train list;
- --from-id 2750000 --to-id 3249999 on ids across 0-6M draws only ids in
  range, stratified over the two buckets, with the range in the header;
- inside the range, an eval, a strat-300, a rig and a train document of each
  list are never drawn, even when they are the only candidates left in a
  bucket;
- without the flags the draw is byte-identical to before for the same seed:
  the full candidate range, every bucket, the same ids as the unfiltered
  stratified draw.

And six mutations: each refusal and the range guard is taken out of a copy
of the draw, and the refused or out-of-range document then has to be drawn --
which proves the guard is what stopped it, and not something else in the file.
"""
from __future__ import annotations

import importlib.util
import io
import json
import pathlib
import sys
import tarfile
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
DRAW = LEARNED / "draw_pubtables_documents.py"
sys.path.insert(0, str(LEARNED))

EVAL_GUARD = "if pmcid in refused_eval:"
STRAT_GUARD = "if pmcid in refused_strat:"
ERA_GUARD = "if pmcid in refused_era:"
RANGE_GUARD = "if not in_range(number, from_id, to_id):"
SPLIT_GUARD = 'if not any(t.get("split") == "train" for t in annotation):'
FLOOR_GUARD = "if not pubtables_cells.tables_by_page(annotation):"


def table(split="train", cells=4, page=0):
    return {"split": split, "pdf_page_index": page,
            "pdf_table_bbox": [0, 0, 10, 10],
            "cells": [{"row_nums": [r], "column_nums": [c],
                       "pdf_text_content": f"{r},{c}"}
                      for r in range(2) for c in range(cells // 2)]}


def write_tarball(path: pathlib.Path, members: dict[str, list[dict]]) -> None:
    with tarfile.open(path, "w:gz") as tar:
        for name, annotation in members.items():
            data = json.dumps(annotation).encode("utf-8")
            info = tarfile.TarInfo(f"{name}_tables.json")
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))


def write_list(path: pathlib.Path, ids: list[str]) -> None:
    path.write_text("# a fixture list\n" + "".join(i + "\n" for i in ids),
                    encoding="utf-8")


def write_rig(path: pathlib.Path, documents: list[str]) -> None:
    path.write_text(json.dumps({"pages": [
        {"document": d, "name": f"{d}-p0001.pdf"} for d in documents]}),
        encoding="utf-8")


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


def run(module, tarball, eval_list, train_list, rig, out, count, seed=638,
        strat_list=None, era_list=None, from_id=None, to_id=None):
    args = ["--annotations", str(tarball),
            "--eval-list", str(eval_list),
            "--train-list", str(train_list),
            "--rig", str(rig),
            "--count", str(count), "--seed", str(seed),
            "--out", str(out)]
    if strat_list is not None:
        args += ["--strat-list", str(strat_list)]
    if era_list is not None:
        args += ["--era-list", str(era_list)]
    if from_id is not None:
        args += ["--from-id", str(from_id)]
    if to_id is not None:
        args += ["--to-id", str(to_id)]
    return module.main(args)


def read_drawn(out: pathlib.Path) -> tuple[list[str], list[str]]:
    ids, comments = [], []
    for raw in out.read_text(encoding="utf-8").splitlines():
        if raw.startswith("#"):
            comments.append(raw)
        elif raw.strip():
            ids.append(raw.strip())
    return ids, comments


def bucket(pmcid: str) -> int:
    return int(pmcid[3:]) // 250_000


def main() -> int:
    import draw_pubtables_documents as real
    import pubtables_splits as splits
    source = DRAW.read_text(encoding="utf-8")
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        # Three buckets, ten candidates each, plus one val-only and one
        # below-floor document that must never draw.
        members = {}
        for n in range(10):
            members[f"PMC{100001 + n}"] = [table()]
            members[f"PMC{300001 + n}"] = [table()]
            members[f"PMC{800001 + n}"] = [table()]
        members["PMC100050"] = [table(split="val")]
        members["PMC300050"] = [table(cells=2)]
        tarball = tmp / "ann.tar.gz"
        write_tarball(tarball, members)
        eval_list = tmp / "eval.txt"
        train_list = tmp / "train.txt"
        rig = tmp / "rig.json"
        write_list(eval_list, ["PMC100001", "PMC800009"])
        write_list(train_list, ["PMC300001"])
        write_rig(rig, ["PMC300002"])

        out = tmp / "era.txt"
        if run(real, tarball, eval_list, train_list, rig, out, 12) != 0:
            fails.append("the draw did not exit 0")
        ids, comments = read_drawn(out)
        header = "\n".join(comments)
        # Deterministic: the same seed draws the same bytes twice.
        again = tmp / "era2.txt"
        run(real, tarball, eval_list, train_list, rig, again, 12)
        if out.read_bytes() != again.read_bytes():
            fails.append("the same seed drew different bytes")
        if "--seed 638" not in header or "Buckets" not in header:
            fails.append("the header does not name the seed and the buckets")
        if "(26 candidates)" not in header:
            fails.append(f"the header does not count the candidates: {header!r}")
        numbers = [int(i[3:]) for i in ids]
        if numbers != sorted(numbers):
            fails.append("the drawn ids are not in id order")
        for banned in ("PMC100001", "PMC800009", "PMC300001", "PMC300002",
                       "PMC100050", "PMC300050"):
            if banned in ids:
                fails.append(f"a refused document was drawn: {banned}")
        # Equal shares: 12 over 3 buckets with ample candidates is 4/4/4.
        shares = {}
        for pmcid in ids:
            shares[bucket(pmcid)] = shares.get(bucket(pmcid), 0) + 1
        if shares != {0: 4, 1: 4, 3: 4}:
            fails.append(f"the shares are not equal: {shares}")
        for want in ("#   PMC 0-249999: 4/9", "#   PMC 250000-499999: 4/8",
                     "#   PMC 750000-999999: 4/9"):
            if want not in comments:
                fails.append(f"the header misses the bucket count: {want}")

        # The shortfall: bucket 0 holds 1 candidate for a share of 4, so
        # the 3 missing go round-robin in id order: bucket 1, bucket 3,
        # bucket 1 again.
        thin = dict(members)
        for n in range(2, 10):
            del thin[f"PMC{100001 + n}"]
        thin_tarball = tmp / "thin.tar.gz"
        write_tarball(thin_tarball, thin)
        thin_out = tmp / "thin.txt"
        run(real, thin_tarball, eval_list, train_list, rig, thin_out, 12)
        thin_ids, _ = read_drawn(thin_out)
        shares = {}
        for pmcid in thin_ids:
            shares[bucket(pmcid)] = shares.get(bucket(pmcid), 0) + 1
        if shares != {0: 1, 1: 6, 3: 5}:
            fails.append(f"the shortfall did not go in id order: {shares}")

        # The refusal counts, by reason.
        _, refused = real.candidates_of(
            tarball, {"PMC100001", "PMC800009"}, {"PMC300002"}, {"PMC300001"})
        want_refused = {"the eval list": 2, "the rig": 1,
                        "the existing train list": 1,
                        "not the training split": 1,
                        "no table above the floor": 1}
        if refused != want_refused:
            fails.append(f"the refusals are not counted by reason: {refused}")

        # The range: ids across 0-6M, drawn with --from-id 2750000
        # --to-id 3249999, only ids in range, stratified over the two buckets.
        wide = {}
        for n in range(10):
            wide[f"PMC{100001 + n}"] = [table()]
            wide[f"PMC{2800001 + n}"] = [table()]
            wide[f"PMC{3100001 + n}"] = [table()]
            wide[f"PMC{5000001 + n}"] = [table()]
            wide[f"PMC{6000001 + n}"] = [table()]
        wide_tarball = tmp / "wide.tar.gz"
        write_tarball(wide_tarball, wide)
        empty_eval = tmp / "empty_eval.txt"
        empty_train = tmp / "empty_train.txt"
        empty_era = tmp / "empty_era.txt"
        empty_strat = tmp / "empty_strat.txt"
        empty_rig = tmp / "empty_rig.json"
        write_list(empty_eval, [])
        write_list(empty_train, [])
        write_list(empty_era, [])
        write_list(empty_strat, [])
        write_rig(empty_rig, [])
        try:
            ranged_out = tmp / "ranged.txt"
            code = run(real, wide_tarball, empty_eval, empty_train,
                       empty_rig, ranged_out, 12,
                       strat_list=empty_strat, era_list=empty_era,
                       from_id=2750000, to_id=3249999)
            if code != 0:
                fails.append("the ranged draw did not exit 0")
            else:
                got, comments = read_drawn(ranged_out)
                header = "\n".join(comments)
                for pmcid in got:
                    number = int(pmcid[3:])
                    if number < 2750000 or number > 3249999:
                        fails.append(
                            f"an out-of-range document was drawn: {pmcid}")
                        break
                shares = {}
                for pmcid in got:
                    shares[bucket(pmcid)] = shares.get(bucket(pmcid), 0) + 1
                if shares != {11: 6, 12: 6}:
                    fails.append(
                        f"the ranged shares are not equal over two buckets: "
                        f"{shares}")
                if "2750000" not in header or "3249999" not in header:
                    fails.append(
                        "the header does not name the range: "
                        f"{header!r}")
                for want in ("#   PMC 2750000-2999999: 6/10",
                             "#   PMC 3000000-3249999: 6/10"):
                    if want not in comments:
                        fails.append(
                            f"the header misses the ranged bucket count: "
                            f"{want}")
                for outside in ("PMC 0-249999", "PMC 5000000-5249999",
                                "PMC 6000000-6249999"):
                    if any(outside in c for c in comments):
                        fails.append(
                            f"a bucket outside the range is reported: "
                            f"{outside}")
        except SystemExit as e:
            fails.append(f"the range flags are not accepted: {e}")
        except TypeError as e:
            fails.append(f"the range draw does not run: {e}")

        # The refusals inside the range, even when they are the only
        # candidates left in a bucket: bucket 11 holds only the five refused
        # documents, bucket 12 the ten normals.
        refused_members = {}
        for n in range(10):
            refused_members[f"PMC{3100001 + n}"] = [table()]
        for doc in ("PMC2800001", "PMC2800002", "PMC2800003", "PMC2800004",
                    "PMC2800005"):
            refused_members[doc] = [table()]
        refused_tarball = tmp / "refused.tar.gz"
        write_tarball(refused_tarball, refused_members)
        range_eval = tmp / "range_eval.txt"
        range_strat = tmp / "range_strat.txt"
        range_rig = tmp / "range_rig.json"
        range_train = tmp / "range_train.txt"
        range_era = tmp / "range_era.txt"
        write_list(range_eval, ["PMC2800001"])
        write_list(range_strat, ["PMC2800002-p0001.pdf"])
        write_rig(range_rig, ["PMC2800003"])
        write_list(range_train, ["PMC2800004"])
        write_list(range_era, ["PMC2800005"])
        try:
            refused_out = tmp / "refused_out.txt"
            code = run(real, refused_tarball, range_eval, range_train,
                       range_rig, refused_out, 8,
                       strat_list=range_strat, era_list=range_era,
                       from_id=2750000, to_id=3249999)
            if code != 0:
                fails.append("the refusal-in-range draw did not exit 0")
            else:
                got, comments = read_drawn(refused_out)
                for banned in ("PMC2800001", "PMC2800002", "PMC2800003",
                               "PMC2800004", "PMC2800005"):
                    if banned in got:
                        fails.append(
                            f"a refused document was drawn in range: {banned}")
                shares = {}
                for pmcid in got:
                    shares[bucket(pmcid)] = shares.get(bucket(pmcid), 0) + 1
                if shares != {12: 8}:
                    fails.append(
                        "the bucket of only refused documents was drawn "
                        f"from: {shares}")
                if any("PMC 2750000-2999999" in c for c in comments):
                    fails.append(
                        "the bucket of only refused documents is reported")
        except SystemExit as e:
            fails.append(f"the refusal-in-range draw did not run: {e}")
        except TypeError as e:
            fails.append(f"the refusal-in-range draw did not run: {e}")

        # Without the flags: byte-identical to before, the full candidate
        # range, every bucket, the same ids as the unfiltered stratified draw.
        try:
            plain_out = tmp / "plain.txt"
            code = run(real, wide_tarball, empty_eval, empty_train,
                       empty_rig, plain_out, 10,
                       strat_list=empty_strat, era_list=empty_era)
            if code != 0:
                fails.append("the unflagged draw did not exit 0")
            else:
                got, comments = read_drawn(plain_out)
                header = "\n".join(comments)
                if "across the full candidate range" not in header:
                    fails.append(
                        "without the flags the header does not say the full "
                        f"candidate range: {header!r}")
                for want in ("#   PMC 0-249999: 2/10",
                             "#   PMC 2750000-2999999: 2/10",
                             "#   PMC 3000000-3249999: 2/10",
                             "#   PMC 5000000-5249999: 2/10",
                             "#   PMC 6000000-6249999: 2/10"):
                    if want not in comments:
                        fails.append(
                            "without the flags the header misses a bucket: "
                            f"{want}")
                want_ids, _ = splits.stratified_draw(sorted(wide), 10, 638)
                if got != want_ids:
                    fails.append(
                        "without the flags the draw is not the unfiltered "
                        f"stratified draw: {got} != {want_ids}")
        except SystemExit as e:
            fails.append(f"the unflagged draw did not run: {e}")
        except TypeError:
            # The old draw takes no --strat-list/--era-list: run it the old
            # way, which is the before the new draw must match.
            plain_out = tmp / "plain.txt"
            run(real, wide_tarball, empty_eval, empty_train,
                empty_rig, plain_out, 10)
            got, comments = read_drawn(plain_out)
            header = "\n".join(comments)
            if "across the full candidate range" not in header:
                fails.append(
                    "without the flags the header does not say the full "
                    "candidate range")

        # Fail closed: no draw without its refusals.
        for missing, name in ((tmp / "no-eval.txt", "eval list"),
                              (tmp / "no-train.txt", "train list"),
                              (tmp / "no-rig.json", "rig manifest"),
                              (tmp / "no-ann.tar.gz", "tarball")):
            args = {"eval_list": eval_list, "train_list": train_list,
                    "rig": rig, "tarball": tarball}
            if "eval" in name:
                args["eval_list"] = missing
            elif "train" in name:
                args["train_list"] = missing
            elif "rig" in name:
                args["rig"] = missing
            else:
                args["tarball"] = missing
            try:
                run(real, args["tarball"], args["eval_list"],
                    args["train_list"], args["rig"], tmp / "x.txt", 2)
                fails.append(f"a missing {name} did not refuse the run")
            except SystemExit:
                pass
        for missing, name in ((tmp / "no-strat.txt", "strat-300 list"),
                              (tmp / "no-era.txt", "era train list")):
            strat_file = missing if "strat" in name else range_strat
            era_file = missing if "era" in name else range_era
            try:
                run(real, refused_tarball, range_eval, range_train,
                    range_rig, tmp / "x2.txt", 2,
                    strat_list=strat_file, era_list=era_file,
                    from_id=2750000, to_id=3249999)
                fails.append(f"a missing {name} did not refuse the run")
            except SystemExit:
                pass
            except TypeError as e:
                fails.append(f"the {name} refusal is not wired: {e}")

        for guard, name in ((EVAL_GUARD, "the eval-name refusal"),
                            (STRAT_GUARD, "the strat-300 refusal"),
                            (ERA_GUARD, "the era-train refusal"),
                            (RANGE_GUARD, "the id-range guard"),
                            (SPLIT_GUARD, "the training-split requirement"),
                            (FLOOR_GUARD, "the cell-floor requirement")):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in "
                             "draw_pubtables_documents.py; the mutation "
                             "cannot be made")
        if not fails:
            mutant = load(source.replace(EVAL_GUARD, "if False:"),
                          "draw_mutant_eval")
            where = tmp / "mut_eval.txt"
            run(mutant, tarball, eval_list, train_list, rig, where, 30)
            got, _ = read_drawn(where)
            if "PMC100001" not in got and "PMC800009" not in got:
                fails.append("with the eval refusal taken out, no eval "
                             "document is drawn -- something else refuses "
                             "them, and this test is not testing the refusal")
            mutant = load(source.replace(SPLIT_GUARD, "if False:"),
                          "draw_mutant_split")
            where = tmp / "mut_split.txt"
            run(mutant, tarball, eval_list, train_list, rig, where, 30)
            got, _ = read_drawn(where)
            if "PMC100050" not in got:
                fails.append("with the split requirement taken out, the "
                             "val-only document is still refused -- "
                             "something else refuses it, and this test is "
                             "not testing the requirement")
            mutant = load(source.replace(FLOOR_GUARD, "if False:"),
                          "draw_mutant_floor")
            where = tmp / "mut_floor.txt"
            run(mutant, tarball, eval_list, train_list, rig, where, 30)
            got, _ = read_drawn(where)
            if "PMC300050" not in got:
                fails.append("with the floor requirement taken out, the "
                             "below-floor document is still refused -- "
                             "something else refuses it, and this test is "
                             "not testing the requirement")
            mutant = load(source.replace(STRAT_GUARD, "if False:"),
                          "draw_mutant_strat")
            where = tmp / "mut_strat.txt"
            run(mutant, refused_tarball, range_eval, range_train,
                range_rig, where, 20,
                strat_list=range_strat, era_list=range_era,
                from_id=2750000, to_id=3249999)
            got, _ = read_drawn(where)
            if "PMC2800002" not in got:
                fails.append("with the strat-300 refusal taken out, the "
                             "strat document is still refused -- something "
                             "else refuses it, and this test is not testing "
                             "the refusal")
            mutant = load(source.replace(ERA_GUARD, "if False:"),
                          "draw_mutant_era")
            where = tmp / "mut_era.txt"
            run(mutant, refused_tarball, range_eval, range_train,
                range_rig, where, 20,
                strat_list=range_strat, era_list=range_era,
                from_id=2750000, to_id=3249999)
            got, _ = read_drawn(where)
            if "PMC2800005" not in got:
                fails.append("with the era-train refusal taken out, the era "
                             "document is still refused -- something else "
                             "refuses it, and this test is not testing the "
                             "refusal")
            mutant = load(source.replace(RANGE_GUARD, "if False:"),
                          "draw_mutant_range")
            where = tmp / "mut_range.txt"
            run(mutant, wide_tarball, empty_eval, empty_train,
                empty_rig, where, 30,
                strat_list=empty_strat, era_list=empty_era,
                from_id=2750000, to_id=3249999)
            got, _ = read_drawn(where)
            if not any(int(i[3:]) < 2750000 or int(i[3:]) > 3249999
                       for i in got):
                fails.append("with the range guard taken out, no out-of-range "
                             "document is drawn -- something else refuses "
                             "them, and this test is not testing the guard")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("era draw: stratified over the PMC id, eval/strat/rig/train refused "
          "by name, ranged, deterministic for the seed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
