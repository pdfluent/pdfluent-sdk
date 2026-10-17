#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The final-table trainer reads the era train list beside the old one (#638).

`train_table_is_v2.py --train-lists` chooses which PubTables train lists a
run reads: the original list, the era-balanced addition, or both (the
default). Six cases:

- the flag defaults to both, and the era path defaults to the checked-in
  `pubtables_train_documents_era.txt` beside the trainer;
- with the era file present, both reads the union, original the original
  list only, era the era list only -- each headed list read as its own
  documents, '#' lines skipped;
- with the era file MISSING, both trains on the original list and says one
  stderr line, `era train list missing: training on the original list
  only` -- never refused, never silent;
- with the era file missing, an explicit era is refused with a clear error:
  it asked for a list that is not there;
- a main() run on both with the era file missing prints that line and
  continues past the list resolution instead of refusing;
- the era path goes through the same name refusal as the original list: a
  file named `pubtables_eval_documents.txt` never trains, whichever flag
  points at it.

And three mutations: the both default, the union, and the stderr line are
each taken out of a copy of the trainer, and the cased input then has to
read or say otherwise -- which proves each is what the test observes, and
not something else in the file.
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

NOTE = "era train list missing: training on the original list only"
DEFAULT_BOTH = '"--train-lists", default="both",'
UNION = "return original | read_document_list(era_path)"
WARN = "print(ERA_MISSING_NOTE, file=sys.stderr)"


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


def train_lists_default(module) -> str:
    return module.make_parser().parse_args(["--dump", "d.jsonl"]).train_lists


def era_default(module) -> str:
    return module.make_parser().parse_args(
        ["--dump", "d.jsonl"]).pubtables_era_documents


def read(module, original: pathlib.Path, era: pathlib.Path, which: str):
    """(documents, stderr) of one read_train_documents call."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        documents = module.read_train_documents(str(original), str(era), which)
    return documents, err.getvalue()


def main() -> int:
    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_table_is_v2_real_era")
    fails = []
    if real.ERA_MISSING_NOTE != NOTE:
        fails.append(f"the missing-era line is not the agreed one: {real.ERA_MISSING_NOTE!r}")
    if pathlib.Path(real.ERA_LIST) != LEARNED / "pubtables_train_documents_era.txt":
        fails.append(f"the era default is not the checked-in list: {real.ERA_LIST}")
    if train_lists_default(real) != "both":
        fails.append("--train-lists does not default to both: "
                     f"{train_lists_default(real)}")
    if era_default(real) != str(LEARNED / "pubtables_train_documents_era.txt"):
        fails.append("--pubtables-era-documents does not default to the "
                     f"checked-in era list: {era_default(real)}")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        original = tmp / "original.txt"
        original.write_text("# the original train list\n"
                            "\n"
                            "PMC1111111\n"
                            "PMC2222222\n",
                            encoding="utf-8")
        era = tmp / "era.txt"
        era.write_text("# the era-balanced addition\n"
                       "PMC2222222\n"
                       "PMC3333333\n",
                       encoding="utf-8")
        missing = tmp / "no-era.txt"

        documents, err = read(real, original, era, "both")
        if documents != {"PMC1111111", "PMC2222222", "PMC3333333"}:
            fails.append(f"both did not read the union: {sorted(documents)}")
        if err != "":
            fails.append(f"both with the era file present said: {err!r}")
        documents, err = read(real, original, era, "original")
        if documents != {"PMC1111111", "PMC2222222"}:
            fails.append(f"original did not read the original list only: {sorted(documents)}")
        if err != "":
            fails.append(f"original said: {err!r}")
        documents, err = read(real, original, era, "era")
        if documents != {"PMC2222222", "PMC3333333"}:
            fails.append(f"era did not read the era list only: {sorted(documents)}")
        if err != "":
            fails.append(f"era with the file present said: {err!r}")

        documents, err = read(real, original, missing, "both")
        if documents != {"PMC1111111", "PMC2222222"}:
            fails.append("both with the era file missing did not fall back "
                         f"to the original list: {sorted(documents)}")
        if err != NOTE + "\n":
            fails.append("both with the era file missing did not say the "
                         f"one agreed line: {err!r}")
        documents, err = read(real, original, missing, "original")
        if documents != {"PMC1111111", "PMC2222222"}:
            fails.append("original with the era file missing did not read "
                         f"the original list: {sorted(documents)}")
        if err != "":
            fails.append("original with the era file missing said: "
                         f"{err!r}")
        try:
            read(real, original, missing, "era")
            fails.append("an explicit era with the file missing was not refused")
        except SystemExit as e:
            if "era train list" not in str(e) or str(missing) not in str(e):
                fails.append(f"the missing-era refusal is not clear: {e}")
        try:
            real.read_train_documents(str(original), str(era), "bogus")
            fails.append("an unknown train-lists choice was not refused")
        except SystemExit:
            pass

        evil = tmp / "pubtables_eval_documents.txt"
        evil.write_text("PMC1111111\n", encoding="utf-8")
        try:
            read(real, original, evil, "era")
            fails.append("the eval list was not refused as the era list")
        except SystemExit:
            pass
        try:
            read(real, original, evil, "both")
            fails.append("the eval list was not refused in both mode")
        except SystemExit:
            pass

        # Through main(): both with the era file missing prints the line and
        # continues past the list resolution -- the run then dies of the
        # missing dump, which is past the point this case proves.
        old = sys.argv
        sys.argv = ["train_table_is_v2.py", "--dump", str(tmp / "no.jsonl"),
                    "--pubtables-documents", str(original),
                    "--pubtables-era-documents", str(missing)]
        err = io.StringIO()
        try:
            with contextlib.redirect_stderr(err):
                real.main()
            fails.append("a main() run without a dump returned")
        except SystemExit as e:
            fails.append(f"a main() run on both with the era file missing "
                         f"refused instead of continuing: {e}")
        except FileNotFoundError:
            pass
        finally:
            sys.argv = old
        if err.getvalue() != NOTE + "\n":
            fails.append("a main() run on both with the era file missing did "
                         f"not say the one agreed line: {err.getvalue()!r}")

        # And an explicit era through main() refuses before LightGBM loads.
        sys.argv = ["train_table_is_v2.py", "--dump", str(tmp / "no.jsonl"),
                    "--pubtables-documents", str(original),
                    "--pubtables-era-documents", str(missing),
                    "--train-lists", "era"]
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                real.main()
            fails.append("a main() run on era with the file missing returned")
        except SystemExit as e:
            if "era train list" not in str(e):
                fails.append("a main() run on era with the file missing "
                             f"refused unclearly: {e}")
        finally:
            sys.argv = old

        for guard, name in ((DEFAULT_BOTH, "the both default"),
                            (UNION, "the union"),
                            (WARN, "the missing-era line")):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in "
                             "train_table_is_v2.py; the mutation cannot be made")
        if fails:
            for f in fails:
                print(f"FAIL: {f}", file=sys.stderr)
            return 1

        mutant = load(source.replace(DEFAULT_BOTH,
                                     '"--train-lists", default="original",'),
                      "train_table_is_v2_mutant_both")
        if train_lists_default(mutant) == "both":
            fails.append("with the both default taken out, --train-lists "
                         "still defaults to both -- something else points it "
                         "there, and this test is not testing the default")
        mutant = load(source.replace(UNION, "return original"),
                      "train_table_is_v2_mutant_union")
        documents, _ = read(mutant, original, era, "both")
        if documents == {"PMC1111111", "PMC2222222", "PMC3333333"}:
            fails.append("with the union taken out, both still reads the era "
                         "documents -- something else reads them, and this "
                         "test is not testing the union")
        mutant = load(source.replace(WARN, "pass"),
                      "train_table_is_v2_mutant_warn")
        _, err = read(mutant, original, missing, "both")
        if NOTE in err:
            fails.append("with the stderr line taken out, the missing era "
                         "list is still announced -- something else says it, "
                         "and this test is not testing the line")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("table-is-v2 trainer: --train-lists reads original, era, or both "
          "(the default), and a missing era list is one stderr line, not a "
          "refusal.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
