#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""collect_capi_release.sh assembles the judged dir and the tarballs.

From `natives/<platform>/capi/<lib>` for the seven platforms: the capi dir
holds the seven libs under the gate's asset names plus the header (what
`--capi-dir` judges), and one `pdfluent-capi-<version>-<platform>.tar.gz`
per platform lands in the tarballs dir (what the publish uploads). A
missing platform fails naming it. Expected names come from the gate, so
the test holds no list of its own.
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
SCRIPT = HERE / "collect_capi_release.sh"

MINIMUM_CASES = 3  # FLOOR


def _gate():
    spec = importlib.util.spec_from_file_location(
        "capi_gate", HERE / "artifacts_cover_every_platform.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _fixture(root: pathlib.Path, skip: str | None = None) -> pathlib.Path:
    gate = _gate()
    natives = root / "natives"
    for row in gate.PLATFORMS:
        key = row[gate._K]
        if key == skip:
            continue
        slot = natives / key / "capi"
        slot.mkdir(parents=True)
        (slot / ("pdf_capi.dll" if key.startswith("win") else "libpdf_capi.so")).write_bytes(
            b"\x7fELF" + key.encode() + b"\x00" * 64
        )
    return natives


class CollectCapiTests(unittest.TestCase):
    def test_full_set_collects_the_judged_dir_and_seven_tarballs(self):
        gate = _gate()
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            natives = _fixture(root)
            capi, balls = root / "capi", root / "tarballs"
            proc = subprocess.run(
                [
                    "bash", str(SCRIPT),
                    "--natives", str(natives),
                    "--capi-dir", str(capi),
                    "--tarballs", str(balls),
                ],
                capture_output=True, text=True, timeout=180,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual(
                {p.name for p in capi.iterdir()},
                set(gate.CAPI_ASSETS) | {"pdfluent.h"},
            )
            version = re.search(
                r'^version\s*=\s*"([^"]+)"',
                (REPO / "crates" / "pdf-capi" / "Cargo.toml").read_text(),
                re.M,
            ).group(1)
            self.assertEqual(
                {p.name for p in balls.iterdir()},
                {
                    f"pdfluent-capi-{version}-{row[gate._K]}.tar.gz"
                    for row in gate.PLATFORMS
                },
            )

    def test_missing_platform_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            natives = _fixture(root, skip="osx-arm64")
            proc = subprocess.run(
                [
                    "bash", str(SCRIPT),
                    "--natives", str(natives),
                    "--capi-dir", str(root / "capi"),
                    "--tarballs", str(root / "tarballs"),
                ],
                capture_output=True, text=True, timeout=180,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("osx-arm64", proc.stderr + proc.stdout)

    def test_partial_ok_collects_what_exists_and_names_the_skip(self):
        import os

        gate = _gate()
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            natives = _fixture(root, skip="win-x64")
            env = dict(os.environ, PARTIAL_OK="1")
            proc = subprocess.run(
                [
                    "bash", str(SCRIPT),
                    "--natives", str(natives),
                    "--capi-dir", str(root / "capi"),
                    "--tarballs", str(root / "tarballs"),
                ],
                capture_output=True, text=True, timeout=180, env=env,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("win-x64", proc.stdout + proc.stderr)
            self.assertEqual(
                {p.name for p in (root / "capi").iterdir()},
                (set(gate.CAPI_ASSETS) | {"pdfluent.h"}) - {
                    next(a for k, a in
                         ((r[gate._K], r[gate._C]) for r in gate.PLATFORMS)
                         if k == "win-x64")
                },
            )
            self.assertEqual(len(list((root / "tarballs").iterdir())), 6)

    def test_empty_capi_slot_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            natives = _fixture(root)
            victim = natives / "win-x64" / "capi" / "pdf_capi.dll"
            victim.unlink()
            proc = subprocess.run(
                [
                    "bash", str(SCRIPT),
                    "--natives", str(natives),
                    "--capi-dir", str(root / "capi"),
                    "--tarballs", str(root / "tarballs"),
                ],
                capture_output=True, text=True, timeout=180,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("win-x64", proc.stderr + proc.stdout)


if __name__ == "__main__":
    unittest.main()
