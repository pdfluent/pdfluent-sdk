#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No workflow schedules the retired macos-13 label.

GitHub retired macos-13 on hosted runners: a dispatch naming it sits
queued or fails. osx-x64 builds on macos-latest (arm64) with --target
and smokes on macos-15-intel, so the x86_64 artefact is built anywhere
and RUN on x86_64. This holds the absence everywhere, comments
included: a label that must never be scheduled must never be named.
"""
from __future__ import annotations

import pathlib
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

MINIMUM_CASES = 1  # FLOOR


class RetiredRunnerTests(unittest.TestCase):
    def test_no_workflow_names_macos_13(self):
        offenders = []
        for path in sorted(WORKFLOWS.glob("*.yml")):
            for lineno, line in enumerate(path.read_text().splitlines(), 1):
                if "macos-13" in line:
                    offenders.append(f"{path.name}:{lineno}: {line.strip()}")
        self.assertEqual(offenders, [])


if __name__ == "__main__":
    unittest.main()
