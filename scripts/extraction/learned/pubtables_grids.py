#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables cell grids, once, keyed by page (#574).

    pubtables_grids.py --out grids.jsonl.gz

WHY THIS EXISTS. Every consumer of the annotations streamed the 1.4 GB tar and
held the parsed result for a whole split: on a Mac several sessions share,
three runs in a row were killed for memory. The tar carries a great deal this
lane never reads. This writes the four fields per table and four per cell that
it does, keyed by the page's sha256, so a run holds one page at a time.

It is a cache and not a source: the tar remains the truth, this is derived
from it in one pass and can be deleted at any time.
"""
from __future__ import annotations

import argparse
import gzip
import json
import pathlib
import sys
import tarfile

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=pathlib.Path, required=True)
    args = ap.parse_args()

    manifest = json.loads((STORE / "pdf" / "MANIFEST.json").read_text())
    pages = {}
    for p in manifest["pages"]:
        pages.setdefault(p["document"], []).append(p)

    written = tables = 0
    with gzip.open(args.out, "wt") as out:
        with tarfile.open(STORE / "PubTables-1M-PDF_Annotations.tar.gz", "r|gz") as tar:
            for member in tar:
                if not member.name.endswith("_tables.json"):
                    continue
                pmc = member.name.split("_")[0]
                if pmc not in pages:
                    continue
                handle = tar.extractfile(member)
                if handle is None:
                    continue
                try:
                    found = json.load(handle)
                except Exception:
                    continue
                for page in pages[pmc]:
                    mine = []
                    for t in found:
                        if int(t.get("pdf_page_index", -1)) != int(page["page_no"]):
                            continue
                        if t.get("exclude_for_structure"):
                            continue
                        # THE CELL'S TEXT IS PART OF THE TRUTH, and leaving it
                        # out is not a saving. Without it `html_of` writes a
                        # table of empty cells, every column is scored against
                        # nothing, and the ceiling that reads 0.9085 in
                        # `region_end_to_end.py` read 0.1504 here -- the same
                        # tables, a different truth. Caught by computing one
                        # known quantity two ways, which is the only thing that
                        # has ever caught this class (#574).
                        cells = [{"r": c.get("row_nums"), "c": c.get("column_nums"),
                                  "b": c.get("pdf_bbox"),
                                  "t": c.get("pdf_text_tight_bbox"),
                                  "x": c.get("pdf_text_content"),
                                  "h": 1 if c.get("is_column_header") else 0}
                                 for c in (t.get("cells") or [])]
                        if len(cells) < 4:
                            continue
                        mine.append({"box": t.get("pdf_table_bbox"),
                                     "page_height": t["pdf_full_page_bbox"][3],
                                     "cells": cells})
                    if not mine:
                        continue
                    out.write(json.dumps({"sha256": page["sha256"],
                                          "document": pmc,
                                          "tables": mine}) + "\n")
                    written += 1
                    tables += len(mine)
    print(f"wrote {written} page(s) and {tables} table(s) to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
