#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Java sources stay compilable under the pom's source 11 (#636).

The jar targets Java 11 with -Werror, but release-natives.yml only builds it
on dispatch -- so a `record` declaration sat on master until the 1.1.0 build
4 packaging failed on it (`records are not supported in -source 11`, plus
the `'record' may become a restricted type name` warning). This guard is the
push-time half of that compiler gate: it refuses, in every .java file under
bindings/java/src, the constructs source 11 cannot take:

  * the `record` token anywhere in code -- a declaration is an error under
    11, and `record` as an identifier warns, which -Werror promotes;
  * `sealed` / `non-sealed` type declarations and `permits` clauses, which
    need source 17.

Deliberately NOT flagged: `var` (legal since source 10), and `yield`,
`sealed` or `permits` as plain identifiers, which compile without warnings.
Comments and string/char literals never count -- PdfluentDocument's javadoc
says "record" three times about the native stride, and that is fine.

Each case drives the stripper plus the matcher over an inline snippet, and
the last case scans the real tree. Reintroducing `record GridDoubt` fails
that case naming the file and line.
"""
from __future__ import annotations

import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
JAVA_SRC = REPO / "bindings" / "java" / "src"

MINIMUM_CASES = 9  # FLOOR
MIN_JAVA_FILES = 20  # FLOOR: a scan that reads nothing approves everything

_RECORD = re.compile(r"\brecord\b")
_SEALED_DECL = re.compile(r"\b(?:non-sealed\s+|sealed\s+)(?:class|interface|enum)\b")
_PERMITS_CLAUSE = re.compile(r"\bpermits\s+[A-Z]\w*")


def _strip_comments_and_strings(text: str) -> str:
    """Blank comments and literals, keeping newlines so lines still line up."""
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        two = text[i : i + 2]
        three = text[i : i + 3]
        if two == "//":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
        elif two == "/*":
            while i < n and text[i : i + 2] != "*/":
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            out.append("  ")
            i += 2
        elif three == '"""':
            out.append("   ")
            i += 3
            while i < n and text[i : i + 3] != '"""':
                if text[i] == "\\" and i + 1 < n:
                    out.append("  ")
                    i += 2
                else:
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
            out.append("   ")
            i += 3
        elif text[i] == '"':
            out.append(" ")
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\" and i + 1 < n:
                    out.append("  ")
                    i += 2
                else:
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
            out.append(" ")
            i += 1
        elif text[i] == "'":
            out.append(" ")
            i += 1
            while i < n and text[i] != "'":
                if text[i] == "\\" and i + 1 < n:
                    out.append("  ")
                    i += 2
                else:
                    out.append(" " if text[i] != "\n" else "\n")
                    i += 1
            out.append(" ")
            i += 1
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def violations(text: str) -> list[tuple[int, str]]:
    """(line number, line) for every source-11-breaking construct in `text`."""
    stripped = _strip_comments_and_strings(text)
    found: list[tuple[int, str]] = []
    for lineno, line in enumerate(stripped.split("\n"), start=1):
        if (
            _RECORD.search(line)
            or _SEALED_DECL.search(line)
            or _PERMITS_CLAUSE.search(line)
        ):
            found.append((lineno, line.strip()))
    return found


class JavaSource11Tests(unittest.TestCase):
    def test_record_declaration_is_refused(self):
        self.assertEqual(
            [(1, "public record GridDoubt(int thinRows) {}")],
            violations("public record GridDoubt(int thinRows) {}"),
        )

    def test_record_as_identifier_is_refused(self):
        # Not an error on its own, but it warns under source 11 and the
        # pom promotes warnings -- build 4 died on the warning first.
        self.assertTrue(violations("int record = 1;"))

    def test_sealed_declaration_and_permits_clause_are_refused(self):
        self.assertTrue(
            violations("public sealed class Shape permits Circle {}")
        )
        self.assertTrue(violations("final class Circle extends Shape {}") == [])

    def test_var_and_yield_identifiers_are_allowed(self):
        # `var` is source 10; `yield` as a name is a plain identifier.
        # Flagging either would refuse code the compiler accepts.
        self.assertEqual([], violations('var name = "x";'))
        self.assertEqual([], violations("int yield = 0;"))

    def test_comments_are_not_code(self):
        self.assertEqual(
            [], violations("// the old method's record would shift every field")
        )
        self.assertEqual([], violations("/* record */ int x = 1;"))
        self.assertEqual(
            [], violations("/**\n * knows the record is seventeen long.\n */")
        )

    def test_strings_are_not_code(self):
        self.assertEqual([], violations('String s = "record";'))
        self.assertEqual([], violations('String s = "a\\"record\\"b";'))
        self.assertEqual([], violations('String s = """record""";'))
        self.assertEqual([], violations("char c = 'r';"))

    def test_multiline_block_comment_hides_record(self):
        snippet = "/*\n * a record of things\n */\nint x = 1;\n"
        self.assertEqual([], violations(snippet))

    def test_offence_reports_its_line(self):
        snippet = "class A {\n    int ok = 1;\n    int record = 2;\n}\n"
        self.assertEqual([(3, "int record = 2;")], violations(snippet))

    def test_the_tree_is_clean(self):
        files = sorted(JAVA_SRC.rglob("*.java"))
        self.assertGreaterEqual(
            len(files),
            MIN_JAVA_FILES,
            f"only {len(files)} .java files under {JAVA_SRC}: the scan is broken",
        )
        problems = [
            f"{path.relative_to(REPO)}:{lineno}: {line}"
            for path in files
            for lineno, line in violations(path.read_text(encoding="utf-8"))
        ]
        self.assertEqual([], problems)


if __name__ == "__main__":
    unittest.main()
