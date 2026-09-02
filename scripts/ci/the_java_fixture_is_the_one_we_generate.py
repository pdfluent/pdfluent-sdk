#!/usr/bin/env python3
"""The Java binding's test fixture is the file our own generator produces.

`crates/pdf-java/src/test/resources/sample.pdf` was a LiveCycle form from a
corpus we hold but may not redistribute, in a tree that is going public (#310).
It is replaced by a file we generate.

The file is committed, so `mvn test` still runs without a Rust build first. That
only works while the committed bytes and the generator agree, which is what this
checks: run the generator into a temporary file and compare byte for byte.

Nothing is normalised away. A PDF's `/CreationDate`, `/ModDate` and `/ID` are
exactly the fields a writer fills from the clock, so they are pinned in the
generator instead -- a comparison that skips them is a comparison that stops
reading the bytes most likely to drift.
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
FIXTURE = REPO / "crates/pdf-java/src/test/resources/binding-fixture.pdf"
EXAMPLE = "java_binding_fixture"


def main() -> int:
    if not FIXTURE.exists():
        print(f"[java-fixture] FATAL: {FIXTURE.relative_to(REPO)} is missing. The "
              "Java tests read it and this check compares it; neither can happen "
              "over a file that is not there.", file=sys.stderr)
        return 2

    cargo = subprocess.run(["cargo", "--version"], capture_output=True, text=True,
                           check=False)
    if cargo.returncode != 0:
        # Not a skip. Without cargo the generator cannot run, and reporting OK
        # would mean reporting that two things match while having compared one.
        print("[java-fixture] FATAL: cargo is not available, so the generator "
              "could not be run. Refusing to report a match that was never "
              "measured.", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory() as raw:
        opnieuw = Path(raw) / "regenerated.pdf"
        run = subprocess.run(
            ["cargo", "run", "--quiet", "--release", "-p", "pdf-manip",
             "--example", EXAMPLE, "--", str(opnieuw)],
            cwd=REPO, capture_output=True, text=True, check=False,
            env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
        )
        if run.returncode != 0:
            print(f"[java-fixture] FATAL: the generator failed (exit "
                  f"{run.returncode}):\n{run.stderr.strip()[:800]}", file=sys.stderr)
            return 2

        gemaakt = opnieuw.read_bytes()

    gecommit = FIXTURE.read_bytes()
    if gemaakt == gecommit:
        print(f"[java-fixture] OK: {FIXTURE.name} is byte-identical to what "
              f"{EXAMPLE} produces ({len(gecommit)} bytes)")
        return 0

    verschil = next((i for i, (a, b) in enumerate(zip(gemaakt, gecommit)) if a != b),
                    min(len(gemaakt), len(gecommit)))
    print(f"[java-fixture] {FIXTURE.relative_to(REPO)} is not what the generator "
          f"produces.\n"
          f"  committed:   {len(gecommit)} bytes\n"
          f"  regenerated: {len(gemaakt)} bytes\n"
          f"  first difference at byte {verschil}\n\n"
          "  Either the generator changed and the file was not regenerated, or "
          "the file was edited by hand. Regenerate it:\n\n"
          f"    cargo run --release -p pdf-manip --example {EXAMPLE} -- \\\n"
          f"      {FIXTURE.relative_to(REPO)}\n\n"
          "  If the generator's output stopped being deterministic, that is the "
          "bug -- a date or an /ID leaking from the clock -- and normalising it "
          "away here would hide exactly the field that broke.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
