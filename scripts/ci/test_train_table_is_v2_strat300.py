#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The final-table trainer refuses the strat-300 measure set (#638).

That set is for measuring only: a model that has seen it cannot be measured
on it. So `train_table_is_v2.py` refuses it exactly as the other trainers
do -- by the list file name `pubtables_eval_strat300.txt` and by content --
before LightGBM loads. Six cases:

- the guard reads the committed `pubtables_eval_strat300.txt` beside it;
- a file NAMED `pubtables_eval_strat300.txt` is refused as a train list,
  as the eval list is;
- a strat-300 page in the training truth is refused with the page named,
  and so is another page of a strat-300 document: training on a measure
  document's other page is still seeing the measure;
- a strat-300 page in a training dump is refused with the page named;
- a train documents list NAMING a strat-300 document is refused with the
  document named;
- pages and documents outside the set train through the guard, and a
  missing measure set refuses the run -- without it a run would train as
  if the set were empty.

And five mutations: the name refusal, the guard call, and each of the three
content checks (truth, dumps, documents) is taken out of a copy of the
trainer, and the refused input then has to get through -- which proves each
guard is what stopped it, and not something else in the file.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
TRAINER = LEARNED / "train_table_is_v2.py"
STRAT = LEARNED / "pubtables_eval_strat300.txt"
sys.path.insert(0, str(LEARNED))

STRAT_NAME = "pubtables_eval_strat300.txt"
NAME_GUARD = "if pathlib.Path(path).name == PUBTABLES_STRAT300:"
CALL_GUARD = ("    refuse_strat_pages(args.pubtables_truth, (args.dump, args.pubtables),\n"
              "                       allowed_pub)")
TRUTH_GUARD = 'if name in strat or page.get("document") in strat_docs:'
DUMP_GUARD = 'if page_id in strat or record.get("document") in strat_docs:'
DOCS_GUARD = "overlap = set(documents) & strat_docs"


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


def strat_page() -> str:
    for raw in STRAT.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            return line
    raise AssertionError("the committed strat-300 set names no page")


def strat_document(page: str) -> str:
    stem = page[:-4] if page.endswith(".pdf") else page
    cut = stem.rfind("-p")
    return stem[:cut] if cut > 0 else stem


def refusal_of(module, argv: list[str]) -> str | None:
    """The SystemExit message of a main() run, or None when it returns.

    Anything but a refusal propagates: the refusal happens before LightGBM
    loads, so a refused run never reaches the import.
    """
    old = sys.argv
    sys.argv = argv
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.main()
        return None
    except SystemExit as e:
        return str(e)
    finally:
        sys.argv = old


def trains_through(module, argv: list[str]) -> bool:
    """True when the run gets past the guard: it returns, or it dies of
    anything but a refusal.

    LightGBM never loads here: the guard stands before its import, and
    executing it on this interpreter crashes the process with no traceback
    (two OpenMP runtimes; see the learned README). A mutant that reaches the
    import has trained through the guard, whatever dies next.
    """
    old = sys.argv
    sys.argv = argv
    sys.modules["lightgbm"] = None  # type: ignore[assignment]
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.main()
        return True
    except SystemExit:
        return False
    except Exception:
        return True
    finally:
        sys.argv = old
        del sys.modules["lightgbm"]


def main() -> int:
    import pubtables_splits as splits

    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_table_is_v2_real_strat")
    fails = []
    if pathlib.Path(real.STRAT300) != STRAT:
        fails.append("the v2 guard does not read the committed strat-300 "
                     f"set: {real.STRAT300}")
    page = strat_page()
    document = strat_document(page)
    if splits.document_of(page) != document:
        fails.append(f"the test reads {page} as {document} but the trainer "
                     "reads it as something else")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        clean_docs = tmp / "clean_documents.txt"
        clean_docs.write_text("PMC900001\n", encoding="utf-8")
        strat_docs = tmp / "strat_documents.txt"
        strat_docs.write_text(f"PMC900001\n{document}\n", encoding="utf-8")
        clean_truth = tmp / "clean_truth.json"
        clean_truth.write_text(json.dumps(
            [{"name": "PMC900001-p0000.pdf", "document": "PMC900001",
              "blocks": []}]), encoding="utf-8")
        page_truth = tmp / "page_truth.json"
        page_truth.write_text(json.dumps(
            [{"name": "PMC900001-p0000.pdf", "document": "PMC900001",
              "blocks": []},
             {"name": page, "document": document,
              "blocks": [{"kind": "table", "bbox": [0, 0, 10, 10]}]}]),
            encoding="utf-8")
        doc_truth = tmp / "doc_truth.json"
        doc_truth.write_text(json.dumps(
            [{"name": f"{document}-p0099.pdf", "document": document,
              "blocks": [{"kind": "table", "bbox": [0, 0, 10, 10]}]}]),
            encoding="utf-8")
        clean_dump = tmp / "clean.jsonl"
        clean_dump.write_text(json.dumps(
            {"document": "PMC900001", "page_id": "PMC900001-p0000.pdf",
             "height": 100}) + "\n", encoding="utf-8")
        page_dump = tmp / "page.jsonl"
        page_dump.write_text(json.dumps(
            {"document": document, "page_id": page, "height": 100}) + "\n",
            encoding="utf-8")
        pubs = tmp / "pubs.jsonl"
        pubs.write_text(json.dumps(
            {"document": "PMC900001", "page_id": "PMC900001-p0000.pdf",
             "height": 100}) + "\n", encoding="utf-8")

        def argv(dump=clean_dump, pubtables=pubs, truth=clean_truth,
                documents=clean_docs):
            return ["train_table_is_v2.py", "--dump", str(dump),
                    "--pubtables", str(pubtables), "--pubtables-truth",
                    str(truth), "--pubtables-documents", str(documents)]

        evil = tmp / STRAT_NAME
        evil.write_text("PMC900001\n", encoding="utf-8")
        try:
            real.read_document_list(str(evil))
            fails.append("the strat-300 list was not refused by name")
        except SystemExit as e:
            if STRAT_NAME not in str(e):
                fails.append("the strat-300 name refusal does not name the "
                             f"list: {e}")
        nested = tmp / "sub"
        nested.mkdir()
        moved = nested / STRAT_NAME
        moved.write_text("PMC900001\n", encoding="utf-8")
        try:
            real.read_document_list(str(moved))
            fails.append("the strat-300 list was not refused from a subdirectory")
        except SystemExit:
            pass

        refused = refusal_of(real, argv(truth=page_truth))
        if refused is None:
            fails.append("a strat-300 page in the truth trained through v2")
        elif page not in refused or "strat-300" not in refused:
            fails.append("the truth refusal does not name the strat-300 "
                         f"page: {refused}")
        refused = refusal_of(real, argv(truth=doc_truth))
        if refused is None:
            fails.append("another page of a strat-300 document in the truth "
                         "trained through v2")
        elif f"{document}-p0099.pdf" not in refused:
            fails.append("the truth refusal does not name the strat-300 "
                         f"document's page: {refused}")
        refused = refusal_of(real, argv(dump=page_dump))
        if refused is None:
            fails.append("a strat-300 page in the dump trained through v2")
        elif page not in refused:
            fails.append("the dump refusal does not name the strat-300 "
                         f"page: {refused}")

        # A strat-300 document is an eval document, so the eval-overlap
        # refusal fires first on the real eval list. The patched eval list
        # below holds no document at all, so only the strat guard can refuse
        # the strat document -- which is what this case proves.
        empty_eval = tmp / "empty_eval.txt"
        empty_eval.write_text("# no eval documents\n", encoding="utf-8")
        old_eval = real.EVAL_LIST
        real.EVAL_LIST = empty_eval
        try:
            refused = refusal_of(real, argv(documents=strat_docs))
        finally:
            real.EVAL_LIST = old_eval
        if refused is None:
            fails.append("a train list naming a strat-300 document trained "
                         "through v2")
        elif document not in refused or "strat-300" not in refused:
            fails.append("the documents refusal does not name the strat-300 "
                         f"document: {refused}")

        try:
            real.refuse_strat_pages(str(clean_truth), [str(clean_dump)],
                                    {"PMC900001"})
            real.refuse_strat_pages(None, [], set())
        except SystemExit as e:
            fails.append(f"clean inputs were refused by v2: {e}")
        old = real.STRAT300
        real.STRAT300 = tmp / "no-strat.txt"
        try:
            real.refuse_strat_pages(str(clean_truth), [str(clean_dump)],
                                    {"PMC900001"})
            fails.append("a missing measure set did not refuse v2")
        except SystemExit:
            pass
        finally:
            real.STRAT300 = old

        for guard, name in ((NAME_GUARD, "the strat-name refusal"),
                            (CALL_GUARD, "the strat guard call"),
                            (TRUTH_GUARD, "the truth content check"),
                            (DUMP_GUARD, "the dump content check"),
                            (DOCS_GUARD, "the documents content check")):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in "
                             "train_table_is_v2.py; the mutation cannot be made")
        if fails:
            for f in fails:
                print(f"FAIL: {f}", file=sys.stderr)
            return 1

        mutant = load(source.replace(NAME_GUARD, "if False:"),
                      "train_table_is_v2_mutant_strat_name")
        try:
            mutant.read_document_list(str(evil))
        except SystemExit:
            fails.append("with the strat-name refusal taken out, the "
                         "strat-300 list is still refused -- something else "
                         "refuses it, and this test is not testing the refusal")
        mutant = load(source.replace(CALL_GUARD, "    pass"),
                      "train_table_is_v2_mutant_strat_call")
        if not trains_through(mutant, argv(truth=page_truth)):
            fails.append("with the guard call taken out, the strat-300 page "
                         "is still refused by v2 -- something else refuses "
                         "it, and this test is not testing the guard")
        mutant = load(source.replace(TRUTH_GUARD, "if False:"),
                      "train_table_is_v2_mutant_strat_truth")
        try:
            mutant.refuse_strat_pages(str(page_truth), [str(clean_dump)],
                                      {"PMC900001"})
        except SystemExit:
            fails.append("with the truth check taken out, the strat-300 page "
                         "is still refused -- something else refuses it, and "
                         "this test is not testing the check")
        mutant = load(source.replace(DUMP_GUARD, "if False:"),
                      "train_table_is_v2_mutant_strat_dump")
        try:
            mutant.refuse_strat_pages(str(clean_truth), [str(page_dump)],
                                      {"PMC900001"})
        except SystemExit:
            fails.append("with the dump check taken out, the strat-300 page "
                         "is still refused -- something else refuses it, and "
                         "this test is not testing the check")
        mutant = load(source.replace(DOCS_GUARD, "overlap = set()"),
                      "train_table_is_v2_mutant_strat_docs")
        try:
            mutant.refuse_strat_pages(str(clean_truth), [str(clean_dump)],
                                      {"PMC900001", document})
        except SystemExit:
            fails.append("with the documents check taken out, the strat-300 "
                         "document is still refused -- something else refuses "
                         "it, and this test is not testing the check")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("table-is-v2 trainer: the strat-300 set is refused by name and by "
          "content (truth, dumps, documents), before LightGBM loads.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
