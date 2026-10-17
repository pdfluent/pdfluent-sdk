#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A red path audit names its finding on its last line (#636).

Both readers of the snapshot log read its edges: the gate prints the last
fifteen lines, and the failed-gates block the first lines saying fail. The
check's verdict named neither a failure nor a file, so a red audit reported
only the assembly lines that came before it -- the train log showed three
`[simulate]` lines and no finding. The check repeats the failing check with
its count on the last line; these cases plant a leak in a fixture tree and
hold that promise, plus the clean tree that must still pass.

The leak is assembled, never quoted: this file ships in the published tree,
so a literal machine path in it would be the audit's next finding.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
CHECK = pathlib.Path(
    os.environ.get(
        "PUBLIC_SNAPSHOT_CHECK_UNDER_TEST",
        REPO / "scripts" / "release" / "public_snapshot_check.sh",
    )
)

# A machine path, in two halves: the audit's pattern only matches whole
# bytes, so the joined leak trips the check while neither half trips the
# audit on this file.
LEAK = "/home/" + "audit-fixture-user/secret-plans.md"


def _run(tree: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(CHECK), "--audit-only", str(tree)],
        capture_output=True,
        text=True,
        timeout=120,
    )


class PublicSnapshotCheckTests(unittest.TestCase):
    def test_failure_names_its_finding_on_the_last_line(self):
        with tempfile.TemporaryDirectory(prefix="snapshot-check") as tmp:
            tree = pathlib.Path(tmp)
            (tree / "clean.txt").write_text("nothing to see here\n")
            (tree / "leak.txt").write_text(f"see {LEAK} for details\n")
            proc = _run(tree)
        self.assertNotEqual(
            proc.returncode, 0, "a planted machine path must fail the audit"
        )
        output = proc.stdout + proc.stderr
        last = [line for line in output.splitlines() if line.strip()][-1]
        self.assertIn(
            "FAIL",
            last,
            f"the last line must read as a failure so the failed-gates "
            f"block picks it up, was: {last!r}",
        )
        self.assertIn(
            "path audit",
            last,
            f"the last line must repeat the failing check, was: {last!r}",
        )

    def test_clean_tree_still_passes(self):
        with tempfile.TemporaryDirectory(prefix="snapshot-check") as tmp:
            tree = pathlib.Path(tmp)
            (tree / "clean.txt").write_text("nothing to see here\n")
            proc = _run(tree)
        self.assertEqual(
            proc.returncode, 0, proc.stdout + proc.stderr
        )
        self.assertIn("OK:", proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
