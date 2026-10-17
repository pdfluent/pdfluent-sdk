#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""build_pdfluent_sdist.sh refuses the maturin that panics on this layout.

maturin 1.12.6 panics with StripPrefixError on the split workspace
(pdf-python is its own workspace under crates/, with path dependencies in
the parent workspace); 1.15.0 is the version measured to build, and the
1.13/1.14 middle was never measured, so the floor is 1.15. The script reads
`maturin --version` and refuses anything older — or unreadable — before
it runs the sdist, so the failure names the version instead of panicking
halfway through the release.

Each case runs the real script against a fake `maturin` on PATH that
reports a canned version and records its invocations: no toolchain, no
network, and the refusal is judged on behaviour (the sdist never runs).
"""
from __future__ import annotations

import os
import pathlib
import shutil
import stat
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "build_pdfluent_sdist.sh"
BASH = shutil.which("bash") or "/bin/bash"

MINIMUM_CASES = 4  # FLOOR

FAKE_MATURIN = """\
#!/bin/sh
# A maturin that reports a canned version and records every other call.
if [ "$1" = "--version" ]; then
  echo "{version}"
  exit 0
fi
echo "$@" >> "$FAKE_MATURIN_CALLS"
exit 0
"""


def _run(version: str | None) -> tuple[subprocess.CompletedProcess, list[str]]:
    """Run the script with a fake maturin reporting `version` on PATH.

    `None` means no maturin at all. Returns the process and the recorded
    non---version invocations.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        bindir = root / "bin"
        bindir.mkdir()
        calls = root / "calls.txt"
        env = dict(os.environ)
        if version is not None:
            fake = bindir / "maturin"
            fake.write_text(FAKE_MATURIN.format(version=version))
            fake.chmod(
                fake.stat().st_mode
                | stat.S_IEXEC
                | stat.S_IXGRP
                | stat.S_IXOTH
            )
            env["PATH"] = f"{bindir}{os.pathsep}{env.get('PATH', '')}"
        else:
            env["PATH"] = str(bindir)
        env["FAKE_MATURIN_CALLS"] = str(calls)
        out = root / "out"
        proc = subprocess.run(
            [BASH, str(SCRIPT), "--out", str(out)],
            capture_output=True,
            text=True,
            timeout=60,
            env=env,
        )
        recorded = (
            calls.read_text().splitlines() if calls.exists() else []
        )
        return proc, recorded


class BuildSdistTests(unittest.TestCase):
    def test_old_maturin_is_refused_before_the_sdist_runs(self):
        proc, recorded = _run("maturin 1.12.6")
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("1.12.6", proc.stderr)
        self.assertIn("1.15", proc.stderr)
        self.assertIn("1.15.0", proc.stderr)
        self.assertEqual(recorded, [], "the sdist must never run")

    def test_unmeasured_middle_is_refused_too(self):
        # 1.13 and 1.14 were never measured on this layout: the floor is
        # the measured version, not the version of the upstream fix.
        for version in ("maturin 1.13.4", "maturin 1.14.0"):
            proc, recorded = _run(version)
            self.assertNotEqual(proc.returncode, 0, version)
            self.assertIn("1.15.0", proc.stderr, version)
            self.assertEqual(recorded, [], "the sdist must never run")

    def test_new_maturin_builds_the_pdf_python_sdist(self):
        proc, recorded = _run("maturin 1.15.0")
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertEqual(len(recorded), 1, recorded)
        self.assertIn("crates/pdf-python/Cargo.toml", recorded[0])
        self.assertIn("--out", recorded[0])

    def test_unreadable_version_is_refused(self):
        proc, recorded = _run("maturin, the wheelwright's friend")
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertEqual(recorded, [], "the sdist must never run")

    def test_missing_maturin_is_refused_by_name(self):
        proc, recorded = _run(None)
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("maturin", proc.stderr)
        self.assertEqual(recorded, [], "the sdist must never run")


if __name__ == "__main__":
    unittest.main()
