#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The provenance table is current, and the check that says so can fail.

`herkomsttabel.py --check` compares docs/HERKOMST.md with the tree. Nothing ran
it, so the committed table drifted. This runs it against the real tree, then
against synthetic trees to prove each way it must refuse: drift, a missing
file, a broken tree walk, and a forked crate that carries our header.

If the `--check` branch of the script is removed, the drift and missing-file
cases here go green-to-red: they assert exit code 1.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "ci" / "herkomsttabel.py"
HEADER = "// Copyright (c) 2026 Innovation Trigger B.V.\n"


def run(root: pathlib.Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args], capture_output=True, text=True,
        check=False, env=dict(os.environ, PDFLUENT_HERKOMST_ROOT=str(root),
                              PYTHONDONTWRITEBYTECODE="1"))


def make_tree(root: pathlib.Path, n: int = 31, forked_with_header: bool = False) -> None:
    (root / "docs").mkdir(parents=True, exist_ok=True)
    for i in range(n):
        crate = root / "crates" / f"own-{i:02d}"
        (crate / "src").mkdir(parents=True)
        (crate / "Cargo.toml").write_text(
            f'[package]\nname = "own-{i:02d}"\nlicense = "AGPL-3.0-or-later"\n')
        (crate / "src" / "lib.rs").write_text(HEADER + "pub fn f() {}\n")
    fork = root / "crates" / "hayro-ccitt"
    (fork / "src").mkdir(parents=True)
    (fork / "Cargo.toml").write_text('[package]\nname = "pdfluent-ccitt"\nlicense = "MIT"\n')
    (fork / "src" / "lib.rs").write_text(
        (HEADER if forked_with_header else "") + "pub fn g() {}\n")


class HerkomstTable(unittest.TestCase):
    def test_committed_table_matches_the_real_tree(self):
        done = run(REPO, "--check")
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_committed_table_names_no_tool(self):
        text = (REPO / "docs" / "HERKOMST.md").read_text().lower()
        for word in ("co-authored", "claude", "anthropic", "assistant", "trailer"):
            self.assertNotIn(word, text)

    def test_committed_table_keeps_the_upstream_facts(self):
        text = (REPO / "docs" / "HERKOMST.md").read_text()
        for needle in ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT", "lopdf",
                       "OpenJPEG", "PDFium Authors", "NOTICE"):
            self.assertIn(needle, text)

    def test_fresh_table_passes_and_a_hand_edit_is_drift(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root)
            self.assertEqual(run(root).returncode, 0)
            self.assertEqual(run(root, "--check").returncode, 0)
            doc = root / "docs" / "HERKOMST.md"
            doc.write_text(doc.read_text() + "\nextra line\n")
            done = run(root, "--check")
            self.assertEqual(done.returncode, 1, "drift was accepted")
            self.assertIn("drifted", done.stderr)

    def test_changed_tree_is_drift(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root)
            run(root)
            (root / "crates" / "own-00" / "src" / "extra.rs").write_text("pub fn h() {}\n")
            self.assertEqual(run(root, "--check").returncode, 1)

    def test_missing_table_fails(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root)
            done = run(root, "--check")
            self.assertEqual(done.returncode, 1, "a missing table was accepted")
            self.assertIn("missing", done.stderr)

    def test_too_few_crates_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root, n=5)
            done = run(root)
            self.assertEqual(done.returncode, 1)
            self.assertIn("FLOOR", done.stderr)

    def test_forked_crate_is_listed_with_its_upstream(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root)
            run(root)
            text = (root / "docs" / "HERKOMST.md").read_text()
            self.assertIn("| `hayro-ccitt` | hayro (Laurenz Stampfl) | Apache-2.0 OR MIT |", text)
            self.assertNotIn("| `hayro-ccitt` | AGPL", text)

    def test_forked_crate_with_our_header_is_flagged(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root, forked_with_header=True)
            done = run(root)
            self.assertIn("WARNING", done.stderr)
            text = (root / "docs" / "HERKOMST.md").read_text()
            self.assertIn("## Attention", text)
            self.assertIn("`hayro-ccitt`", text.split("## Attention")[1])

    def _interpret_tree(self, root, extra=None):
        make_tree(root)
        gen = root / "crates" / "pdf-interpret" / "src" / "font" / "generated"
        gen.mkdir(parents=True)
        (root / "crates" / "pdf-interpret" / "Cargo.toml").write_text(
            '[package]\nname = "pdf-interpret"\nlicense = "MIT"\n')
        (gen / "glyph_names.rs").write_text(HEADER + "pub fn n() {}\n")
        if extra:
            (root / "crates" / "pdf-interpret" / "src" / extra).write_text(
                HEADER + "pub fn x() {}\n")

    def test_allowlisted_generated_file_does_not_warn(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            self._interpret_tree(root)
            done = run(root)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertNotIn("WARNING", done.stderr)
            text = (root / "docs" / "HERKOMST.md").read_text()
            self.assertNotIn("## Attention", text)
            self.assertIn("src/font/generated/glyph_names.rs", text)
            self.assertIn("build_glyph_names.py", text)

    def test_other_file_with_our_header_in_same_crate_still_warns(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            self._interpret_tree(root, extra="new_module.rs")
            done = run(root)
            self.assertIn("WARNING", done.stderr)
            text = (root / "docs" / "HERKOMST.md").read_text()
            self.assertIn("`pdf-interpret`", text.split("## Attention")[1])

    def test_generated_file_in_another_crate_still_warns(self):
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            make_tree(root)
            gen = root / "crates" / "hayro-ccitt" / "src" / "font" / "generated"
            gen.mkdir(parents=True)
            (gen / "glyph_names.rs").write_text(HEADER)
            self.assertIn("WARNING", run(root).stderr)

    def test_table_states_the_author_identities(self):
        text = (REPO / "docs" / "HERKOMST.md").read_text()
        for needle in ("`jasperdew`", "`Jasper de Winter`", "`t <noreply@github.com>`",
                       "`PDFluent <noreply@pdfluent.com>`", "`GATE CI <gate-ci@pdfluent.com>`"):
            self.assertIn(needle, text)


if __name__ == "__main__":
    unittest.main()
