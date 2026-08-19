#!/usr/bin/env python3
"""Do the workspace and pdf-python agree on the crates they share?

WHY THIS EXISTS

crates/pdf-python is deliberately outside the workspace -- it carries its own
[workspace] table, and the root Cargo.toml records the reason: PyO3 needs a
matching Python interpreter, so it is built through maturin rather than by
`cargo build --workspace`. That decision is sound and this check does not
challenge it.

What it does challenge is the consequence nobody chose: a second Cargo.lock
resolving independently. On 2026-08-19 it pinned font-types 0.11.1 and kurbo
0.13.0 where the workspace had 0.11.3 and 0.13.1. So the Python binding -- the
one most of our SDK customers actually use -- was tested against a different
dependency graph from the one we ship everywhere else, and nothing said so.

Separate build, same versions. That is the whole rule.

Exit codes:
    0  the two lockfiles agree on every shared package
    1  they disagree
    2  could not run
"""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# Packages allowed to differ, with the reason. Same principle as
# docs/capability_exceptions.toml: an exception is a decision written down, and a
# short list somebody has to justify beats a rule nobody enforces.
ALLOWED_TO_DIFFER: dict[str, str] = {
    "target-lexicon": (
        "A build dependency of pyo3, which exists nowhere else in the workspace. "
        "pyo3 0.24 needs 0.13.x; the workspace's 0.12.16 arrives via an unrelated "
        "consumer. Genuinely different requirements, not drift."
    ),
}
WORKSPACE_LOCK = REPO / "Cargo.lock"
PYTHON_LOCK = REPO / "crates" / "pdf-python" / "Cargo.lock"


def versions(path: Path) -> dict[str, set[str]]:
    out: dict[str, set[str]] = defaultdict(set)
    for name, ver in re.findall(
        r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"', path.read_text()
    ):
        out[name].add(ver)
    return out


def main() -> None:
    for p in (WORKSPACE_LOCK, PYTHON_LOCK):
        if not p.exists():
            print(f"[lockfile_agreement] FATAL: {p} is missing", file=sys.stderr)
            sys.exit(2)

    ws = versions(WORKSPACE_LOCK)
    py = versions(PYTHON_LOCK)
    shared = sorted(set(ws) & set(py))

    # Only a version pdf-python has and the workspace does NOT counts.
    #
    # Comparing the sets outright called 62 packages a disagreement, most of them
    # because the workspace resolves MORE versions: it contains the desktop app
    # and the CLI, which drag in older majors that pdf-python never sees. A subset
    # is not a conflict, it is a smaller graph. What matters is pdf-python
    # compiling against something nothing else in the repo does -- font-types
    # 0.11.1 where the workspace only has 0.11.3.
    disagreements = [
        (n, sorted(ws[n]), sorted(py[n]))
        for n in shared
        if not py[n] <= ws[n] and n not in ALLOWED_TO_DIFFER
    ]

    excused = sorted(n for n in shared if not py[n] <= ws[n] and n in ALLOWED_TO_DIFFER)

    print(f"[lockfile_agreement] {len(shared)} packages appear in both lockfiles")

    for name in excused:
        print(f"[lockfile_agreement] allowed to differ: {name} "
              f"(workspace {','.join(sorted(ws[name]))}, pdf-python {','.join(sorted(py[name]))})")
        print(f"[lockfile_agreement]   {ALLOWED_TO_DIFFER[name]}")

    if not disagreements:
        print("[lockfile_agreement] every other shared package resolves to the same version(s)")
        sys.exit(0)

    print(f"[lockfile_agreement] FAIL: {len(disagreements)} package(s) differ\n")
    print(f"  {'package':28} {'workspace':26} pdf-python")
    print("  " + "-" * 74)
    for name, w, p in disagreements:
        print(f"  {name:28} {','.join(w):26} {','.join(p)}")
    print()
    print("[lockfile_agreement] The Python binding would be tested against a different")
    print("[lockfile_agreement] dependency graph from the one every other binding ships.")
    print("[lockfile_agreement] Fix with:")
    print("[lockfile_agreement]   cd crates/pdf-python && cargo update")
    print("[lockfile_agreement] and commit the resulting lockfile alongside the root one.")
    sys.exit(1)


if __name__ == "__main__":
    main()
