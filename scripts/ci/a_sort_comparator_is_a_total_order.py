#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No sort comparator in the engine decides "equal" by a tolerance (#574).

A comparator that calls two values equal when they lie within some distance
of each other is not transitive -- 10.0 ~ 9.6 and 9.6 ~ 9.2, but not
10.0 ~ 9.2 -- and one that measures the distance by only one side's height is
not even symmetric. Rust's sort panics on such a comparator ("user-provided
comparison function does not correctly implement a total order"), the release
profile makes the panic an abort, and on 28-09-2026 five DocLayNet validation
pages lost their whole page to one in the lattice reader
(crates/pdf-engine/src/learned/tables.rs).

So a `sort_by` / `sort_unstable_by` closure in the engine crates may not hold
`.abs()` or an `if` that compares with `<` or `>`: sort by a total key
(`total_cmp`, `sort_by_key`), then group by the tolerance in a pass after the
sort, which is where a tolerance belongs.

Exit 0 when none does, 1 naming each one.
"""
from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
CRATES = ("crates/pdf-engine/src", "crates/pdf-learned/src")
CALL = re.compile(r"\.(sort_by|sort_unstable_by)\s*\(")
TOLERANCE_IF = re.compile(r"\bif\b[^{]*?(?<![-=<>])[<>](?![=>])|\bif\b[^{]*?[<>]=")


def closures(text: str):
    """(offset, body) of every comparator passed to sort_by / sort_unstable_by."""
    for m in CALL.finditer(text):
        depth, i = 1, m.end()
        while i < len(text) and depth:
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            i += 1
        yield m.start(), text[m.end():i - 1]


def strip_comments(body: str) -> str:
    return "\n".join(line.split("//", 1)[0] for line in body.splitlines())


def offenders(root: pathlib.Path) -> list[str]:
    out = []
    for crate in CRATES:
        for path in sorted((root / crate).rglob("*.rs")):
            text = path.read_text(encoding="utf-8")
            for at, body in closures(text):
                code = strip_comments(body)
                why = ("`.abs()`" if ".abs()" in code
                       else "an `if` comparing with < or >" if TOLERANCE_IF.search(code) else None)
                if why:
                    line = text.count("\n", 0, at) + 1
                    out.append(f"{path.relative_to(root)}:{line}: the comparator holds {why}")
    return out


def main() -> int:
    found = offenders(ROOT)
    if found:
        print(f"[sort-total] FAIL: {len(found)} comparator(s) decide equality by a tolerance:",
              file=sys.stderr)
        for f in found:
            print(f"  {f}", file=sys.stderr)
        print("  Sort by a total key (total_cmp, sort_by_key) and group by the tolerance "
              "after the sort.", file=sys.stderr)
        return 1
    print("[sort-total] OK: every sort comparator in the engine crates is free of tolerances.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
