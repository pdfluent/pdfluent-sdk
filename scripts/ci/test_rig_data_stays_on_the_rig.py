#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Rig pages are refused off the round machine, and nothing else is."""
import json
import os
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import rig_data_stays_on_the_rig as rig

ROOT = pathlib.Path(__file__).resolve().parents[2]


def a_rig_page_name() -> str:
    pages = json.loads((ROOT / "corpus/structured/DOCLAYNET_TEST.json").read_text())["pages"]
    name = pages[0]["name"]
    return name if name.endswith(".pdf") else name + ".pdf"


class RigDataTests(unittest.TestCase):
    def scan(self, files: dict[str, bytes]) -> list[str]:
        with tempfile.TemporaryDirectory() as d:
            for rel, data in files.items():
                p = pathlib.Path(d) / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(data)
            with patch.dict(os.environ, {"RIG_GUARD_PLACES": d, "RIG_GUARD_AS_MAC": "1"}):
                try:
                    rig.refuse_here("the test")
                    return []
                except SystemExit as e:
                    return [str(e)]

    def test_a_rig_page_on_the_mac_is_refused(self):
        self.assertTrue(self.scan({f"scratch/round-pages/{a_rig_page_name()}": b"%PDF"}))

    def test_a_doclaynet_test_shard_is_refused_and_a_validation_one_is_not(self):
        self.assertTrue(self.scan({"hub/datasets--docling-project--DocLayNet-v1.2/test-00000-of-00006.parquet": b""}))
        self.assertFalse(self.scan({"hub/datasets--docling-project--DocLayNet-v1.2/validation-00000-of-00007.parquet": b""}))

    def test_other_pages_are_left_alone(self):
        self.assertFalse(self.scan({"proxy/pages/0123abcd.pdf": b"%PDF", "notes.txt": b""}))

    def test_the_parsebench_tuning_half_may_stay(self):
        pages = json.loads((ROOT / "corpus/structured/PARSEBENCH.json").read_text())["pages"]
        tuning = next(p["name"].rsplit("/", 1)[-1] for p in pages if p.get("split") == "tuning")
        sealed = next(p["name"].rsplit("/", 1)[-1] for p in pages if p.get("split") != "tuning")
        self.assertFalse(self.scan({f"pb-tuning/data/{tuning}": b"%PDF"}))
        self.assertTrue(self.scan({f"elsewhere/{sealed}": b"%PDF"}))

    def test_the_rig_split_is_refused_off_the_round_machine_only(self):
        with patch.dict(os.environ, {"RIG_GUARD_AS_MAC": "1"}):
            with self.assertRaises(SystemExit):
                rig.refuse_split("doclaynet", "test")
            with self.assertRaises(SystemExit):
                rig.refuse_split("pubtables-1m", None)
            with self.assertRaises(SystemExit):
                rig.refuse_split("fintabnet", None)
            rig.refuse_split("doclaynet", "validation")
        with patch.dict(os.environ, {"PDFLUENT_ROUND_MACHINE": "1"}, clear=False):
            os.environ.pop("RIG_GUARD_AS_MAC", None)
            rig.refuse_split("doclaynet", "test")

    def test_it_is_wired_where_the_rig_is_fetched_and_dumped(self):
        for rel in ("scripts/ci/loop/structured_bench.py", "scripts/extraction/learned/dump_doclaynet.py",
                    "scripts/extraction/rig_like_proxy.py"):
            self.assertIn("rig_data_stays_on_the_rig", (ROOT / rel).read_text(), rel)


if __name__ == "__main__":
    unittest.main()
