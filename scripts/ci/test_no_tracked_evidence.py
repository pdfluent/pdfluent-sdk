#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Does the tracked-evidence guard fail on a planted path, and pass the rest?

Four cases in a fixture repository, each the shape of a real mistake:

  - a tracked `meas/x/notes.md` -- train Q's shape -- fails, and the guard
    names it;
  - the same bytes untracked and ignored pass: the guard judges what the
    repository publishes, which is the index, not the disk;
  - a tracked `meas-out/y.log` fails the same way;
  - a tracked file merely NAMED `meas.md` elsewhere passes: the guard watches
    the two roots, not the word.

And the mutation the suite exists to survive: with the path check removed, the
first case goes green -- which must fail this test. `scan()` takes its
predicate as a parameter for exactly that reason, so the mutant below runs the
real scan over the real fixture with a predicate that refuses nothing, and
asserts green. Were the guard's own predicate ever gutted to that, the first
case would go green and fail the suite instead of passing it; were this test
ever rewritten to pass regardless, the mutant's green assertion is the one
that catches it. Both directions are asserted, in one place.
"""

from __future__ import annotations

import sys as _sys
import pathlib as _pathlib

_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

import subprocess  # noqa: E402
import sys  # noqa: E402
import tempfile  # noqa: E402
from pathlib import Path  # noqa: E402

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from no_tracked_evidence import ROOTS, scan  # noqa: E402


def _git(wd: Path, *args: str) -> subprocess.CompletedProcess:
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a fixture
    # READING the real repository, not WRITING to the real config (#297).
    return subprocess.run(
        ["git", "-C", str(wd), "-c", "core.hooksPath=/nonexistent", *args],
        capture_output=True, text=True, env=sealed_env(cwd=wd),
    )


def _guard(wd: Path) -> subprocess.CompletedProcess:
    # The guard as a program, end to end: exit code and the named paths, not
    # just the scan it is built on. A refusal that names nothing is one nobody
    # can act on.
    return subprocess.run(
        [sys.executable, str(HERE / "no_tracked_evidence.py")],
        cwd=wd, capture_output=True, text=True, env=sealed_env(cwd=wd),
    )


def main() -> int:
    failures: list[str] = []

    # Roots with nothing in them accept everything, and say so in the words of
    # a pass. That state has to be impossible, not merely unlikely.
    if not ROOTS:
        failures.append(
            "the real evidence roots are empty, so the guard accepts everything"
        )
    for root in ROOTS:
        if not root.endswith("/"):
            failures.append(
                f"the root `{root}` names no directory, so it can never match"
            )

    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            print(
                "[test-tracked-evidence] FAIL: "
                "could not create the fixture repository",
                file=sys.stderr,
            )
            return 1

        # The ignore entries the real repository carries, so the ignored half
        # of the fixture means what it means in production.
        (wd / ".gitignore").write_text("meas/\nmeas-out/\n", encoding="utf-8")
        _git(wd, "add", ".gitignore")

        # CASE 1. Tracked evidence under meas/ -- train Q's shape. Force-added:
        # ignored on disk, tracked in the index, which is exactly the state the
        # guard exists to refuse.
        (wd / "meas" / "x").mkdir(parents=True)
        (wd / "meas" / "x" / "notes.md").write_text("# notes\n", encoding="utf-8")
        _git(wd, "add", "-f", "meas/x/notes.md")
        hits = scan(str(wd))
        if hits != ["meas/x/notes.md"]:
            failures.append(
                "a tracked meas/x/notes.md should fail naming it, "
                f"got {hits}"
            )

        # CASE 2. The same bytes untracked and ignored pass: the repository
        # publishes the index, not the disk.
        (wd / "meas" / "y").mkdir(parents=True)
        (wd / "meas" / "y" / "scratch.md").write_text("scratch\n", encoding="utf-8")
        hits = scan(str(wd))
        if hits != ["meas/x/notes.md"]:
            failures.append(
                "an untracked ignored file should pass, "
                f"got {hits}"
            )

        # CASE 3. meas-out/ is the same rule.
        (wd / "meas-out").mkdir(exist_ok=True)
        (wd / "meas-out" / "y.log").write_text("log\n", encoding="utf-8")
        _git(wd, "add", "-f", "meas-out/y.log")
        hits = scan(str(wd))
        if hits != ["meas-out/y.log", "meas/x/notes.md"]:
            failures.append(
                "a tracked meas-out/y.log should fail beside the first, "
                f"got {hits}"
            )

        # CASE 4. A file merely NAMED meas.md elsewhere passes: the guard
        # watches the two roots, not the word.
        (wd / "docs").mkdir(exist_ok=True)
        (wd / "docs" / "meas.md").write_text("about measurements\n", encoding="utf-8")
        _git(wd, "add", "docs/meas.md")
        hits = scan(str(wd))
        if hits != ["meas-out/y.log", "meas/x/notes.md"]:
            failures.append(
                "a tracked docs/meas.md should pass, "
                f"got {hits}"
            )

        # MUTATION. The path check removed: the real scan over the real
        # fixture, with a predicate that refuses nothing. Green is the correct
        # answer for the mutant -- and this assertion is what makes the first
        # case's red load-bearing, because a guard gutted to this mutant turns
        # that case green and fails this test.
        mutant = scan(str(wd), predicate=lambda _p: False)
        if mutant:
            failures.append(
                "with the path check removed the fixture should scan green, "
                f"got {mutant}"
            )
        real = scan(str(wd))
        if real != ["meas-out/y.log", "meas/x/notes.md"]:
            failures.append(
                "the real predicate should still name both paths, "
                f"got {real}"
            )

        # END TO END. The guard as a program: red, naming both paths...
        r = _guard(wd)
        if (r.returncode != 1 or "meas/x/notes.md" not in r.stderr
                or "meas-out/y.log" not in r.stderr):
            failures.append(
                "the guard should exit 1 naming both paths, "
                f"got rc={r.returncode} stderr={r.stderr[-300:]!r}"
            )
        # ...then green once the evidence is untracked but left on disk --
        # the state the production tree is in after the fix.
        _git(wd, "rm", "--cached", "-q", "meas/x/notes.md", "meas-out/y.log")
        r = _guard(wd)
        if r.returncode != 0:
            failures.append(
                "untracked evidence left on disk should exit 0, "
                f"got rc={r.returncode} stderr={r.stderr[-300:]!r}"
            )

    # A git that cannot answer is a refusal, not a pass. A bare directory is
    # not enough to prove it -- git discovers repositories upwards -- so this
    # points at a path that cannot be inside one at all.
    with tempfile.TemporaryDirectory() as tmp:
        try:
            scan(str(Path(tmp) / "does-not-exist"))
            failures.append(
                "a scan git cannot answer reported a clean tree instead of refusing"
            )
        except SystemExit as e:
            if "could not" not in str(e).lower():
                failures.append("failed without git, but without saying so")

    if failures:
        print(
            f"[test-tracked-evidence] FAIL: {len(failures)} case(s):",
            file=sys.stderr,
        )
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        "[test-tracked-evidence] OK: a tracked path under each root fails "
        "naming it, untracked and merely-named files pass, the mutant with "
        "the path check removed scans green, and a git that cannot answer "
        "is refused."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
