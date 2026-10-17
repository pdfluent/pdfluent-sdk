#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No FinTabNet train/tuning draw holds a page of an eval set.

Ten eval pages sit on the train side of the company split, where
`fintabnet_split.side()` cannot see them. `corpus/structured/
EXCLUDE_EVAL_OVERLAP.json` names their sha256 -- hashes only, never the
names -- and `fintabnet_split.page_excluded()` is the one check the
draw, the dump and the dev fetch all read.

The suite proves the refusal through a scratch exclusion file, so no
test names a real page: a listed stem is refused everywhere, an
unlisted one passes. The committed file itself is held only to its
shape -- ten distinct hashes and the reason line.
"""
from __future__ import annotations

import hashlib
import io
import json
import os
import pathlib
import sys
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "tables_a"))
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "tables_b"))

import build_manifests as manifests  # noqa: E402
import dev_manifest  # noqa: E402
import dump  # noqa: E402
import fintabnet_archive  # noqa: E402
import fintabnet_split  # noqa: E402

REASON = "pages of an eval set; never in a train/tuning draw"

LISTED = "TEST_2000_page_1"
UNLISTED = "TEST_2000_page_2"


def exclusion_file(tmp: pathlib.Path, *stems: str) -> pathlib.Path:
    """A scratch exclusion file naming `stems`, by hash like the real one."""
    path = tmp / "exclusion.json"
    path.write_text(json.dumps({
        "reason": REASON,
        "pages_sha256": [hashlib.sha256(stem.encode()).hexdigest()
                         for stem in stems]}))
    return path


def ticker(lane: str, taken: set[str]) -> str:
    """A synthetic ticker on `lane`, past every ticker in `taken`."""
    at = 0
    while True:
        name = f"C{at:03d}"
        at += 1
        if name not in taken and fintabnet_split.lane(name) == lane:
            taken.add(name)
            return name


def grid(rows: int, cols: int) -> list[dict]:
    cells = [{"row_nums": [0], "column_nums": [at],
              "is_column_header": True, "pdf_text_content": "w"}
             for at in range(cols)]
    for row in range(1, rows):
        for at in range(cols):
            cells.append({"row_nums": [row], "column_nums": [at],
                          "is_column_header": False,
                          "pdf_text_content": "w"})
    return cells


def kept_table() -> dict:
    return {"pdf_page_index": 0, "exclude_for_structure": False,
            "pdf_table_bbox": [0.0, 0.0, 10.0, 10.0],
            "cells": grid(3, 2)}


class ExclusionTests(unittest.TestCase):
    def setUp(self):
        self._env = os.environ.get(fintabnet_split.EXCLUSION_ENV)
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = pathlib.Path(self._tmp.name)

    def tearDown(self):
        if self._env is None:
            os.environ.pop(fintabnet_split.EXCLUSION_ENV, None)
        else:
            os.environ[fintabnet_split.EXCLUSION_ENV] = self._env
        fintabnet_split._hashes.clear()
        self._tmp.cleanup()

    def exclude(self, *stems: str) -> None:
        os.environ[fintabnet_split.EXCLUSION_ENV] = str(
            exclusion_file(self.tmp, *stems))

    def test_a_listed_page_is_refused_and_an_unlisted_one_passes(self):
        self.exclude(LISTED)
        self.assertTrue(fintabnet_split.page_excluded(LISTED))
        self.assertFalse(fintabnet_split.page_excluded(UNLISTED))

    def test_without_a_file_nothing_is_refused(self):
        os.environ[fintabnet_split.EXCLUSION_ENV] = str(
            self.tmp / "no-such-file.json")
        self.assertFalse(fintabnet_split.page_excluded(LISTED))

    def test_the_shipped_list_holds_ten_hashes_and_the_reason(self):
        data = json.loads(
            fintabnet_split.EXCLUSION_FILE.read_text(encoding="utf-8"))
        self.assertEqual(data.get("reason"), REASON)
        hashes = data.get("pages_sha256")
        self.assertIsInstance(hashes, list)
        self.assertEqual(len(hashes), 10)
        self.assertEqual(len(set(hashes)), 10)
        for entry in hashes:
            self.assertIsInstance(entry, str)
            self.assertEqual(len(entry), 64)
            int(entry, 16)

    def test_the_draw_refuses_a_listed_page(self):
        taken: set[str] = set()
        listed_co = ticker("train", taken)
        kept_co = ticker("train", taken)
        listed = f"{listed_co}_2019_page_0"
        kept = f"{kept_co}_2019_page_0"
        self.exclude(listed)
        tarball = self.tmp / "ftn.tar.gz"
        with tarfile.open(tarball, "w:gz") as tar:
            for stem in (listed, kept):
                data = json.dumps([kept_table()]).encode("utf-8")
                info = tarfile.TarInfo(f"{stem}_tables.json")
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        lanes, refused = manifests.collect_ftn(tarball)
        drawn = [page["company"] for page in lanes["train"]]
        self.assertNotIn(listed_co, drawn)
        self.assertIn(kept_co, drawn)
        self.assertEqual(refused.get("the eval-overlap list"), 1)

    def test_the_dump_refuses_a_listed_table(self):
        taken: set[str] = set()
        listed_co = ticker("train", taken)
        kept_co = ticker("train", taken)
        self.exclude(f"{listed_co}_2019_page_3")

        def row(company: str) -> dict:
            return {"source": "fintabnet/ibm-1.0.0",
                    "document": f"{company}_2019", "page": 3,
                    "table": 0, "split": "train",
                    "register": "fintabnet",
                    "licence": "CDLA-Permissive-1.0",
                    "company": company, "year": "2019"}

        pages = dump.Pages(pathlib.Path("."), {}, {}, None, False)
        with self.assertRaises(dump.TableRefused) as caught:
            pages.ftn_table(row(listed_co), [kept_table()])
        self.assertIn("eval-overlap", str(caught.exception))
        self.assertEqual(
            pages.ftn_table(row(kept_co), [kept_table()])["cells"],
            kept_table()["cells"])

    def test_the_dev_fetch_refuses_a_listed_page_before_a_byte_is_read(self):
        taken: set[str] = set()
        listed_co = ticker("dev", taken)
        stem = f"{listed_co}_2019_page_0"
        self.exclude(stem)
        manifest = self.tmp / "dev.json"
        manifest.write_text(json.dumps({"tables": [{
            "source": "fintabnet/ibm-1.0.0",
            "document": f"{listed_co}_2019", "page": 0,
            "table": 0, "split": "dev", "register": "fintabnet",
            "company": listed_co, "year": "2019"}]}))
        with self.assertRaises(SystemExit) as caught:
            dev_manifest.main(["--fetch-ftn-pdfs", str(manifest),
                               str(self.tmp / "out")])
        self.assertIn("eval-overlap", str(caught.exception))
        self.assertIn(stem, str(caught.exception))


if __name__ == "__main__":
    unittest.main()
