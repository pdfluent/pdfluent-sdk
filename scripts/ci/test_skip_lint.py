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
block all satisfy it.

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

ANNOUNCES = ("eprintln!", "println!", "skip(", "panic!", "unreachable!")


def line_of(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def main() -> None:
    offenders: list[tuple[Path, int, str]] = []
    scanned = 0

    for path in sorted(REPO.glob("crates/*/tests/*.rs")):
        text = path.read_text(errors="replace")
        scanned += 1
        for pattern in PATTERNS:
            for m in pattern.finditer(text):
                body = m.group("body")
                if "return" not in body:
                    continue  # not a bail-out
                if any(a in body for a in ANNOUNCES):
                    continue  # says something — fine
                snippet = " ".join(m.group(0).split())[:88]
                offenders.append((path.relative_to(REPO), line_of(text, m.start()), snippet))

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
    print('[test_skip_lint]   eprintln!("SKIPPED (not a pass): no host font available");')
    sys.exit(1)


if __name__ == "__main__":
    main()
