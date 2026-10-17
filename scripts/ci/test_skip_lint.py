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

WHY JAVASCRIPT IS READ, AND .NET AND JAVA ARE NOT

Jest does have a skipped state, but a suite that skips because the native addon
is missing looks the same in a CI log as one that ran: a count of skipped tests
nobody reads, under a green tick. The Node binding's seven suites skipped that
way from the day they were written, and two of them declared a skip and then ran
anyway (#585). So a JavaScript test file that can skip must say so on stderr, in
the same words as Rust: `SKIPPED (not a pass): <reason>`.

It must say so with `process.stderr.write`, not `console.error`. Jest captures
console output per suite and prints none of it for a suite that is entirely
skipped -- measured on the same suite, 7 messages with the raw write and 0 with
`console.error`. A `console.*` announcement is therefore silent in exactly the
run it is for, and the lint treats it as no announcement at all (#588).

The .NET and Java suites were examined for #588 and have no silent shape: they
skip through the runner (`[Fact(Skip = "...")]`, `@Disabled("...")`), which
reports the skip and its reason by construction, and a missing fixture throws.
That is the Python situation, so they stay out of the glob.

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
RETURNING_MACROS = ("skip_test!",)

# ...and only when the `return` hands back nothing.
#
# A test function returns `()`, so its bail-out is always a bare `return;`.
# A helper that returns a value is a different animal: `return
# String::from("<unparseable>")` does not end a test, it hands the caller a
# word to assert on, and the caller is what decides whether anything was
# measured. Until 10-09-2026 the lint read the word `return` alone and reported
# such a helper in
# `a_script_the_latin_substitute_cannot_draw_gets_a_face_that_can.rs` as a
# silent skip (#510). A lint that cries wolf gets switched off, which costs
# more than the one line it flagged.
#
# `return ();` is the same statement spelled out, and a narrowing that reads
# the explicit unit as "hands something back" would let through precisely the
# silent skip this whole file exists for. Reported by codex on #1865 (#510).
RETURNS_NOTHING = re.compile(r"\breturn\s*(?:\(\s*\)\s*)?(?:;|$)", re.M)

# What counts as "and it says so".
#
# `skip_test!` from the `test-skip` crate announces and returns in one. It
# exists because the announcing form was three lines and the silent one was one
# word, and under time pressure the short one wins.
ANNOUNCES = ("eprintln!", "println!", "skip(", "panic!", "unreachable!", "skip_test!")


def bails_out(body: str) -> bool:
    """Does this block end the test, rather than hand a value back?"""
    if any(macro in body for macro in RETURNING_MACROS):
        return True
    return bool(RETURNS_NOTHING.search(body.strip()))


# JavaScript: which files can skip, and what counts as saying so.
#
# Line-wise on purpose. A guard that needs a JavaScript parser is a guard that
# breaks on syntax it has never seen. The shapes are few and recognisable: an
# explicit skip (`describe.skip`, `test.skip`, `it.skip`, the `x` aliases, or
# the `X ? describe : describe.skip` form), or a `require` inside a `try` whose
# `catch` degrades the suite.
JS_GLOBS = (
    "crates/*/tests/*.js",
    "crates/*/tests/*.mjs",
    "crates/*/tests/*.cjs",
    "crates/*/frontend/__tests__/*.js",
)
JS_SKIPS = re.compile(r"\b(?:describe|test|it)\.skip\b|\bx(?:describe|it|test)\s*\(")
# Bounded rather than brace-matched: `({ PdfDocument } = require(...))` puts
# braces inside the try, and a `[^{}]` body would never see that require.
JS_GUARDED_REQUIRE = re.compile(
    r"\btry\s*\{(?:(?!\bcatch\b).){0,400}?\brequire\s*\((?:(?!\bcatch\b).){0,400}?\bcatch\b", re.S
)
JS_ANNOUNCES = re.compile(r"process\.stderr\.write\(\s*[`'\"]SKIPPED \(not a pass\): ")
JS_CONSOLE_ANNOUNCES = re.compile(r"console\.\w+\(\s*[`'\"]SKIPPED \(not a pass\)")
# Where the suite starts: the first describe/test/it, skipped or not, or the
# `X ? describe : describe.skip` choice. An announcement has to come before it
# (codex, #1947): one inside a skipped suite's callback never runs.
JS_SUITE_STARTS = re.compile(
    r"\b(?:describe|test|it)(?:\.skip|\.only)?\s*\(|\?\s*describe\s*:|\bx(?:describe|it|test)\s*\("
)
# A catch that asserts on what it caught is a test of the failure, and one that
# throws fails the run: neither is a suite degrading around it (codex, #1947).
# Only the catch body counts -- an `expect` in a test after it does not.
JS_CATCH_ENDS_IT = re.compile(r"\bexpect\s*\(|\bthrow\b")


def js_block(text: str, open_at: int) -> str:
    """The text of the `{ ... }` block opening at `open_at`, by counting braces.

    A counter, not a parser: braces inside strings can fool it, and the cost of
    that is one wrong verdict on one odd file, which the lint names by line.
    """
    depth = 0
    for i in range(open_at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_at:i + 1]
    return text[open_at:]


def js_catch_ends_it(text: str, catch_at: int) -> bool:
    brace = text.find("{", catch_at)
    return brace != -1 and bool(JS_CATCH_ENDS_IT.search(js_block(text, brace)))


def js_inside_a_function(text: str, index: int) -> bool:
    """Whether `index` sits in a function body: a `{` opened after `function`
    or `=>`, still open. Code there runs only if called."""
    stack: list[bool] = []
    line_start = 0
    for i in range(index):
        c = text[i]
        if c == "\n":
            line_start = i + 1
        elif c == "{":
            before = text[line_start:i]
            stack.append(bool(re.search(r"\bfunction\b[^{}]*$|=>\s*$", before)))
        elif c == "}" and stack:
            stack.pop()
    return any(stack)


def js_code_lines(text: str) -> str:
    """The text with comments blanked, line numbers kept."""
    text = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)
    return re.sub(r"(?m)^(\s*)//.*$", r"\1", text)


def js_can_skip(text: str) -> re.Match[str] | None:
    m = JS_SKIPS.search(text)
    if m is not None:
        return m
    for g in JS_GUARDED_REQUIRE.finditer(text):
        if not js_catch_ends_it(text, g.end() - len("catch")):
            return g
    return None


def js_announces_in_time(text: str) -> bool:
    """An announcement that runs: before the suite starts, and not inside a
    function nobody may call (codex, #1947)."""
    start = JS_SUITE_STARTS.search(text)
    for announce in JS_ANNOUNCES.finditer(text):
        if start is not None and announce.start() >= start.start():
            return False
        if not js_inside_a_function(text, announce.start()):
            return True
    return False


def js_silent_skip(text: str) -> tuple[int, str] | None:
    """The first line where this file can skip, if it never says so on stderr
    before the suite starts."""
    text = js_code_lines(text)
    m = js_can_skip(text)
    if m is None or js_announces_in_time(text):
        return None
    if JS_CONSOLE_ANNOUNCES.search(text):
        why = "announces through console.*, which jest swallows for a skipped suite"
    elif JS_ANNOUNCES.search(text):
        why = "writes SKIPPED (not a pass) only after the suite starts, where a skip never runs it"
    else:
        why = "can skip and never writes SKIPPED (not a pass) to stderr"
    return line_of(text, m.start()), why


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
                if not bails_out(body):
                    continue  # not a bail-out, or not one that ends the test
                if any(a in body for a in ANNOUNCES):
                    continue  # says something — fine
                snippet = " ".join(m.group(0).split())[:88]
                offenders.append((path.relative_to(root), line_of(text, m.start()), snippet))

    for pattern in JS_GLOBS:
        for path in sorted(root.glob(pattern)):
            text = path.read_text(errors="replace")
            scanned += 1
            found = js_silent_skip(text)
            if found is not None:
                offenders.append((path.relative_to(root), found[0], found[1]))

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
    print("[test_skip_lint] and in JavaScript, on stderr and not through console.*:")
    print("[test_skip_lint]   process.stderr.write('SKIPPED (not a pass): native addon not built\\n')")
    sys.exit(1)


if __name__ == "__main__":
    main()
