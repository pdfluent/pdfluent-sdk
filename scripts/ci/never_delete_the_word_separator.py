#!/usr/bin/env python3
"""Nothing may delete character code 32 from a content stream (#210, #182).

`replace_simple_font_code_refs(doc, font_id, 32, None)` removes every
occurrence of code 32 rather than substituting one. Code 32 is the word
separator, so "To improve cooperation" comes out as "Toimprovecooperation".

The output still validates: veraPDF reports it conformant, character retention
stays at 100% because no character was replaced by a wrong one, and the file is
smaller. Every measurement anybody was watching says the conversion improved.

The corpus test that catches this needs the corpus, and announces a skip
without one -- which is most machines. This costs nothing and runs everywhere,
so the two together mean the defect cannot come back quietly.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
BRON = WORTEL / "crates/pdf-manip/src"
# The call shape that deletes rather than substitutes.
PATROON = re.compile(r"replace_simple_font_code_refs\s*\([^)]*?,\s*32\s*,\s*None\s*\)", re.S)


def main() -> int:
    if not BRON.is_dir():
        print(f"FAIL: {BRON.relative_to(WORTEL)} is missing", file=sys.stderr)
        return 1

    treffers = []
    gelezen = 0
    for pad in sorted(BRON.rglob("*.rs")):
        rauw = pad.read_text(errors="replace")
        gelezen += 1
        # Comments name the call on purpose -- the fix is documented by saying
        # what it deliberately does not do. Matching those would make the guard
        # fail on its own explanation.
        tekst = "\n".join(
            r.split("//")[0] if "//" in r else r for r in rauw.splitlines()
        )
        for m in PATROON.finditer(tekst):
            regel = tekst[: m.start()].count("\n") + 1
            treffers.append(f"{pad.relative_to(WORTEL)}:{regel}")

    if gelezen == 0:
        print("FAIL: no Rust sources read; the check cannot have looked.",
              file=sys.stderr)
        return 1

    if treffers:
        print(f"FAIL: {len(treffers)} call(s) delete code 32 instead of substituting "
              "it:", file=sys.stderr)
        for t in treffers:
            print(f"  {t}", file=sys.stderr)
        print("\nCode 32 is the word separator. Removing it produces a file that "
              "validates, retains every character it kept, and is smaller -- while "
              "the words have run together. Pass Some(code) with a substitute, or "
              "leave the stream alone.", file=sys.stderr)
        return 1

    print(f"[word-separator] OK: {gelezen} file(s); nothing deletes code 32.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
