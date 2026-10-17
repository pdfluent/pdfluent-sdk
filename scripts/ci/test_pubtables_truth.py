#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables truth file carries each kept page's own tables (#638).

`pubtables_truth.py` writes, for each kept page of a fetch manifest, the
page's tables from the annotation tarball in the format the trainers consume:
`source`, `name`, `document`, `page_no`, `path`, `blocks` of
`{kind: "table", text, html, order, bbox}`. Five cases on a synthetic manifest
and tarball:

- every entry names the manifest page and every block the annotation's table,
  html and text as `pubtables_cells` gives them, in order;
- a page carries its own page's tables: `page_no` is the 0-based index the
  fetch wrote, and a page between two table pages carries neither's;
- a manifest document the tarball never held is skipped, and a page with no
  table above the floor is kept with no blocks;
- documents the manifest does not name are not in the truth file;
- the entries are in manifest order.

And two mutations: the block kind and the page match are each taken out of a
copy of the writer, and the blocks then come out kindless and the pages then
carry page 0's tables -- which proves each line is what wrote them.
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
TRUTH = LEARNED / "pubtables_truth.py"
sys.path.insert(0, str(LEARNED))

KIND_GUARD = '"kind": "table",'
PAGE_GUARD = "tables = by_page.get(page[\"page_no\"], [])"


def table(page, words, split="train"):
    rows = max(2, (len(words) + 1) // 2)
    cells = []
    for at, word in enumerate(words):
        cells.append({"row_nums": [at // 2], "column_nums": [at % 2],
                      "pdf_text_content": word})
    while len(cells) < 4:
        cells.append({"row_nums": [rows], "column_nums": [len(cells) % 2],
                      "pdf_text_content": "pad"})
    return {"split": split, "pdf_page_index": page,
            "pdf_table_bbox": [1.0 * page, 2.0, 30.0, 40.0], "cells": cells}


def write_tarball(path: pathlib.Path, members: dict[str, list[dict]]) -> None:
    with tarfile.open(path, "w:gz") as tar:
        for name, annotation in members.items():
            data = json.dumps(annotation).encode("utf-8")
            info = tarfile.TarInfo(f"{name}_tables.json")
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))


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


def main() -> int:
    import pubtables_truth as real
    import pubtables_cells
    source = TRUTH.read_text(encoding="utf-8")
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        ann_a = [table(0, ["alpha", "beta", "gamma", "delta"]),
                 table(0, ["one", "two", "three", "four"]),
                 table(2, ["red", "green", "blue", "yellow"])]
        ann_b = [table(1, ["up", "down", "left", "right"])]
        tarball = tmp / "ann.tar.gz"
        write_tarball(tarball, {"PMC100001": ann_a, "PMC100002": ann_b,
                                "PMC100009": [table(0, ["stray"] * 4)]})
        manifest = tmp / "MANIFEST.json"
        pages = [
            {"name": "PMC100001-p0000.pdf", "document": "PMC100001",
             "page_no": 0},
            {"name": "PMC100001-p0002.pdf", "document": "PMC100001",
             "page_no": 2},
            {"name": "PMC100002-p0001.pdf", "document": "PMC100002",
             "page_no": 1},
            {"name": "PMC100002-p0005.pdf", "document": "PMC100002",
             "page_no": 5},
            {"name": "PMC100007-p0000.pdf", "document": "PMC100007",
             "page_no": 0},
        ]
        manifest.write_text(json.dumps({"pages": pages}), encoding="utf-8")
        out = tmp / "TRUTH.json"
        if real.main(["--manifest", str(manifest),
                      "--annotations", str(tarball),
                      "--pages-dir", str(tmp / "pages"),
                      "--out", str(out)]) != 0:
            fails.append("the writer did not exit 0")
        truth = json.loads(out.read_text(encoding="utf-8"))
        by_name = {p["name"]: p for p in truth}
        if [p["name"] for p in truth] != [p["name"] for p in pages[:4]]:
            fails.append("the entries are not the manifest's pages in "
                         f"order: {[p['name'] for p in truth]}")
        first = by_name["PMC100001-p0000.pdf"]
        if (first["source"], first["document"], first["page_no"]) != \
                ("pubtables-1m/train", "PMC100001", 0):
            fails.append(f"the entry does not name its page: {first}")
        if first["path"] != str(tmp / "pages" / "PMC100001-p0000.pdf"):
            fails.append(f"the entry does not locate its page: {first}")
        want = pubtables_cells.tables_by_page(ann_a)[0]
        if len(first["blocks"]) != 2:
            fails.append(f"the two-table page has {len(first['blocks'])} "
                         "blocks")
        for order, (block, found) in enumerate(zip(first["blocks"], want)):
            if block["kind"] != "table" or block["order"] != order:
                fails.append(f"the block is not table {order}: {block}")
            if block["html"] != found["html"] or \
                    block["text"] != pubtables_cells.text_of(found["html"]):
                fails.append("the block is not the annotation's table: "
                             f"{block}")
            if block["bbox"] != found["bbox"]:
                fails.append(f"the block lost its box: {block}")
        if "alpha beta gamma delta" not in first["blocks"][0]["text"]:
            fails.append("the first block's text is not its cells' words")
        second = by_name["PMC100001-p0002.pdf"]
        if len(second["blocks"]) != 1 or \
                "red green blue yellow" not in second["blocks"][0]["text"]:
            fails.append("page 2 does not carry page 2's table: "
                         f"{second['blocks']}")
        if [b["bbox"] for b in second["blocks"]] != [[2.0, 2.0, 30.0, 40.0]]:
            fails.append("page 2's box is not page 2's")
        third = by_name["PMC100002-p0001.pdf"]
        if len(third["blocks"]) != 1 or \
                "up down left right" not in third["blocks"][0]["text"]:
            fails.append("the second document's page is wrong")
        if by_name["PMC100002-p0005.pdf"]["blocks"]:
            fails.append("a page with no table above the floor has blocks")
        if any(p["document"] == "PMC100009" for p in truth):
            fails.append("an unnamed document is in the truth file")

        for guard, name in ((KIND_GUARD, "the block kind"),
                            (PAGE_GUARD, "the page match")):
            if guard not in source:
                fails.append(f"{name} ({guard!r}) is not in "
                             "pubtables_truth.py; the mutation cannot be made")
        if not fails:
            mutant = load(source.replace(KIND_GUARD, ""),
                          "truth_mutant_kind")
            where = tmp / "mut_kind.json"
            mutant.main(["--manifest", str(manifest),
                         "--annotations", str(tarball),
                         "--pages-dir", str(tmp / "pages"), "--out", str(where)])
            blocks = json.loads(where.read_text(encoding="utf-8"))[0]["blocks"]
            if all(b.get("kind") == "table" for b in blocks):
                fails.append("with the kind taken out, the blocks still "
                             "carry kind table -- something else writes it, "
                             "and this test is not testing the line")
            mutant = load(source.replace(PAGE_GUARD,
                                         "tables = by_page.get(0, [])"),
                          "truth_mutant_page")
            where = tmp / "mut_page.json"
            mutant.main(["--manifest", str(manifest),
                         "--annotations", str(tarball),
                         "--pages-dir", str(tmp / "pages"), "--out", str(where)])
            got = {p["name"]: p
                   for p in json.loads(where.read_text(encoding="utf-8"))}
            if got["PMC100001-p0002.pdf"]["blocks"] == \
                    by_name["PMC100001-p0002.pdf"]["blocks"]:
                fails.append("with the page match taken out, page 2 still "
                             "carries page 2's tables -- something else "
                             "matches them, and this test is not testing "
                             "the match")
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("truth: each kept page's own tables, in the format the trainers "
          "read.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
