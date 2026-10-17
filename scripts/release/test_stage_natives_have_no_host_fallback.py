#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The native stagers never stage a host build under a named platform.

Both stagers used to fall back to `$TGT/release/` for linux-x64 and osx-arm64
— and the dotnet one defaulted osx-arm64 to the host path outright. A build
in `$TGT/release/` is the host's arch and libc, so on an Intel Mac that put
an x86 dylib in the osx-arm64 slot and the gate counted files and passed it.
A missing platform must stay missing (PARTIAL_OK=1 for the dev loop), and a
triple-specific build must still stage.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
JAVA_STAGER = REPO / "scripts" / "release" / "stage_java_natives.sh"
DOTNET_STAGER = REPO / "scripts" / "release" / "stage_dotnet_natives.sh"

MINIMUM_CASES = 4  # FLOOR


def _run(script: pathlib.Path, *args: str, target_dir: pathlib.Path) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["PARTIAL_OK"] = "1"
    for key in list(env):
        if key.startswith(("JNI_", "CAPI_")):
            del env[key]
    return subprocess.run(
        ["bash", str(script), *args],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO,
        env=env,
    )


class NoHostFallbackTests(unittest.TestCase):
    def test_java_stager_ignores_a_host_build(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            host = root / "target" / "release"
            host.mkdir(parents=True)
            (host / "libpdfluent_java.dylib").write_bytes(b"host" * 100)
            (host / "libpdf_capi.dylib").write_bytes(b"host" * 100)
            mod = root / "bindings" / "java"
            proc = _run(JAVA_STAGER, str(mod), target_dir=root / "target")
            self.assertNotEqual(proc.returncode, 0, proc.stdout)  # nothing staged
            self.assertFalse(
                (mod / "src" / "main" / "resources" / "native").exists(),
                proc.stdout,
            )

    def test_java_stager_stages_a_triple_specific_build(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            triple = root / "target" / "aarch64-apple-darwin" / "release"
            triple.mkdir(parents=True)
            (triple / "libpdfluent_java.dylib").write_bytes(b"arm" * 100)
            (triple / "libpdf_capi.dylib").write_bytes(b"arm" * 100)
            mod = root / "bindings" / "java"
            proc = _run(JAVA_STAGER, str(mod), target_dir=root / "target")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            staged = mod / "src" / "main" / "resources" / "native"
            self.assertTrue(
                (staged / "osx-arm64" / "libpdfluent_java.dylib").is_file()
            )
            self.assertTrue(
                (staged / "osx-arm64" / "libpdf_capi.dylib").is_file()
            )

    def test_dotnet_stager_ignores_a_host_build(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            host = root / "target" / "release"
            host.mkdir(parents=True)
            (host / "libpdf_capi.dylib").write_bytes(b"host" * 100)
            (host / "libpdf_capi.so").write_bytes(b"host" * 100)
            proj = root / "proj"
            proc = _run(DOTNET_STAGER, str(proj), target_dir=root / "target")
            self.assertNotEqual(proc.returncode, 0, proc.stdout)  # nothing staged
            self.assertFalse((proj / "runtimes").exists(), proc.stdout)

    def test_dotnet_stager_stages_a_triple_specific_build(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            triple = root / "target" / "aarch64-apple-darwin" / "release"
            triple.mkdir(parents=True)
            (triple / "libpdf_capi.dylib").write_bytes(b"arm" * 100)
            proj = root / "proj"
            proc = _run(DOTNET_STAGER, str(proj), target_dir=root / "target")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertTrue(
                (proj / "runtimes" / "osx-arm64" / "native"
                 / "libpdf_capi.dylib").is_file()
            )


if __name__ == "__main__":
    unittest.main()
