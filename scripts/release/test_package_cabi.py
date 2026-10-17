#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""package_cabi.sh cuts one tarball per platform for the release.

Default mode stays host-only (the operator path). `--lib/--plat/--out`
cuts `pdfluent-capi-<version>-<platform>.tar.gz` from a prebuilt lib, with
the same layout the audit expects (single root, include/, lib/, LICENSE,
README.md, VERSION). The platform must be one of the gate's seven keys —
a second platform list that drifts is a wrong release, so it is pinned
against the gate.
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import subprocess
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
SCRIPT = HERE / "package_cabi.sh"

MINIMUM_CASES = 6  # FLOOR


def _gate():
    spec = importlib.util.spec_from_file_location(
        "cabi_gate", HERE / "artifacts_cover_every_platform.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _run(lib: pathlib.Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", str(SCRIPT), "--lib", str(lib), *args],
        capture_output=True,
        text=True,
        timeout=120,
    )


class PackageCabiTests(unittest.TestCase):
    def test_per_platform_tarball_has_the_release_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            lib = root / "libpdf_capi.so"
            lib.write_bytes(b"\x7fELF" + b"\x00" * 100)
            out = root / "out"
            proc = _run(lib, "--plat", "linux-x64", "--out", str(out))
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            balls = sorted(out.glob("pdfluent-capi-*.tar.gz"))
            self.assertEqual(len(balls), 1, proc.stdout + proc.stderr)
            version = re.search(
                r'^version\s*=\s*"([^"]+)"',
                (REPO / "crates" / "pdf-capi" / "Cargo.toml").read_text(),
                re.M,
            ).group(1)
            self.assertEqual(
                balls[0].name, f"pdfluent-capi-{version}-linux-x64.tar.gz"
            )
            with tarfile.open(balls[0], "r:gz") as tf:
                names = tf.getnames()
            top = f"pdfluent-capi-{version}-linux-x64"
            for member in (
                f"{top}/lib/libpdf_capi.so",
                f"{top}/include/pdfluent.h",
                f"{top}/LICENSE",
                f"{top}/LICENSE-COMMERCIAL",
                f"{top}/NOTICE",
                f"{top}/README.md",
                f"{top}/VERSION",
            ):
                self.assertIn(member, names)
            with tarfile.open(balls[0], "r:gz") as tf:
                text = tf.extractfile(f"{top}/VERSION").read().decode()
            self.assertEqual(text.strip(), version)

    def test_a_relative_out_dir_is_the_callers(self):
        # release-natives passes a relative dir (collect_capi_release.sh
        # --tarballs capi-tarballs); tar runs inside the staging dir, so the
        # path must be anchored to the caller's directory first.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            lib = root / "libpdf_capi.so"
            lib.write_bytes(b"\x7fELF" + b"\x00" * 100)
            proc = subprocess.run(
                ["bash", str(SCRIPT), "--lib", str(lib), "--plat", "linux-x64", "--out", "rel-out"],
                capture_output=True,
                text=True,
                timeout=120,
                cwd=root,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual(len(list((root / "rel-out").glob("pdfluent-capi-*.tar.gz"))), 1)

    def test_unknown_platform_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            lib = pathlib.Path(tmp) / "libpdf_capi.so"
            lib.write_bytes(b"\x7fELF")
            proc = _run(
                lib, "--plat", "linux-mips", "--out", str(pathlib.Path(tmp) / "o")
            )
            self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
            self.assertIn("linux-mips", proc.stderr)

    def test_unpaired_lib_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            lib = pathlib.Path(tmp) / "libpdf_capi.so"
            lib.write_bytes(b"\x7fELF")
            proc = _run(lib)
            self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
            self.assertIn("--lib and --plat come as a pair", proc.stderr)

    def test_missing_lib_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            proc = _run(
                pathlib.Path(tmp) / "absent.so",
                "--plat", "linux-x64",
                "--out", str(pathlib.Path(tmp) / "o"),
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)

    def test_platforms_are_the_gate_seven(self):
        body = SCRIPT.read_text()
        ours = re.search(r'^PLATFORMS="([^"]+)"', body, re.M).group(1).split()
        gate = _gate()
        self.assertEqual(ours, [row[0] for row in gate.PLATFORMS])


if __name__ == "__main__":
    unittest.main()
