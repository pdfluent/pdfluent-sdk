#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the Homebrew formula renderer (T6 §2).

A complete SHA256SUMS renders a formula naming all four archives with their
digests plus the --version and extract-on-fixture tests; a fixture with three
of the four turns the renderer red with the missing platform's name.
"""
from __future__ import annotations

import hashlib
import importlib.util
import os
import pathlib
import stat
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "render_homebrew_formula", HERE / "render_homebrew_formula.py"
)
renderer = importlib.util.module_from_spec(_spec)
sys.modules["render_homebrew_formula"] = renderer
_spec.loader.exec_module(renderer)

MINIMUM_CASES = 10  # FLOOR

VERSION = "1.0.0"
TRIPLES = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
)


def _sums(drop: str | None = None) -> dict[str, str]:
    out = {}
    for triple in TRIPLES:
        if triple == drop:
            continue
        name = f"pdfluent-{VERSION}-{triple}.tar.gz"
        out[name] = hashlib.sha256(name.encode()).hexdigest()
    return out


def _write_sums_file(root: pathlib.Path, sums: dict[str, str]) -> pathlib.Path:
    path = root / "SHA256SUMS"
    path.write_text("".join(f"{d}  {n}\n" for n, d in sorted(sums.items())))
    return path


class RenderTests(unittest.TestCase):
    def test_renderer_targets_match_the_gate_table(self):
        # One table: the renderer serves exactly the triples the artefact
        # gate requires for the Homebrew platforms.
        _gspec = importlib.util.spec_from_file_location(
            "gate_for_render", HERE / "artifacts_cover_every_platform.py"
        )
        _gate = importlib.util.module_from_spec(_gspec)
        sys.modules["gate_for_render"] = _gate
        _gspec.loader.exec_module(_gate)
        triple_of = {plat: triple for plat, triple, _ in _gate.CLI_PLATFORMS}
        self.assertEqual(
            sorted(t for _, _, t in renderer.HOMEBREW_TARGETS),
            sorted(triple_of[p] for p in _gate.HOMEBREW_PLATFORMS),
        )

    def test_complete_sums_render_all_four_archives(self):
        sums = _sums()
        text = renderer.render(VERSION, sums, "pdfluent/engine")
        for triple in TRIPLES:
            name = f"pdfluent-{VERSION}-{triple}.tar.gz"
            self.assertIn(name, text)
            self.assertIn(sums[name], text)

    def test_formula_has_os_arch_blocks_and_install(self):
        text = renderer.render(VERSION, _sums(), "pdfluent/engine")
        for block in ("on_macos do", "on_linux do", "on_arm do", "on_intel do"):
            self.assertIn(block, text)
        self.assertIn('bin.install "pdfluent"', text)

    def test_formula_installs_the_man_pages(self):
        text = renderer.render(VERSION, _sums(), "pdfluent/engine")
        self.assertIn('man1.install Dir["man/man1/*.1"]', text)

    def test_formula_tests_version_and_extract_on_fixture(self):
        text = renderer.render(VERSION, _sums(), "pdfluent/engine")
        self.assertIn('system bin/"pdfluent", "--version"', text)
        self.assertIn("brew_fixture.pdf", text)
        self.assertIn("extract", text)
        self.assertIn("--format markdown", text)
        self.assertIn(renderer.FIXTURE_SENTENCE, text)

    def test_fixture_pdf_is_wellformed_and_carries_the_sentence(self):
        pdf = renderer.FIXTURE_PDF
        self.assertTrue(pdf.startswith("%PDF-"))
        self.assertIn("%%EOF", pdf)
        self.assertIn(f"({renderer.FIXTURE_SENTENCE})", pdf)
        # No trailing whitespace: editors strip it, which would silently
        # corrupt an embedded binary fixture.
        for line in pdf.splitlines():
            self.assertEqual(line, line.rstrip(), repr(line))

    def test_three_of_four_refuses_with_the_missing_platform_name(self):
        sums = _sums(drop="aarch64-unknown-linux-gnu")
        with self.assertRaises(renderer.MissingPlatform) as ctx:
            renderer.render(VERSION, sums, "pdfluent/engine")
        self.assertIn("linux-arm64", str(ctx.exception))

    def test_main_refuses_with_exit_1_and_names_the_platform(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums_path = _write_sums_file(
                root, _sums(drop="x86_64-apple-darwin")
            )
            rc = renderer.main(
                ["--version", VERSION, "--sums", str(sums_path),
                 "--out", str(root / "pdfluent.rb")]
            )
            self.assertEqual(rc, 1)
            self.assertFalse((root / "pdfluent.rb").exists())

    def test_main_writes_the_formula(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums_path = _write_sums_file(root, _sums())
            out = root / "pdfluent.rb"
            self.assertEqual(
                renderer.main(
                    ["--version", VERSION, "--sums", str(sums_path),
                     "--out", str(out)]
                ),
                0,
            )
            self.assertIn("class Pdfluent < Formula", out.read_text())

    def test_brew_audit_without_brew_is_exit_2_not_a_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            formula = root / "pdfluent.rb"
            formula.write_text("class Pdfluent < Formula\nend\n")
            env = dict(os.environ, PATH=str(root))  # empty dir: no brew
            old_path = os.environ.get("PATH")
            os.environ["PATH"] = str(root)
            try:
                self.assertEqual(renderer.run_brew_audit(formula), 2)
            finally:
                if old_path is None:
                    del os.environ["PATH"]
                else:
                    os.environ["PATH"] = old_path

    def test_brew_audit_invokes_brew_audit_strict(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            calls = root / "calls.txt"
            fake = root / "brew"
            fake.write_text(
                "#!/bin/sh\n"
                f'echo "$@" > "{calls}"\n'
                "exit 0\n"
            )
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
            formula = root / "pdfluent.rb"
            formula.write_text("class Pdfluent < Formula\nend\n")
            old_path = os.environ.get("PATH", "")
            os.environ["PATH"] = f"{root}{os.pathsep}{old_path}"
            try:
                self.assertEqual(renderer.run_brew_audit(formula), 0)
            finally:
                os.environ["PATH"] = old_path
            self.assertEqual(calls.read_text().strip(),
                             f"audit --strict {formula}")


if __name__ == "__main__":
    unittest.main()
