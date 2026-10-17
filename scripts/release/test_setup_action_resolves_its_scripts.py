#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The setup action calls the scripts that are in the tree, by a path that is.

`${{ github.action_path }}/../../scripts/...` resolves one level short --
the action lives in .github/actions/setup-selfhosted, so `../..` lands in
.github, not the root -- and the first real run failed looking for
.github/scripts/release/selfhosted_setup.sh. Both steps resolve from the
repository root instead, and this reads action.yml, resolves every
selfhosted_setup reference the way the runner would, and refuses when one
names a file that is not there.
"""
from __future__ import annotations

import pathlib
import re
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
ACTION = REPO / ".github" / "actions" / "setup-selfhosted" / "action.yml"

# A reference to either setup script, whatever prefixes it: the workspace
# root, the action path with its ../.. hops, or a shape nobody taught this.
# A ${{ ... }} expression counts as one unit: its insides hold spaces, which
# a plain non-space run cannot cross.
REF = re.compile(
    r"""(?P<prefix>(?:\$\{\{[^}]*\}\}|[^\s])*?)selfhosted_setup\.(?P<ext>sh|ps1)"""
)

MINIMUM_REFS = 2  # FLOOR: the .sh leg and the .ps1 leg


def resolve(prefix: str) -> pathlib.Path:
    """The directory the runner looks in for a reference with this prefix."""
    prefix = prefix.strip("\"'")
    for root in ("$GITHUB_WORKSPACE/", "$env:GITHUB_WORKSPACE/"):
        if prefix.startswith(root):
            return REPO / prefix[len(root):]
    if "github.action_path" in prefix:
        tail = prefix.split("}}", 1)[1].lstrip("/")
        return ACTION.parent / tail
    raise AssertionError(f"a setup-script reference this cannot resolve: {prefix!r}")


def references() -> list[tuple[str, pathlib.Path]]:
    doc = yaml.safe_load(ACTION.read_text())
    found = []
    for step in doc["runs"]["steps"]:
        for match in REF.finditer(step.get("run", "")):
            found.append(
                (match.group(0), resolve(match.group("prefix")) / match.group(0).split("/")[-1])
            )
    # Deduplicate: the .sh name rides once per step that calls it.
    seen: dict[str, pathlib.Path] = {}
    for ref, path in found:
        seen.setdefault(ref, path)
    return sorted(seen.items())


class SetupActionTests(unittest.TestCase):
    def test_both_legs_reference_a_setup_script(self):
        refs = references()
        self.assertGreaterEqual(len(refs), MINIMUM_REFS, refs)
        exts = sorted(path.suffix for _, path in refs)
        self.assertEqual(exts, [".ps1", ".sh"], refs)

    def test_every_referenced_script_is_in_the_tree(self):
        for ref, path in references():
            with self.subTest(ref):
                self.assertTrue(
                    path.is_file(),
                    f"{ref} resolves to {path}, which is not in the tree",
                )

    def test_no_reference_hops_off_the_action_path(self):
        # The shape that failed the first run: relative hops off
        # github.action_path miscount the depth and land in .github.
        text = ACTION.read_text()
        self.assertNotIn("github.action_path", text)


if __name__ == "__main__":
    unittest.main()
