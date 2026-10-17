#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""release-natives.yml smokes the packaged artefacts on all seven platforms.

The smoke matrix platforms equal the gate's PLATFORMS keys — the gate stays
the single source of truth — on the runners the release promises (arm64 on
arm64 hardware, musl inside Alpine). Every leg clean-installs and extracts
through all six channels, and a leg that cannot serve its platform fails
naming it: SMOKE NOT RUN is a failure, never a silent green.
"""
from __future__ import annotations

import importlib.util
import pathlib
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
WORKFLOW = REPO / ".github" / "workflows" / "release-natives.yml"
SMOKE_DIR = HERE / "consumer_smokes"

MINIMUM_CASES = 8  # FLOOR

EXPECTED_RUNNERS = {
    "linux-x64": "ubuntu-latest",
    "linux-arm64": "ubuntu-24.04-arm",
    "linux-x64-musl": "ubuntu-latest",
    "linux-arm64-musl": "ubuntu-24.04-arm",
    "osx-x64": "macos-15-intel",
    "osx-arm64": "macos-latest",
    "win-x64": "windows-latest",
}

# channel smoke -> the extraction call that proves it extracts rather than
# merely importing the package.
SMOKES_EXTRACT = {
    "smoke_python.sh": "extract_markdown",
    "smoke_node.sh": "extractMarkdown",
    "smoke_java.sh": "extractMarkdown",
    "smoke_dotnet.sh": "ExtractMarkdown",
    "smoke_capi.sh": "pdf_document_extract_markdown",
    "smoke_cli.sh": "extract",
}


def _gate():
    spec = importlib.util.spec_from_file_location(
        "smoke_gate", HERE / "artifacts_cover_every_platform.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _doc() -> dict:
    return yaml.safe_load(WORKFLOW.read_text())


def _smoke_job() -> dict:
    return _doc()["jobs"]["smoke"]


def _matrix_rows() -> list:
    return _smoke_job()["strategy"]["matrix"]["include"]


def _runs() -> str:
    return "\n".join(
        str(step.get("run", ""))
        for step in _smoke_job().get("steps", [])
        if isinstance(step, dict)
    )


class SmokeMatrixTests(unittest.TestCase):
    def test_smoke_platforms_equal_the_gate_platforms(self):
        gate = _gate()
        want = [row[gate._K] for row in gate.PLATFORMS]
        got = [row["platform"] for row in _matrix_rows()]
        self.assertEqual(got, want)

    def test_smoke_runners_are_the_promised_ones(self):
        for row in _matrix_rows():
            with self.subTest(platform=row["platform"]):
                self.assertEqual(row["runs-on"], EXPECTED_RUNNERS[row["platform"]])

    def test_arm64_legs_fail_loud_without_an_arm64_runner(self):
        runs = _runs()
        self.assertIn("SMOKE NOT RUN", runs)
        self.assertIn("needs an arm64 runner", runs)
        arm_steps = [
            step
            for step in _smoke_job().get("steps", [])
            if isinstance(step, dict)
            and "arm64" in str(step.get("if", ""))
            and "SMOKE NOT RUN" in str(step.get("run", ""))
        ]
        self.assertTrue(arm_steps, "no arm64-gated step prints SMOKE NOT RUN")

    def test_musl_legs_smoke_inside_alpine(self):
        musl = [row for row in _matrix_rows() if row.get("musl") is True]
        self.assertEqual(
            sorted(row["platform"] for row in musl),
            ["linux-arm64-musl", "linux-x64-musl"],
        )
        runs = _runs()
        self.assertIn("alpine:", runs)
        self.assertIn("docker run", runs)

    def test_every_leg_runs_all_six_channel_smokes(self):
        runs = _runs()
        for smoke in SMOKES_EXTRACT:
            with self.subTest(smoke=smoke):
                self.assertIn(smoke, runs)

    def test_smoke_job_is_the_release_lane(self):
        self.assertEqual(_smoke_job().get("env", {}).get("PDFLUENT_RELEASE_LANE"), "1")

    def test_smoke_job_needs_the_package_job(self):
        self.assertIn("package", _smoke_job().get("needs", []))

    def test_every_channel_smoke_extracts_and_prints_its_first_line(self):
        for smoke, call in SMOKES_EXTRACT.items():
            with self.subTest(smoke=smoke):
                text = (SMOKE_DIR / smoke).read_text()
                self.assertIn(call, text)
                self.assertIn("first line", text.lower())


if __name__ == "__main__":
    unittest.main()
