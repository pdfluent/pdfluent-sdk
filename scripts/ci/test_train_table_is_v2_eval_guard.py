#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The final-table trainer reads train documents only (#638).

`scripts/extraction/learned/train_table_is_v2.py` trains on PubTables pages
whose documents a list file names. Four guards keep the eval set out:

- a file named `pubtables_eval_documents.txt` is refused by NAME, always --
  no content check can tell a train list from an eval one, and a swapped
  argument must not teach the model its own exam;
- `--pubtables-documents` defaults to the checked-in train list, so
  `--pubtables` alone still trains on a NAMED train set, never an unnamed
  one, and a retrain runs without absolute paths;
- lines starting with '#' in a list file are comments and skipped, so a
  headed list cannot smuggle a document in as a heading;
- a train list NAMING an eval document is refused by CONTENT: a renamed
  eval list passes the name refusal, so the checked-in eval list is read
  too and any overlap refused.

And four mutations: each guard is taken out of a copy of the trainer, and
the refused input then has to get through -- which proves the guard is what
stopped it, and not something else in the file.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
TRAINER = LEARNED / "train_table_is_v2.py"

EVAL_NAME = "pubtables_eval_documents.txt"
EVAL_GUARD = "if pathlib.Path(path).name == PUBTABLES_EVAL_DOCUMENTS:"
DEFAULT_GUARD = 'default=str(TRAIN_LIST)'
COMMENT_GUARD = 'if not line or line.startswith("#"):'
OVERLAP_GUARD = "overlap = allowed_pub & read_eval_documents(EVAL_LIST)"


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


def refuses(module, path: pathlib.Path) -> bool:
    try:
        module.read_document_list(str(path))
        return False
    except SystemExit:
        return True


def default_documents(module) -> str:
    """The --pubtables-documents value when the flag is not passed."""
    return module.make_parser().parse_args(
        ["--dump", "d.jsonl", "--pubtables", "p.jsonl"]).pubtables_documents


def refuses_overlap(module, documents: pathlib.Path) -> bool:
    """True when training on `documents` is refused (before LightGBM loads)."""
    old = sys.argv
    sys.argv = ["train_table_is_v2.py", "--dump", "d.jsonl",
                "--pubtables", "p.jsonl",
                "--pubtables-documents", str(documents)]
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            module.main()
        return False
    except SystemExit:
        return True
    finally:
        sys.argv = old


def main() -> int:
    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_table_is_v2_real")
    fails = []
    # THE DEFAULTS POINT AT THE CHECKED-IN LISTS, beside the trainer: the
    # train documents a retrain reads, and the eval documents it must never
    # read. Disjoint, or every default run would refuse itself.
    train_list = LEARNED / "pubtables_train_documents.txt"
    eval_list = LEARNED / EVAL_NAME
    if pathlib.Path(real.TRAIN_LIST) != train_list:
        fails.append(f"the default train list is not the checked-in one: {real.TRAIN_LIST}")
    if pathlib.Path(real.EVAL_LIST) != eval_list:
        fails.append(f"the default eval list is not the checked-in one: {real.EVAL_LIST}")
    train_docs = real.read_document_list(str(train_list)) if train_list.exists() else set()
    eval_docs = real.read_eval_documents(str(eval_list)) if eval_list.exists() else set()
    if not train_docs:
        fails.append("the checked-in train list names no documents")
    if not eval_docs:
        fails.append("the checked-in eval list names no documents")
    if train_docs & eval_docs:
        fails.append("the checked-in train and eval lists overlap: "
                     "every default run would refuse itself")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        listed = (tmp / "pubtables_train_documents.txt")
        listed.write_text("# PubTables train documents, one per line\n"
                          "\n"
                          "PMC1111111\n"
                          "  # a heading mid-list is still a comment\n"
                          "PMC2222222\n",
                          encoding="utf-8")
        if real.read_document_list(str(listed)) != {"PMC1111111", "PMC2222222"}:
            fails.append("a train list did not read as its two documents")
        evil = tmp / EVAL_NAME
        evil.write_text("PMC1111111\n", encoding="utf-8")
        if not refuses(real, evil):
            fails.append("the eval documents list was not refused by name")
        nested = tmp / "sub"
        nested.mkdir()
        moved = nested / EVAL_NAME
        moved.write_text("PMC1111111\n", encoding="utf-8")
        if not refuses(real, moved):
            fails.append("the eval documents list was not refused from a subdirectory")
        if default_documents(real) != str(train_list):
            fails.append("--pubtables-documents does not default to the "
                         f"checked-in train list: {default_documents(real)}")
        contaminated = tmp / "train_with_eval.txt"
        some_eval = sorted(eval_docs)[0] if eval_docs else "PMC0000000"
        contaminated.write_text(f"PMC1111111\n{some_eval}\n", encoding="utf-8")
        if not refuses_overlap(real, contaminated):
            fails.append("a train list naming an eval document was not refused")

        for guard, name in ((EVAL_GUARD, "the eval-name refusal"),
                            (DEFAULT_GUARD, "the train-list default"),
                            (COMMENT_GUARD, "the '#' skip"),
                            (OVERLAP_GUARD, "the eval-overlap refusal")):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in train_table_is_v2.py; "
                             "the mutation cannot be made")
        if fails:
            for f in fails:
                print(f"FAIL: {f}", file=sys.stderr)
            return 1

        mutant = load(source.replace(EVAL_GUARD, "if False:"),
                      "train_table_is_v2_mutant_eval")
        if refuses(mutant, evil):
            fails.append("with the eval-name refusal taken out, the eval list is "
                         "still refused -- something else refuses it, and this test "
                         "is not testing the refusal")
        mutant = load(source.replace(DEFAULT_GUARD, "default=None"),
                      "train_table_is_v2_mutant_default")
        if default_documents(mutant) == str(train_list):
            fails.append("with the train-list default taken out, --pubtables "
                         "still reads the checked-in list -- something else "
                         "points it there, and this test is not testing "
                         "the default")
        mutant = load(source.replace(COMMENT_GUARD, "if not line:"),
                      "train_table_is_v2_mutant_comment")
        if mutant.read_document_list(str(listed)) == {"PMC1111111", "PMC2222222"}:
            fails.append("with the '#' skip taken out, the comments are still "
                         "skipped -- something else skips them, and this test is "
                         "not testing the skip")
        mutant = load(source.replace(OVERLAP_GUARD, "overlap = set()"),
                      "train_table_is_v2_mutant_overlap")
        try:
            through = not refuses_overlap(mutant, contaminated)
        except Exception:
            through = True
        if not through:
            fails.append("with the overlap refusal taken out, the contaminated "
                         "list is still refused -- something else refuses it, "
                         "and this test is not testing the refusal")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("table-is-v2 trainer: the eval list is refused by name and by content, "
          "--pubtables-documents defaults to the checked-in train list, and "
          "'#' lines are skipped.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
