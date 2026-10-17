#!/usr/bin/env python3
"""The two generated error files must match what the generator produces (#426).

`docs/error_catalogue.md` and `docs/sdk-error-codes.json` both say they are
generated from `crates/pdfluent/src/error.rs`. Nothing enforced that. #414
hand-edited a row into the markdown, so the file on disk and the file the
generator would write drifted apart, and the next person to run the generator
would have silently deleted that row.

The JSON matters more: it is the input pdfluent.com renders as the anchor list
on /errors, and `scripts/ci/error_codes_have_an_anchor.py` fails when a code
has no anchor there. Together the two guards close the loop that #320, #347
and #426 each found open in turn -- a code in error.rs cannot reach a release
without a section on the page.

Exit codes:
  0 -- both files are exactly what the generator writes
  1 -- one of them is stale or hand-edited
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts/docs/generate_error_catalogue.py"
GENERATED = ("docs/error_catalogue.md", "docs/sdk-error-codes.json")


def main() -> int:
    if not GENERATOR.exists():
        print(f"FAIL: {GENERATOR.relative_to(ROOT)} is missing", file=sys.stderr)
        return 1

    # Regenerate into a throwaway copy of the tree rather than over the real
    # files: a guard that rewrites the working tree to prove it is clean makes
    # every later step read a file the author never wrote.
    with tempfile.TemporaryDirectory(prefix="error-catalogue-") as tmp:
        sandbox = pathlib.Path(tmp)
        (sandbox / "docs").mkdir()
        (sandbox / "scripts/docs").mkdir(parents=True)
        (sandbox / "crates/pdfluent/src").mkdir(parents=True)
        shutil.copy2(GENERATOR, sandbox / "scripts/docs" / GENERATOR.name)
        shutil.copy2(
            ROOT / "crates/pdfluent/src/error.rs",
            sandbox / "crates/pdfluent/src/error.rs",
        )
        run = subprocess.run(
            [sys.executable, str(sandbox / "scripts/docs" / GENERATOR.name)],
            capture_output=True,
            text=True,
        )
        if run.returncode != 0:
            print(
                "FAIL: the generator itself failed:\n" + run.stdout + run.stderr,
                file=sys.stderr,
            )
            return 1

        stale = []
        for relative in GENERATED:
            actual = ROOT / relative
            expected = sandbox / relative
            if not expected.exists():
                print(
                    f"FAIL: the generator no longer writes {relative}. If that is "
                    "deliberate, remove it from GENERATED here and say what "
                    "produces the file now.",
                    file=sys.stderr,
                )
                return 1
            if not actual.exists() or actual.read_bytes() != expected.read_bytes():
                stale.append(relative)

        if stale:
            print(
                "FAIL: these generated files do not match the generator: "
                + ", ".join(stale)
                + ".\nRun: python3 scripts/docs/generate_error_catalogue.py\n"
                "If the difference is content you meant to keep, it belongs in "
                "the generator (HOW_TO_FIX / BINDING_STATUS), not in the output "
                "-- an edit made only in the output is deleted by the next "
                "regeneration.",
                file=sys.stderr,
            )
            return 1

    print(f"[error-catalogue] OK: {len(GENERATED)} generated file(s) are current")
    return 0


if __name__ == "__main__":
    sys.exit(main())
