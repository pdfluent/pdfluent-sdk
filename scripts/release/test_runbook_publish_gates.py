#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every publish section first requires the gate AND the smoke runner.

PyPI, wasm, node and crates.io had no artefact gate and no smoke runner in
the runbook — node was a bare `npm publish` — and NuGet and Maven had only
the gate. Prose rots, so each section below is held by an assertion: the
gate command and the smoke-runner command are present, the bare publish is
gone, and both come before the command that uploads.
"""
from __future__ import annotations

import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
RUNBOOK = REPO / "docs" / "release" / "REGISTRY_1_0_0_RUNBOOK.md"

MINIMUM_CASES = 7  # FLOOR

RUNNER = "post_publish_smoke_runner.sh"


def _code(body: str) -> str:
    """The section's ```bash blocks joined: what the operator runs.

    Prose may name any tool; only the blocks count, so a mention cannot
    stand in for the command.
    """
    return "\n".join(re.findall(r"```bash(.*?)```", body, re.S))


def _sections() -> dict[str, str]:
    text = RUNBOOK.read_text()
    marks = [(m.group(1), m.start()) for m in re.finditer(r"^### (3\.\d) ", text, re.M)]
    out = {}
    for i, (name, start) in enumerate(marks):
        end = marks[i + 1][1] if i + 1 < len(marks) else len(text)
        out[name] = text[start:end]
    return out


class RunbookPublishGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sections = _sections()

    def test_all_six_publish_sections_exist(self):
        for name in ("3.1", "3.2", "3.3", "3.4", "3.5", "3.6"):
            self.assertIn(name, self.sections, f"runbook lost section {name}")

    def test_crates_io_gates_audits_and_smokes_before_live(self):
        code = _code(self.sections["3.1"])
        # The gate audits every crate in order and publishes nothing; the old
        # inline `cargo publish` loop bypassed the audit, the order and the
        # clean-tree check.
        self.assertIn("publish_ordered.sh", code)
        self.assertIn("publish_ordered.sh --live", code)
        self.assertNotIn("cargo publish", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel crates_io", code)
        self.assertLess(code.index(RUNNER), code.index("--live"))

    def test_pypi_gates_and_smokes_every_wheel_before_upload(self):
        code = _code(self.sections["3.2"])
        self.assertIn("build_pdfluent_sdist.sh", code)
        self.assertIn("artifacts_cover_every_platform.py --wheels", code)
        self.assertIn("license_audit.py", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel pypi", code)
        self.assertLess(code.index(RUNNER), code.index("twine upload"))
        self.assertLess(code.index("--wheels"), code.index("twine upload"))

    def test_wasm_gates_and_smokes_before_live(self):
        code = _code(self.sections["3.3"])
        self.assertIn("publish_wasm.sh", code)
        self.assertIn("publish_wasm.sh --live", code)
        self.assertNotIn("npm publish", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel wasm", code)
        self.assertLess(code.index(RUNNER), code.index("--live"))

    def test_node_gates_and_smokes_before_publish_sh(self):
        code = _code(self.sections["3.4"])
        self.assertIn("artifacts_cover_every_platform.py --node-dir", code)
        self.assertIn("guard-prepublish.cjs", code)
        self.assertIn("license_audit.py", code)
        self.assertIn("scripts/release/publish.sh", code)
        self.assertNotIn("npm publish", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel node", code)
        self.assertLess(code.index(RUNNER), code.index("scripts/release/publish.sh"))

    def test_nuget_smokes_before_push(self):
        code = _code(self.sections["3.5"])
        self.assertIn("--nupkg", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel nuget", code)
        self.assertLess(code.index(RUNNER), code.index("nuget push"))

    def test_maven_smokes_before_deploy(self):
        code = _code(self.sections["3.6"])
        self.assertIn("--jar", code)
        self.assertIn(RUNNER, code)
        self.assertIn("--channel maven", code)
        self.assertLess(code.index(RUNNER), code.index("mvn deploy"))


if __name__ == "__main__":
    unittest.main()
