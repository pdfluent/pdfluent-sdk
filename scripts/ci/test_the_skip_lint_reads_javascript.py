#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The skip lint has to read JavaScript, and must not be fooled by console.* (#588).

WHY THIS TEST EXISTS

Until #588 `scripts/ci/test_skip_lint.py` globbed `crates/*/tests/*.rs` only.
The Node binding's seven jest suites skipped in silence without the native
addon, and two declared a skip and then ran anyway; #585 repaired them by hand,
which is not a guard.

Three fixtures pin the rule, and the third is the one that matters: an
announcement through `console.error` must FAIL, because jest prints no console
output for a suite that is entirely skipped. A lint that accepted it would pass
the exact form that is invisible in CI.

The last test runs the lint over a copy of the real Node suites with their
`process.stderr.write` announcements removed. Green on the repository and red on
that copy is the proof that the repaired files are read, not merely present.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
LINT = REPO / "scripts" / "ci" / "test_skip_lint.py"
NODE_TESTS = REPO / "crates" / "pdf-node" / "tests"

ANNOUNCED = """
let mod;
try {
  mod = require('../index');
} catch (e) {
  mod = null;
}
if (!mod) {
  process.stderr.write('SKIPPED (not a pass): native addon not built\\n');
}
const d = mod ? describe : describe.skip;
d('it works', () => { test('opens', () => {}); });
"""

SILENT = """
describe.skip('office export (native module not built)', () => {
  test('placeholder', () => {});
});
"""

# Jest swallows this for a skipped suite: 0 lines printed, measured (#585).
CONSOLE = """
let mod = null;
if (!mod) {
  console.error('SKIPPED (not a pass): native addon not built');
}
const d = mod ? describe : describe.skip;
d('it works', () => { test('opens', () => {}); });
"""

# No `.skip` anywhere: the suite degrades through a guarded require and an
# `if`, which leaves a green file with the real tests quietly gone.
GUARDED_REQUIRE = """
let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  PdfDocument = null;
}
if (PdfDocument) {
  test('opens', () => {});
}
"""

NOT_A_SKIP = """
const { execFileSync } = require('child_process');
test('the guard never writes package.json', () => {
  try {
    execFileSync('node', ['guard.cjs']);
  } catch (_) {
    /* a non-zero exit is still a valid outcome */
  }
});
"""


# (codex, #1947) The announcement is in the file, but inside the skipped
# suite's callback: jest never runs it.
ANNOUNCED_TOO_LATE = """
describe.skip('office export (native module not built)', () => {
  test('placeholder', () => {
    process.stderr.write('SKIPPED (not a pass): native addon not built\\n');
  });
});
"""

# And only in a comment.
ANNOUNCED_IN_A_COMMENT = """
// process.stderr.write('SKIPPED (not a pass): native addon not built\\n');
describe.skip('office export', () => { test('placeholder', () => {}); });
"""

# (codex, #1947) A test of a failing require, asserting on the error: every
# test runs, nothing degrades.
ASSERTS_THE_FAILURE = """
test('a missing module says so', () => {
  try {
    require('./does-not-exist');
  } catch (e) {
    expect(e.code).toBe('MODULE_NOT_FOUND');
  }
});
"""


# (codex, #1947 round two) In a helper nobody calls.
ANNOUNCED_IN_AN_UNCALLED_HELPER = """
function announce() {
  process.stderr.write('SKIPPED (not a pass): native addon not built\\n');
}
describe.skip('office export', () => { test('placeholder', () => {}); });
"""

# The canonical degradation, with an expect after the catch but not in it.
DEGRADES_WITH_AN_EXPECT_LATER = """
let mod;
try {
  mod = require('../index');
} catch (e) {
  mod = null;
}
if (mod) {
  test('opens', () => { expect(mod).toBeTruthy(); });
}
"""

# A catch that adds context and throws: the run fails, nothing degrades.
RETHROWS = """
let mod;
try {
  mod = require('../index');
} catch (e) {
  throw new Error('build the addon first: ' + e.message);
}
test('opens', () => { expect(mod).toBeTruthy(); });
"""


def lint(root: pathlib.Path) -> tuple[int, str]:
    done = subprocess.run(
        [sys.executable, str(LINT), str(root)],
        capture_output=True, text=True, check=False,
    )
    return done.returncode, done.stdout + done.stderr


def run(source: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        tests = root / "crates" / "a-binding" / "tests"
        tests.mkdir(parents=True)
        (tests / "suite.test.js").write_text(source, encoding="utf-8")
        return lint(root)


class TheSkipLintReadsJavaScript(unittest.TestCase):
    def test_an_announced_skip_is_accepted(self) -> None:
        code, output = run(ANNOUNCED)
        self.assertEqual(code, 0, f"an announced skip was reported:\n{output}")
        self.assertIn("scanned 1 test files", output)

    def test_a_silent_describe_skip_is_reported(self) -> None:
        code, output = run(SILENT)
        self.assertEqual(code, 1, f"a bare describe.skip was not reported:\n{output}")
        self.assertIn("suite.test.js", output)

    def test_a_console_announcement_is_reported(self) -> None:
        code, output = run(CONSOLE)
        self.assertEqual(
            code, 1,
            "an announcement through console.error was accepted; jest prints "
            f"nothing of it for a skipped suite. The lint said:\n{output}",
        )
        self.assertIn("console", output)

    def test_a_guarded_require_without_announcement_is_reported(self) -> None:
        code, output = run(GUARDED_REQUIRE)
        self.assertEqual(code, 1, f"a degrading try/require was not reported:\n{output}")

    def test_a_try_without_require_is_not_a_skip(self) -> None:
        code, output = run(NOT_A_SKIP)
        self.assertEqual(code, 0, f"a try/catch around a child process was reported:\n{output}")

    def test_an_announcement_inside_the_skipped_suite_is_reported(self) -> None:
        code, output = run(ANNOUNCED_TOO_LATE)
        self.assertEqual(code, 1, f"an announcement jest never runs was accepted:\n{output}")
        self.assertIn("after the suite starts", output)

    def test_an_announcement_in_a_comment_is_reported(self) -> None:
        code, output = run(ANNOUNCED_IN_A_COMMENT)
        self.assertEqual(code, 1, f"a commented-out announcement was accepted:\n{output}")

    def test_a_catch_that_asserts_is_not_a_skip(self) -> None:
        code, output = run(ASSERTS_THE_FAILURE)
        self.assertEqual(code, 0, f"a test asserting on a failed require was reported:\n{output}")

    def test_an_announcement_in_an_uncalled_helper_is_reported(self) -> None:
        code, output = run(ANNOUNCED_IN_AN_UNCALLED_HELPER)
        self.assertEqual(code, 1, f"an announcement in an uncalled function was accepted:\n{output}")

    def test_an_expect_after_the_catch_does_not_excuse_it(self) -> None:
        code, output = run(DEGRADES_WITH_AN_EXPECT_LATER)
        self.assertEqual(code, 1, f"a degrading catch passed on a later expect:\n{output}")

    def test_a_catch_that_rethrows_is_not_a_skip(self) -> None:
        code, output = run(RETHROWS)
        self.assertEqual(code, 0, f"a rethrowing catch was reported:\n{output}")

    def test_the_repaired_node_suites_stay_repaired(self) -> None:
        announce = re.compile(r"^.*process\.stderr\.write\('SKIPPED \(not a pass\).*\n", re.M)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            tests = root / "crates" / "pdf-node" / "tests"
            shutil.copytree(NODE_TESTS, tests)
            code, output = lint(root)
            self.assertEqual(code, 0, f"the real Node suites are not clean:\n{output}")

            stripped = 0
            for path in tests.glob("*.js"):
                text = path.read_text(encoding="utf-8")
                new, n = announce.subn("", text)
                stripped += n
                path.write_text(new, encoding="utf-8")
            self.assertGreaterEqual(stripped, 7, "fewer announcements than #585 landed")
            code, output = lint(root)
            self.assertEqual(code, 1, f"stripping every announcement stayed green:\n{output}")
            self.assertGreaterEqual(output.count("crates/pdf-node/tests/"), 7, output)


if __name__ == "__main__":
    unittest.main()
