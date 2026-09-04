#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
"""Every public diagnostic code has a row in the catalogue, and no row outlives its code.

WHY THIS GUARD EXISTS

A diagnostic is how the SDK reports that a call succeeded and still lost
something -- a page that rendered with paint missing, an image that was
dropped, a page order that was reconstructed. A caller who does not know the
code exists cannot act on it, and until #324 nothing outside the source listed
them: the site documents only the `Error` variants, which are a different
taxonomy.

WHY IT COMPARES IN BOTH DIRECTIONS

A missing row is the obvious failure. A stale row is the quieter one: a
catalogue that only ever grows describes a product that no longer exists, and
the reader cannot tell which half is current. Both directions fail here.

WHY IT READS THE CONSTANTS AND NOT THE STRINGS

The codes appear twice in the source -- as `CODE_*` constants and as literals in
`from_leniency_event`. This reads the constants, because those are what a caller
matches on (`Diagnostic::CODE_NESTING_TOO_DEEP`) and what the append-only
promise is about. A literal that has no constant is a separate defect and not
this guard's business.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


def repository_root() -> Path:
    """The repository this runs IN, not the one this file lives in.

    A guard that derives its target from its own location always inspects its
    own checkout: it cannot be pointed at a fixture, so it cannot be tested.
    """
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=False,
    )
    if out.returncode != 0 or not out.stdout.strip():
        print("[diagnostics] SKIPPED (not a pass): not inside a git repository",
              file=sys.stderr)
        raise SystemExit(1)
    return Path(out.stdout.strip())


CODE_CONSTANT = re.compile(r'CODE_[A-Z_0-9]+\s*:\s*&\'static str\s*=\s*"([A-Z_0-9]+)"')
# A table row: `| `CODE` | ...`. The backticks are required, so prose mentioning
# a code in a sentence does not count as documenting it.
TABLE_ROW = re.compile(r'^\|\s*`([A-Z_0-9]+)`\s*\|', re.M)


def main() -> int:
    root = repository_root()
    source = root / "crates" / "pdfluent" / "src" / "diagnostics.rs"
    catalogue = root / "docs" / "diagnostic_catalogue.md"

    for path in (source, catalogue):
        if not path.is_file():
            print(f"[diagnostics] SKIPPED (not a pass): {path} is missing, so "
                  "nothing was compared.", file=sys.stderr)
            return 1

    in_code = set(CODE_CONSTANT.findall(source.read_text()))
    in_docs = set(TABLE_ROW.findall(catalogue.read_text()))

    if not in_code:
        print("[diagnostics] SKIPPED (not a pass): no CODE_ constants matched in "
              f"{source.name}. An empty set compares equal to anything, which "
              "would pass this guard over nothing at all.", file=sys.stderr)
        return 1

    undocumented = sorted(in_code - in_docs)
    stale = sorted(in_docs - in_code)

    if undocumented:
        print(f"[diagnostics] {len(undocumented)} code(s) have no row in "
              "docs/diagnostic_catalogue.md:\n", file=sys.stderr)
        for code in undocumented:
            print(f"  - {code}", file=sys.stderr)
        print("\n  A diagnostic nobody can look up is a silent degradation the "
              "caller cannot act on.", file=sys.stderr)

    if stale:
        print(f"\n[diagnostics] {len(stale)} row(s) describe a code that no "
              "longer exists:\n", file=sys.stderr)
        for code in stale:
            print(f"  - {code}", file=sys.stderr)
        print("\n  Remove the row. A catalogue that only grows describes a "
              "product that is no longer shipped.", file=sys.stderr)

    if undocumented or stale:
        return 1

    print(f"[diagnostics] OK: {len(in_code)} diagnostic code(s), each with a row "
          "and no row without a code")
    return 0


if __name__ == "__main__":
    sys.exit(main())
