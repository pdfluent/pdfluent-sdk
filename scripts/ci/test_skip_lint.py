#!/usr/bin/env python3
"""Catch tests that skip in silence.

WHY THIS EXISTS

Three times in one week a test passed while measuring nothing:

  * an ASCII sample never reached the Unicode font route it was written for,
  * a line counter read compressed streams without decoding and counted zero,
    so assertions of the form "at most n lines" were satisfied by zero,
  * a test returned early because a host font was missing and reported success.

The first two were caught by insisting the measurement itself be sound. The
third is the one a machine can catch, because it has a recognisable shape: a
test bails out on an environment precondition — a missing binary, font, or
corpus file — and says nothing, so "skipped" and "passed" are indistinguishable
in the output.

A skip is fine. A silent skip is not: it turns an untested code path into a
green tick, which is exactly how the /ToUnicode defect survived a full suite.

WHAT IT ALLOWS

Any early return that announces itself. The convention in this repo is:

    let Some(font) = host_font() else {
        skip("no host font with Unicode coverage found");   // prints to stderr
        return;
    };

`eprintln!`, `println!` or a call to a `skip(...)` helper inside the bailing
block all satisfy it. The shortest form that does, and therefore the one to
reach for, is the `skip_test!` macro from the `test-skip` crate:

    let Some(font) = host_font() else {
        skip_test!("no host font with Unicode coverage found")
    };

That is the whole difference from the silent form: `return` becomes
`skip_test!("reason")`. One word and a reason, on the line that was already
there. It matters that it is not longer: on 25-08-2026 the same mistake was
made twice in two files within hours, and not out of carelessness. The
announcing form was three lines and the silent one was one word, so under time
pressure the silent one won. A rule that costs more than the mistake it prevents
does not survive a busy afternoon.

WHY THERE IS NO PYTHON COUNTERPART

There is nothing to build on that side. `pytest.skip(reason)` puts the test into
a skipped state that the runner reports by construction -- an `s` in the
progress line and a reason in the summary -- so a silent skip is not a shape a
Python test can take. The Rust harness has no skipped state at all: a test that
returns early has passed, as far as libtest is concerned, and that missing
distinction is the whole reason this lint exists.

Exit codes:
    0  no silent skips
    1  silent skips found
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# The two shapes that bail out on a precondition. Deliberately narrow: a broad
# search for `return` in tests matches hundreds of legitimate control-flow
# lines, and a lint that cries wolf gets switched off.
PATTERNS = [
    re.compile(r"let\s+(?:Some|Ok)\s*\([^)]*\)\s*=\s*[^;{]+else\s*\{(?P<body>[^{}]*?)\}", re.S),
    re.compile(r"if\s+[^\n{]*?\.is_none\(\)\s*\{(?P<body>[^{}]*?)\}", re.S),
    re.compile(r"if\s+[^\n{]*?\.is_err\(\)\s*\{(?P<body>[^{}]*?)\}", re.S),
]

# What counts as "the test stops here".
#
# Until 26-08-2026 this was the bare word `return`, and that was a hole: a macro
# that returns for you hides that word, so the lint skipped the block as "not a
# bail-out". `skip_test!` does precisely that, so without this list the lint
# would never have looked at it -- not because it was right, but because it was
# invisible. The next macro that returns in silence would have been just as
# invisible.
BAILS = ("return", "skip_test!")

# What counts as "and it says so".
#
# `skip_test!` from the `test-skip` crate announces and returns in one. It
# exists because the announcing form was three lines and the silent one was one
# word, and under time pressure the short one wins.
ANNOUNCES = ("eprintln!", "println!", "skip(", "panic!", "unreachable!", "skip_test!")


def line_of(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def main() -> None:
    # An optional root, so the lint can be pointed at a fixture tree.
    #
    # Without it the only way to check that this lint still bites was to break a
    # real test file and look -- which nobody does, and which is how the hole
    # below sat open: a macro that returns for you hides the word `return`, so
    # the lint skipped the block as "not a bail-out". It was invisible rather
    # than clean. test_the_skip_lint_reads_a_returning_macro.py builds both
    # shapes in a temporary tree and asserts which one is reported.
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO

    offenders: list[tuple[Path, int, str]] = []
    scanned = 0

    for path in sorted(root.glob("crates/*/tests/*.rs")):
        text = path.read_text(errors="replace")
        scanned += 1
        for pattern in PATTERNS:
            for m in pattern.finditer(text):
                body = m.group("body")
                if not any(b in body for b in BAILS):
                    continue  # not a bail-out
                if any(a in body for a in ANNOUNCES):
                    continue  # says something — fine
                snippet = " ".join(m.group(0).split())[:88]
                offenders.append((path.relative_to(root), line_of(text, m.start()), snippet))

    if scanned == 0:
        print("[test_skip_lint] FATAL: no test files found — the lint is not looking at anything",
              file=sys.stderr)
        sys.exit(1)

    print(f"[test_skip_lint] scanned {scanned} test files")

    if not offenders:
        print("[test_skip_lint] no silent skips")
        sys.exit(0)

    print(f"[test_skip_lint] {len(offenders)} test(s) bail out without saying so:")
    for path, line, snippet in offenders:
        print(f"  {path}:{line}")
        print(f"    {snippet}")
    print()
    print("[test_skip_lint] A skipped test that prints nothing is indistinguishable from")
    print("[test_skip_lint] a passing one. Announce the reason before returning, e.g.")
    print('[test_skip_lint]   skip_test!("no host font available")')
    sys.exit(1)


if __name__ == "__main__":
    main()
