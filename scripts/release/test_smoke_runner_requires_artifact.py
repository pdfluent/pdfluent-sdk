#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The smokes test exactly the given artefact, and a missing smoke fails.

The runner passes --artifact/--platform, but smoke_python and smoke_java used
to swallow both: the python smoke installed whatever was on PyPI and the java
one auto-detected any jar in the tree. And a channel whose smoke script was
missing passed the runner green. Every case here fails fast, before any
virtualenv or Maven project exists, so these run in seconds with no network.
"""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
RUNNER = REPO / "scripts" / "release" / "post_publish_smoke_runner.sh"
SMOKE_PYTHON = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_python.sh"
SMOKE_JAVA = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_java.sh"
SMOKE_DOTNET = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_dotnet.sh"
SMOKE_RUST = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_rust.sh"
SMOKE_CAPI = REPO / "scripts" / "release" / "consumer_smokes" / "smoke_capi.sh"

MINIMUM_CASES = 7  # FLOOR


def _run(script: pathlib.Path, *args: str, release_lane: bool = False) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    if release_lane:
        env["PDFLUENT_RELEASE_LANE"] = "1"
    else:
        env.pop("PDFLUENT_RELEASE_LANE", None)
    return subprocess.run(
        ["bash", str(script), *args],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO,
        env=env,
    )


class SmokeArtifactTests(unittest.TestCase):
    def test_python_smoke_refuses_a_missing_artifact_by_name(self):
        missing = "/nonexistent/pdfluent-9.9.9-py3-none-any.whl"
        proc = _run(SMOKE_PYTHON, "--artifact", missing)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(missing, proc.stdout + proc.stderr)

    def test_python_smoke_records_the_platform(self):
        # --cross-check: a foreign platform is refused without it, and this
        # case is about the recording, not the refusal.
        proc = _run(
            SMOKE_PYTHON,
            "--artifact", "/nonexistent/x.whl",
            "--platform", "linux-arm64-musl",
            "--cross-check",
        )
        self.assertIn("linux-arm64-musl", proc.stdout + proc.stderr)

    def test_python_smoke_requires_an_artifact_in_the_release_lane(self):
        proc = _run(SMOKE_PYTHON, release_lane=True)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn("--artifact", proc.stdout + proc.stderr)

    def test_java_smoke_refuses_a_missing_artifact_by_name(self):
        missing = "/nonexistent/pdfluent-9.9.9.jar"
        proc = _run(SMOKE_JAVA, "--artifact", missing)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(missing, proc.stdout + proc.stderr)

    def test_java_smoke_requires_an_artifact_in_the_release_lane(self):
        proc = _run(SMOKE_JAVA, release_lane=True)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn("--artifact", proc.stdout + proc.stderr)

    def test_dotnet_smoke_refuses_a_missing_artifact_by_name(self):
        # The runner passes --artifact; the smoke used to know only --nupkg
        # and shifted the runner's flag away, auto-detecting whatever the
        # tree held instead of the nupkg under test.
        missing = "/nonexistent/PDFluent-9.9.9.nupkg"
        proc = _run(SMOKE_DOTNET, "--artifact", missing)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(missing, proc.stdout + proc.stderr)

    def test_dotnet_smoke_requires_an_artifact_in_the_release_lane(self):
        # Auto-detect in the release lane would vouch for any nupkg in the
        # tree instead of the artefact about to ship.
        proc = _run(SMOKE_DOTNET, release_lane=True)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn("--artifact", proc.stdout + proc.stderr)

    def test_capi_smoke_refuses_a_missing_artifact_by_name(self):
        missing = "/nonexistent/pdfluent-c-9.9.9-osx-arm64.tar.gz"
        proc = _run(SMOKE_CAPI, "--artifact", missing)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(missing, proc.stdout + proc.stderr)

    def test_capi_smoke_requires_an_artifact_in_the_release_lane(self):
        proc = _run(SMOKE_CAPI, release_lane=True)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn("--artifact", proc.stdout + proc.stderr)

    def test_rust_smoke_refuses_a_missing_artifact_by_name(self):
        missing = "/nonexistent/pdfluent-9.9.9.crate"
        proc = _run(SMOKE_RUST, "--artifact", missing)
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(missing, proc.stdout + proc.stderr)

    def test_rust_smoke_requires_an_artifact_in_the_release_lane(self):
        # Without --artifact this smoke builds against live crates.io: the
        # same hole the python smoke closed, testing the registry rather
        # than the artefact about to ship.
        proc = _run(SMOKE_RUST, release_lane=True)
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("--artifact", proc.stdout + proc.stderr)

    def test_runner_fails_a_channel_whose_smoke_is_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            scripts = root / "scripts" / "release"
            scripts.mkdir(parents=True)
            shutil.copy2(RUNNER, scripts / RUNNER.name)
            (root / "scripts" / "release" / "consumer_smokes").mkdir(parents=True)
            proc = subprocess.run(
                ["bash", str(scripts / RUNNER.name), "--channel", "node"],
                capture_output=True,
                text=True,
                timeout=120,
                cwd=root,
            )
            self.assertEqual(proc.returncode, 3, proc.stdout + proc.stderr)

    def test_runner_counts_a_missing_smoke_as_a_failure_in_all(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            scripts = root / "scripts" / "release"
            scripts.mkdir(parents=True)
            shutil.copy2(RUNNER, scripts / RUNNER.name)
            (root / "scripts" / "release" / "consumer_smokes").mkdir(parents=True)
            proc = subprocess.run(
                ["bash", str(scripts / RUNNER.name), "--channel", "all"],
                capture_output=True,
                text=True,
                timeout=180,
                cwd=root,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            # Nine channels: rust python wasm dotnet java node binary cli capi.
            self.assertIn("fails=9", proc.stdout)
            summary = next(
                (root / "benchmarks" / "runs" / "post_publish_verify").glob(
                    "*"
                )
            ) / "SUMMARY.md"
            self.assertIn("missing script", summary.read_text())


if __name__ == "__main__":
    unittest.main()
