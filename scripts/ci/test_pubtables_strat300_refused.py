#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables trainers refuse the strat-300 measure set by name (#638).

That set is for measuring only: a model that has seen it cannot be measured
on it. So `train_table_is` refuses a truth file naming any of its pages, and
`table_verifier` refuses training candidates naming any of them -- both by
page name, both before LightGBM loads, as they refuse the eval list. Six
cases:

- both guards read the committed `pubtables_eval_strat300.txt` beside them;
- a strat-300 page in the truth, and in the candidates, is refused with the
  page named;
- pages outside the set train through the guard;
- a missing measure set refuses the run -- without it a run would train as
  if the set were empty;
- no strat-300 page's document is on the rig, so the refusal is the guard's
  and not the rig's.

And two mutations: each guard call is taken out of a copy of its trainer,
and the strat-300 page then has to train through -- which proves the guard
is what stopped it, and not something else in the file.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
STRAT = LEARNED / "pubtables_eval_strat300.txt"
IS = LEARNED / "train_table_is.py"
VERIFIER = LEARNED / "table_verifier.py"
sys.path.insert(0, str(LEARNED))

IS_GUARD = "    refuse_strat_pages(args.pubtables_truth)"
VERIFIER_GUARD = "    refuse_strat_pages((args.pubtables, args.doclaynet))"


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


def strat_name() -> str:
    for raw in STRAT.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            return line
    raise AssertionError("the committed strat-300 set names no page")


def refuses_is(module, dump: str, pubtables: str, truth: str) -> bool:
    old = sys.argv
    sys.argv = ["train_table_is.py", "--dump", dump, "--pubtables", pubtables,
                "--pubtables-truth", truth]
    try:
        module.main()
        return False
    except SystemExit:
        return True
    finally:
        sys.argv = old


def trains_through_is(module, dump: str, pubtables: str, truth: str) -> bool:
    """True when the run gets past the guard: it returns, or it dies of
    anything but a refusal.

    LightGBM never loads here: the guard stands before its import, and
    executing it on this interpreter crashes the process with no traceback
    (two OpenMP runtimes; see the learned README). A mutant that reaches the
    import has trained through the guard, whatever dies next.
    """
    old = sys.argv
    sys.argv = ["train_table_is.py", "--dump", dump, "--pubtables", pubtables,
                "--pubtables-truth", truth]
    sys.modules["lightgbm"] = None  # type: ignore[assignment]
    try:
        module.main()
        return True
    except SystemExit:
        return False
    except Exception:
        return True
    finally:
        sys.argv = old
        del sys.modules["lightgbm"]


def refuses_verifier(module, pubtables: str, doclaynet: str) -> bool:
    args = argparse.Namespace(pubtables=pubtables, doclaynet=doclaynet, cap=10,
                              trees=2, save=None, model_dir=None)
    try:
        module.train(args)
        return False
    except SystemExit:
        return True


def trains_through_verifier(module, pubtables: str, doclaynet: str) -> bool:
    # LightGBM never loads here either: see trains_through_is.
    args = argparse.Namespace(pubtables=pubtables, doclaynet=doclaynet, cap=10,
                              trees=2, save=None, model_dir=None)
    sys.modules["lightgbm"] = None  # type: ignore[assignment]
    try:
        module.train(args)
        return True
    except SystemExit:
        return False
    except Exception:
        return True
    finally:
        del sys.modules["lightgbm"]


def main() -> int:
    import table_verifier as real_verifier
    import train_table_is as real_is
    import training_rules as rules
    fails = []
    if pathlib.Path(real_is.STRAT300) != STRAT:
        fails.append("the is-table guard does not read the committed "
                     f"strat-300 set: {real_is.STRAT300}")
    if pathlib.Path(real_verifier.STRAT300) != STRAT:
        fails.append("the verifier guard does not read the committed "
                     f"strat-300 set: {real_verifier.STRAT300}")
    page = strat_name()
    rig = {e["document"] for e in
           json.loads((REPO / "corpus" / "structured" / "PUBTABLES1M.json")
                      .read_text(encoding="utf-8"))["pages"]}
    strat_docs = {line[:-4].rsplit("-p", 1)[0]
                  for line in STRAT.read_text(encoding="utf-8").splitlines()
                  if line.strip() and not line.startswith("#")}
    if strat_docs & rig:
        fails.append("a strat-300 document is on the rig: the refusal below "
                     "would not prove the guard")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        stamp = {"tree": rules.serving_tree(),
                 "decisions": rules.serving_decisions()}
        dump = tmp / "dump.jsonl"
        dump.write_text(json.dumps(
            {"source": "doclaynet/train", "refine": "off", "engine": stamp,
             "document": "PMC900001", "page_id": "PMC900001-p0000.pdf",
             "blocks": [], "words": [], "height": 100}) + "\n",
            encoding="utf-8")
        clean_truth = tmp / "clean_truth.json"
        clean_truth.write_text(json.dumps(
            [{"name": "PMC900001-p0000.pdf", "document": "PMC900001",
              "blocks": []}]), encoding="utf-8")
        strat_truth = tmp / "strat_truth.json"
        strat_truth.write_text(json.dumps(
            [{"name": "PMC900001-p0000.pdf", "document": "PMC900001",
              "blocks": []},
             {"name": page, "document": page[:-4].rsplit("-p", 1)[0],
              "blocks": [{"kind": "table", "bbox": [0, 0, 10, 10]}]}]),
            encoding="utf-8")
        pubs = tmp / "pubs.jsonl"
        pubs.write_text(json.dumps(
            {"source": "pubtables-1m/train", "refine": "off", "engine": stamp,
             "document": "PMC900001", "page_id": page,
             "blocks": [], "words": [], "height": 100}) + "\n",
            encoding="utf-8")

        if not refuses_is(real_is, str(dump), str(pubs), str(strat_truth)):
            fails.append("a strat-300 page in the truth trained through "
                         "is-table")
        try:
            real_is.refuse_strat_pages(str(clean_truth))
            real_is.refuse_strat_pages(None)
        except SystemExit as e:
            fails.append(f"a clean truth was refused by is-table: {e}")
        old = real_is.STRAT300
        real_is.STRAT300 = tmp / "no-strat.txt"
        try:
            real_is.refuse_strat_pages(str(clean_truth))
            fails.append("a missing measure set did not refuse is-table")
        except SystemExit:
            pass
        finally:
            real_is.STRAT300 = old

        clean_cands = tmp / "clean.jsonl"
        clean_cands.write_text(
            "".join(json.dumps({"page": "PMC900001-p0000.pdf",
                                "document": "PMC900001", "x": [0.1] * 18,
                                "lines": [0], "grade": 0.5}) + "\n"
                   for _ in range(2)), encoding="utf-8")
        strat_cands = tmp / "strat.jsonl"
        strat_cands.write_text(
            json.dumps({"page": page,
                        "document": page[:-4].rsplit("-p", 1)[0],
                        "x": [0.1] * 18, "lines": [0], "grade": 0.5}) + "\n",
            encoding="utf-8")
        if not refuses_verifier(real_verifier, str(strat_cands),
                                str(clean_cands)):
            fails.append("a strat-300 page in the candidates trained through "
                         "the verifier")
        try:
            real_verifier.refuse_strat_pages([str(clean_cands)])
        except SystemExit as e:
            fails.append(f"clean candidates were refused by the verifier: {e}")
        old = real_verifier.STRAT300
        real_verifier.STRAT300 = tmp / "no-strat.txt"
        try:
            real_verifier.refuse_strat_pages([str(clean_cands)])
            fails.append("a missing measure set did not refuse the verifier")
        except SystemExit:
            pass
        finally:
            real_verifier.STRAT300 = old

        is_source = IS.read_text(encoding="utf-8")
        verifier_source = VERIFIER.read_text(encoding="utf-8")
        for guard, name, source in (
                (IS_GUARD, "the is-table guard call", is_source),
                (VERIFIER_GUARD, "the verifier guard call", verifier_source)):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in its trainer; the "
                             "mutation cannot be made")
        if not fails:
            mutant = load(is_source.replace(IS_GUARD, "    pass"),
                          "is_mutant_strat")
            if not trains_through_is(mutant, str(dump), str(pubs),
                                     str(strat_truth)):
                fails.append("with the guard call taken out, the strat-300 "
                             "page is still refused by is-table -- something "
                             "else refuses it, and this test is not testing "
                             "the guard")
            mutant = load(verifier_source.replace(VERIFIER_GUARD, "    pass"),
                          "verifier_mutant_strat")
            if not trains_through_verifier(mutant, str(strat_cands),
                                           str(clean_cands)):
                fails.append("with the guard call taken out, the strat-300 "
                             "page is still refused by the verifier -- "
                             "something else refuses it, and this test is "
                             "not testing the guard")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("trainers: a strat-300 page is refused by name, before LightGBM "
          "loads.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
