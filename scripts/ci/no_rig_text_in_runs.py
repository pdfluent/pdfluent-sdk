#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No rig page text in the committed structured run files.

Rig page text never goes into the repo: every job can read it, so committed
rig text is a contamination route into tuning and training. A committed run
file keeps per-page SCORES + hashed page ids only -- no text snippets, no
public filenames, no truncated content of any kind.

So every `structured-*.json` under `benchmarks/runs/structured/` must carry
no `set_in` key and no `document` key anywhere, every page and document
`name` must be a 64-hex content hash (with a `.pdf` suffix where a page id
has one), every ParseBench `per_page` key must be such a hash, and every
`documents_that_did_not_answer` entry likewise. Every `structured-*.md`
beside them must carry no backticked filename and no `PMC` article id.

Deliberately NOT flagged: the score axis literally named `text` (a dict of
numbers, not page text), the tool display names (`tools.*.name`), the set
metadata (`category`, `set_version`, `sample`), and the rig's own cache
provenance (`cached_to`, `from_cache`). `paragraph-*.json` is a different
rig's probe output -- it holds no page text and no `set_in`/`document`
fields, and no gate reads it -- so this guard skips it; hashing its
pdfbox corpus keys is a separate decision.

Usage:
    python3 scripts/ci/no_rig_text_in_runs.py [--root PATH]

Exit codes:
  0  no run file carries text or a public filename
  1  a run file does
  2  the runs directory could not be read (announced, never silent)
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
RUNS = pathlib.Path("benchmarks/runs/structured")

HASHED = re.compile(r"[0-9a-f]{64}(\.pdf)?\Z")
MD_FILENAME = re.compile(r"`[^`]*\.pdf`", re.IGNORECASE)
MD_PMC = re.compile(r"\bPMC\d+")


def json_offences(path: pathlib.Path, run: object) -> list[str]:
    """Every text or filename offence in one parsed run file."""
    found: list[str] = []

    def fail(where: str, what: str) -> None:
        found.append(f"{path.name}: {where}: {what}")

    def check_name(where: str, name: object) -> None:
        if isinstance(name, str) and not HASHED.match(name):
            fail(where, f"public name {name!r}")

    def check_dnan(where: str, section: object) -> None:
        if not isinstance(section, dict):
            return
        names = section.get("documents_that_did_not_answer")
        if isinstance(names, list):
            for name in names:
                check_name(f"{where}.documents_that_did_not_answer[]", name)

    def check_tail(where: str, tail: object) -> None:
        if not isinstance(tail, dict):
            return
        for axis, rows in tail.items():
            if not isinstance(rows, list):
                continue
            for row in rows:
                if isinstance(row, dict):
                    check_name(f"{where}.{axis}[]", row.get("name"))

    # The backstop first: these two keys are never legitimate in a run
    # file, wherever they sit. (`in_documents`, `documents_measured` and
    # friends are longer keys and do not match.)
    def backstop(node: object, trail: str) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                if key in ("set_in", "document"):
                    fail(trail or "top level",
                         f"text-bearing field {key!r} is present")
                backstop(value, f"{trail}.{key}" if trail else str(key))
        elif isinstance(node, list):
            for value in node:
                backstop(value, trail + "[]")

    backstop(run, "")
    if not isinstance(run, dict):
        return found
    tools = run.get("tools")
    if isinstance(tools, dict):
        for key, column in tools.items():
            if not isinstance(column, dict):
                continue
            where = f"tools.{key}"
            for row in column.get("documents") or []:
                if isinstance(row, dict):
                    check_name(f"{where}.documents[]", row.get("name"))
            check_tail(f"{where}.tail", column.get("tail"))
            in_documents = column.get("in_documents")
            if isinstance(in_documents, dict):
                for row in in_documents.get("documents_measured") or []:
                    if isinstance(row, dict):
                        check_name(f"{where}.in_documents.documents_measured[]",
                                   row.get("name"))
                check_tail(f"{where}.in_documents.tail",
                           in_documents.get("tail"))
                check_dnan(f"{where}.in_documents", in_documents)
            check_dnan(f"{where}.furniture_in_the_document",
                       column.get("furniture_in_the_document"))
            groups = column.get("groups")
            if isinstance(groups, dict):
                for group, content in groups.items():
                    per_page = (content or {}).get("per_page") \
                        if isinstance(content, dict) else None
                    if isinstance(per_page, dict):
                        for name in per_page:
                            check_name(f"{where}.groups.{group}.per_page",
                                       name)
    provenance = run.get("order_provenance")
    if isinstance(provenance, dict):
        for row in provenance.get("per_page") or []:
            if isinstance(row, dict):
                check_name("order_provenance.per_page[]", row.get("name"))
    return found


def md_offences(path: pathlib.Path, text: str) -> list[str]:
    """Every filename offence in one `.md` companion."""
    found: list[str] = []
    for match in MD_FILENAME.findall(text):
        found.append(f"{path.name}: backticked filename {match}")
    for match in sorted(set(MD_PMC.findall(text))):
        found.append(f"{path.name}: public article id {match}")
    return found


def scan(runs: pathlib.Path) -> tuple[list[str], int]:
    """All offences under the runs directory, plus the files scanned."""
    offences: list[str] = []
    scanned = 0
    for path in sorted(runs.glob("structured-*.json")):
        scanned += 1
        try:
            run = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            offences.append(f"{path.name}: unreadable: {e}")
            continue
        offences.extend(json_offences(path, run))
    for path in sorted(runs.glob("structured-*.md")):
        scanned += 1
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as e:
            offences.append(f"{path.name}: unreadable: {e}")
            continue
        offences.extend(md_offences(path, text))
    return offences, scanned


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(REPO))
    args = ap.parse_args(argv[1:])
    runs = pathlib.Path(args.root) / RUNS
    if not runs.is_dir():
        print(f"[rig-text] FATAL: {runs} is not a directory.",
              file=sys.stderr)
        return 2
    offences, scanned = scan(runs)
    if not scanned:
        print(f"[rig-text] FATAL: no structured run files under {runs}; "
              "refusing to call an empty scan clean.", file=sys.stderr)
        return 2
    if offences:
        print(f"[rig-text] FAIL: rig text or a public filename in "
              f"{len(offences)} place(s):", file=sys.stderr)
        for offence in offences[:20]:
            print(f"    {offence}", file=sys.stderr)
        if len(offences) > 20:
            print(f"    ... and {len(offences) - 20} more.", file=sys.stderr)
        print("\n  Strip the run file before committing it:\n"
              "    python3 scripts/ci/loop/strip_run_text.py "
              "<benchmarks/runs/structured/run.json>",
              file=sys.stderr)
        return 1
    print(f"[rig-text] OK: {scanned} run file(s) carry scores and "
          "hashed ids only.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
