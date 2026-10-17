#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The skip lint has to see a bail-out that a macro performs (#239).

WHY THIS TEST EXISTS

`scripts/ci/test_skip_lint.py` decided whether a block was a bail-out by looking
for the word `return`. A macro that returns for you hides that word, so the
whole block was skipped as "not a bail-out" -- not because it announced
anything, but because the lint never looked at it.

That is invisible in the ordinary way: the lint reports "no silent skips" for a
tree it read and for a tree it could not read, in exactly the same words. So the
guard needs its own guard, and the mutation is the historical one: take
`skip_test!` out of the lint's bail-out list and the silent case below stops
being reported.

Both directions are asserted, because only one of them is worth anything on its
own. A lint that flags everything also flags the silent macro.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
LINT = REPO / "scripts" / "ci" / "test_skip_lint.py"

# A macro that announces and returns: the shape the lint must accept.
ANNOUNCING = """
use test_skip::skip_test;

#[test]
fn it_needs_a_host_font() {
    let Some(font) = host_font() else {
        skip_test!("no host font with Unicode coverage found")
    };
    assert!(!font.is_empty());
}
"""


# A silent bail-out: the shape the lint must report, with no macro in sight.
SILENT = """
#[test]
fn it_writes_the_fixtures() {
    let Ok(dir) = std::env::var("PDFA_FIXTURE_DIR") else {
        return;
    };
    std::fs::write(format!("{dir}/one.pdf"), build()).expect("write fixture");
}
"""

# A helper that hands a word back to its caller. Not a bail-out: the caller
# asserts on the word, so something is still being measured. Reading the bare
# word `return` reported this as a silent skip until 10-09-2026 (#510).
A_HELPER_THAT_RETURNS_A_VALUE = """
fn name_of_embedded_face(doc: &Document) -> String {
    let Ok(face) = ttf_parser::Face::parse(&stream.content, 0) else {
        return String::from("<unparseable>");
    };
    face.family()
}

#[test]
fn the_face_is_the_one_that_can_draw_it() {
    assert_eq!(name_of_embedded_face(&doc), "Noto Naskh Arabic");
}
"""


# `return ();` is `return;` with the unit written out. A narrowing that reads
# the explicit unit as a value hands back the silent skip this lint is for.
# Reported by codex on #1865.
SILENT_WITH_AN_EXPLICIT_UNIT = """
#[test]
fn it_writes_the_fixtures() {
    let Ok(dir) = std::env::var("PDFA_FIXTURE_DIR") else {
        return ();
    };
    std::fs::write(format!("{dir}/one.pdf"), build()).expect("write fixture");
}
"""


def run(source: str, mutate: bool = False) -> tuple[int, str]:
    """Run the lint over a tree holding one test file, and report its verdict.

    With `mutate`, run a copy of the lint from which `skip_test!` has been
    removed from ANNOUNCES and nothing else. That copy stands for the macro that
    returns and says nothing, which cannot be written as a fixture: the lint
    recognises a bail-out by name, so any macro it treats as an exit is by
    construction one it also treats as an announcement.

    Reported on the copy and clean on the original is the only pair of answers
    that proves the block was READ. Both halves are needed: drop `skip_test!`
    from the bail-out list and the copy goes clean; drop it from the
    announcement list and the original starts reporting.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        tests = root / "crates" / "a-crate" / "tests"
        tests.mkdir(parents=True)
        (tests / "skipping.rs").write_text(source, encoding="utf-8")

        lint = LINT
        if mutate:
            text = LINT.read_text(encoding="utf-8")
            wanted = ', "skip_test!")'
            if wanted not in text:
                raise AssertionError(
                    "the lint no longer ends its ANNOUNCES tuple with "
                    f"{wanted!r}, so this test could not make the one mutation "
                    "it exists to make -- and a mutation that does not land "
                    "looks exactly like a mutation that survived"
                )
            lint = root / "mutant.py"
            lint.write_text(text.replace(wanted, ")"), encoding="utf-8")

        done = subprocess.run(
            [sys.executable, str(lint), str(root)],
            capture_output=True, text=True, check=False,
        )
        return done.returncode, done.stdout + done.stderr


class TheSkipLintReadsAReturningMacro(unittest.TestCase):
    def test_a_macro_that_returns_in_silence_is_reported(self) -> None:
        code, output = run(ANNOUNCING, mutate=True)
        self.assertEqual(
            code, 1,
            "a bail-out performed by a macro that announces nothing was not "
            f"reported. The lint said:\n{output}",
        )
        self.assertIn("skipping.rs", output)

    def test_a_macro_that_announces_is_accepted(self) -> None:
        code, output = run(ANNOUNCING)
        self.assertEqual(
            code, 0,
            "`skip_test!` announces and returns in one, and the lint reported it "
            f"as a silent skip anyway. The lint said:\n{output}",
        )

    def test_a_bare_silent_return_is_reported(self) -> None:
        """The rule the whole lint is for, with no macro involved.

        The narrowing below asks what a `return` hands back. Without this case
        the narrowing could go all the way to "nothing is a bail-out" and every
        other case here would still pass.
        """
        code, output = run(SILENT)
        self.assertEqual(
            code, 1,
            "a test that returns in silence when its environment variable is "
            f"unset was not reported. The lint said:\n{output}",
        )
        self.assertIn("skipping.rs", output)

    def test_an_explicit_unit_return_is_still_silent(self) -> None:
        """`return ();` ends the test exactly as `return;` does."""
        code, output = run(SILENT_WITH_AN_EXPLICIT_UNIT)
        self.assertEqual(
            code, 1,
            "a test that bails out with `return ();` was not reported. The "
            f"lint said:\n{output}",
        )
        self.assertIn("skipping.rs", output)

    def test_a_helper_that_returns_a_value_is_not_a_bail_out(self) -> None:
        """A `return` that hands a word to the caller does not end a test.

        The caller asserts on the word, so the measurement is still happening
        one frame up. Reading the bare word `return` made the lint report such
        a helper, and a lint that cries wolf gets switched off.
        """
        code, output = run(A_HELPER_THAT_RETURNS_A_VALUE)
        self.assertEqual(
            code, 0,
            "a helper returning a value was reported as a silent skip. The "
            f"lint said:\n{output}",
        )

    def test_the_lint_refuses_a_tree_it_cannot_read(self) -> None:
        """No test files at all is a broken run, not a clean one.

        Without this the two assertions above could both be satisfied by a lint
        that had stopped finding files: one of them wants exit 0, and an empty
        scan gives exit 0 for free.
        """
        with tempfile.TemporaryDirectory() as tmp:
            done = subprocess.run(
                [sys.executable, str(LINT), tmp],
                capture_output=True, text=True, check=False,
            )
        self.assertEqual(done.returncode, 1)
        self.assertIn("not looking at anything", done.stdout + done.stderr)


if __name__ == "__main__":
    unittest.main()
