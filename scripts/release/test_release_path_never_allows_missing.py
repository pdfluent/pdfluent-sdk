#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The release path never forgives an unbuilt platform.

`--allow-missing` is the dev loop's flag: build-lane.yml and
build_all_platforms.sh may pass it, and do. The release workflows —
release-natives.yml, release-binaries.yml, node-bindings.yml — must not
contain it anywhere, not even in a comment: a forgiven platform in the
release is a silently incomplete channel. The set of release workflows
is pinned too, so a new workflow that judges artefacts joins the rule
deliberately.
"""
from __future__ import annotations

import pathlib
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
WORKFLOWS = REPO / ".github" / "workflows"

MINIMUM_CASES = 3  # FLOOR

RELEASE_WORKFLOWS = (
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
)


def _gate_invoking_workflows() -> set[str]:
    """Workflows that run the artefact gate (not its tests)."""
    found = set()
    for path in sorted(WORKFLOWS.glob("*.yml")):
        if "artifacts_cover_every_platform.py --" in path.read_text():
            found.add(path.name)
    return found


class ReleasePathTests(unittest.TestCase):
    def test_the_release_workflows_are_exactly_those_three(self):
        self.assertEqual(_gate_invoking_workflows(), set(RELEASE_WORKFLOWS))

    def test_no_release_workflow_allows_missing(self):
        for name in RELEASE_WORKFLOWS:
            text = (WORKFLOWS / name).read_text()
            self.assertNotIn(
                "--allow-missing", text, f"{name} forgives unbuilt platforms"
            )

    def test_the_lane_documents_that_it_is_not_the_release_path(self):
        text = (WORKFLOWS / "build-lane.yml").read_text()
        self.assertIn("NOT the release path", text)
        self.assertIn("release-natives.yml", text)


if __name__ == "__main__":
    unittest.main()
