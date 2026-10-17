#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the binary consumer smoke's archive discovery (#636).

The release archives are deliberately FLAT -- release-binaries.yml cuts the
tarball from the stage's contents, with no top-level directory -- but
smoke_binary.sh located the unpacked tree with `ls -d $TMP/pdfluent-*/`,
which died under `set -e` on exactly that layout in the 1.1.0 build 4
binaries gate (`ls: cannot access '/tmp/tmp.*/pdfluent-*/'`). The smoke now
takes a binary at the top level first and falls back to one top-level
pdfluent-* dir for older archives.

Each case builds a release-shaped tarball holding a shim `pdfluent` (a
shell script answering --version), a LICENSE and a verifying SHA256SUMS,
and runs smoke_binary.sh against it end to end. The flat cases fail on the
old discovery naming the `ls`; the top-dir case passes on both, pinning
the backward-compatible half.
"""
from __future__ import annotations

import hashlib
import io
import pathlib
import platform
import subprocess
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
SMOKE = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_binary.sh"

MINIMUM_CASES = 5  # FLOOR

VERSION = "1.0.0"
FOREIGN_TRIPLE = "x86_64-pc-windows-msvc"


def _host_runnable_triple() -> str | None:
    """A KNOWN_TRIPLES entry this host can exec, or None where there is none.

    Mirrors the smoke's own run_it mapping (uname -s/-m, arm64 is aarch64);
    a Windows host has none, since the smoke never execs there either.
    """
    machine = platform.machine()
    if machine == "arm64":
        machine = "aarch64"
    system = platform.system()
    if system == "Linux" and machine in ("x86_64", "aarch64"):
        return f"{machine}-unknown-linux-gnu"
    if system == "Darwin" and machine in ("x86_64", "aarch64"):
        return f"{machine}-apple-darwin"
    return None


def _shim(reported: str) -> bytes:
    return f"#!/bin/sh\necho \"pdfluent {reported}\"\n".encode()


def _archive(
    root: pathlib.Path,
    triple: str,
    *,
    layout: str = "flat",
    reported: str = VERSION,
    binary_name: str = "pdfluent",
    members: dict[str, bytes] | None = None,
) -> pathlib.Path:
    """A release-shaped tarball: shim binary, LICENSE, verifying SHA256SUMS.

    `layout` is "flat" (the release shape, everything at the top level) or
    "topdir" (the older shape, everything under one pdfluent-* dir).
    `members` replaces the whole payload for the no-binary case.
    """
    archive = root / f"pdfluent-{VERSION}-{triple}.tar.gz"
    if members is None:
        payload = {binary_name: _shim(reported), "LICENSE": b"licence\n"}
        sums = "".join(
            f"{hashlib.sha256(data).hexdigest()}  {name}\n"
            for name, data in payload.items()
        )
        payload["SHA256SUMS"] = sums.encode()
    else:
        payload = members
    prefix = "" if layout == "flat" else f"pdfluent-{VERSION}-{triple}/"
    with tarfile.open(archive, "w:gz") as tf:
        for name, data in payload.items():
            info = tarfile.TarInfo(prefix + name)
            info.size = len(data)
            info.mode = 0o755 if name == binary_name else 0o644
            tf.addfile(info, io.BytesIO(data))
    return archive


def _run_smoke(archive: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(SMOKE), "--artifact", str(archive), "--version", VERSION],
        capture_output=True,
        text=True,
        timeout=120,
    )


class SmokeBinaryLayoutTests(unittest.TestCase):
    def test_flat_archive_passes(self):
        # The release shape, red on the old `ls` discovery.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            triple = _host_runnable_triple() or FOREIGN_TRIPLE
            archive = _archive(root, triple, layout="flat")
            proc = _run_smoke(archive)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("[smoke_binary] PASS", proc.stdout)
            if triple == FOREIGN_TRIPLE:
                self.assertIn("skipping --version", proc.stdout)
            else:
                self.assertIn("version match: OK", proc.stdout)

    def test_top_dir_archive_still_passes(self):
        # The older shape keeps working: backward compatibility, pinned.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            triple = _host_runnable_triple() or FOREIGN_TRIPLE
            archive = _archive(root, triple, layout="topdir")
            proc = _run_smoke(archive)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("[smoke_binary] PASS", proc.stdout)
            if triple == FOREIGN_TRIPLE:
                self.assertIn("skipping --version", proc.stdout)
            else:
                self.assertIn("version match: OK", proc.stdout)

    def test_flat_foreign_archive_passes_without_exec(self):
        # A flat archive for another host: structural checks only, no exec.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archive = _archive(
                root, FOREIGN_TRIPLE, layout="flat", binary_name="pdfluent.exe"
            )
            proc = _run_smoke(archive)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("skipping --version", proc.stdout)
            self.assertIn("[smoke_binary] PASS", proc.stdout)

    def test_version_mismatch_fails(self):
        # The PASS above means something: a lying binary fails, naming it.
        triple = _host_runnable_triple()
        if triple is None:
            self.skipTest("no runnable triple on this host")
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            assert triple is not None
            archive = _archive(root, triple, layout="flat", reported="9.9.9")
            proc = _run_smoke(archive)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("version mismatch", proc.stdout)

    def test_archive_without_a_binary_fails_naming_the_defect(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            triple = _host_runnable_triple() or FOREIGN_TRIPLE
            archive = _archive(
                root, triple, layout="flat", members={"README.md": b"hi\n"}
            )
            proc = _run_smoke(archive)
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("no pdfluent binary at the top level", proc.stdout)


if __name__ == "__main__":
    unittest.main()
