#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
"""The catalogue guard bites, in both directions, on a fixture repository.

A guard tested only against the real tree passes for as long as the tree
happens to be correct, which is exactly when a guard is least useful. These
cases build the failure instead of describing it.
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

GUARD = Path(__file__).resolve().parent / "every_diagnostic_code_is_documented.py"


CASES: list[tuple[str, bool]] = []


def case(name: str, ok: bool, detail: str = "") -> None:
    CASES.append((name, ok))
    print(f"  {'ok  ' if ok else 'FAIL'}  {name}" + (f"  -- {detail}" if not ok else ""))


def build(root: Path, codes: list[str], rows: list[str]) -> None:
    src = root / "crates" / "pdfluent" / "src"
    src.mkdir(parents=True, exist_ok=True)
    body = "impl Diagnostic {\n" + "".join(
        f'    pub const CODE_{c}: &\'static str = "{c}";\n' for c in codes
    ) + "}\n"
    (src / "diagnostics.rs").write_text(body)

    docs = root / "docs"
    docs.mkdir(parents=True, exist_ok=True)
    table = "| Code | Severity |\n|---|---|\n" + "".join(
        f"| `{r}` | Warning |\n" for r in rows
    )
    (docs / "diagnostic_catalogue.md").write_text("# Fixture\n\n" + table)


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(GUARD)], cwd=root,
                          capture_output=True, text=True,
                          env=sealed_env(identity=True, cwd=root))


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        root.mkdir()
        subprocess.run(["git", "init", "-q", str(root)], check=True,
                       env=sealed_env(identity=True, cwd=root))

        build(root, ["ALPHA", "BETA"], ["ALPHA", "BETA"])
        r = run(root)
        case("a code with a row passes", r.returncode == 0, r.stdout + r.stderr)

        build(root, ["ALPHA", "BETA"], ["ALPHA"])
        r = run(root)
        case("an undocumented code fails", r.returncode == 1)
        case("and the failure names it", "BETA" in (r.stdout + r.stderr))

        build(root, ["ALPHA"], ["ALPHA", "GAMMA"])
        r = run(root)
        case("a row without a code fails", r.returncode == 1)
        case("and the failure names it", "GAMMA" in (r.stdout + r.stderr))

        # The empty-set trap: an empty left side compares equal to an empty
        # right side, so a guard that stopped matching would pass over nothing.
        build(root, [], [])
        r = run(root)
        case("matching nothing is not a pass", r.returncode == 1,
             f"exit={r.returncode}")

        # A code mentioned in prose but absent from the table is not documented:
        # a reader cannot look it up in a list it is not in.
        build(root, ["ALPHA", "BETA"], ["ALPHA"])
        cat = root / "docs" / "diagnostic_catalogue.md"
        cat.write_text(cat.read_text() + "\nBETA is mentioned only in this sentence.\n")
        r = run(root)
        case("a code named only in prose does not count", r.returncode == 1)

    failed = [n for n, ok in CASES if not ok]
    print(f"\n  {len(CASES)} assertion(s) ran, {len(failed)} failure(s)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
