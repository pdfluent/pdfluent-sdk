#!/usr/bin/env python3
"""The wiring guard is red when a test file runs nowhere, and says which.

Drives the real guard rather than restating it: the helpers are unit-tested, and
the file-level cases run it as a subprocess against the real tree with one file
added and removed again. A guard that is only checked by a paraphrase of itself
proves the paraphrase.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

HIER = pathlib.Path(__file__).resolve().parent
WACHT = HIER / "every_test_is_run.py"
sys.path.insert(0, str(HIER))
import every_test_is_run as wachter  # noqa: E402

fouten: list[str] = []


def geval(wat: str, ok: bool, detail: str = "") -> None:
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fouten.append(wat)


def draai() -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(WACHT)], capture_output=True, text=True)


def main() -> int:
    # 1. The tree as it stands passes. Without this the red cases below prove
    #    only that the guard can fail, not that it tells the two apart.
    schoon = draai()
    geval("the repository as it stands passes", schoon.returncode == 0,
          (schoon.stdout + schoon.stderr)[:300])

    # 2. THE MUTATION. A test file that nothing runs must turn it red BY NAME.
    nieuw = HIER / "test_zzz_deliberately_unwired.py"
    try:
        nieuw.write_text("#!/usr/bin/env python3\nraise SystemExit(0)\n")
        rood = draai()
        geval("an unwired test file turns the guard red", rood.returncode == 1,
              f"exit={rood.returncode}")
        geval("and the failure names the file",
              nieuw.name in (rood.stdout + rood.stderr),
              (rood.stdout + rood.stderr)[:200])
    finally:
        nieuw.unlink(missing_ok=True)

    herstel = draai()
    geval("removing it makes the guard green again", herstel.returncode == 0,
          (herstel.stdout + herstel.stderr)[:200])

    # 3. A MENTION IS NOT A RUN. This is the defect the guard exists to catch,
    #    so it must not be committable by the guard itself: a test named only in
    #    a comment, or in a job `name:`, is not run by anything.
    regels = wachter.uitvoerregels(
        "jobs:\n"
        "  a:\n"
        "    name: python3 scripts/ci/test_named_only.py\n"
        "    steps:\n"
        "      - name: was python3 scripts/ci/test_commented_only.py\n"
        "        run: |\n"
        "          # python3 scripts/ci/test_in_a_comment.py\n"
        "          python3 scripts/ci/test_really_run.py\n")
    blob = "\n".join(wachter.zonder_commentaar(r) for r in "\n".join(regels or []).splitlines())
    geval("a test named in a job `name:` does not count as run",
          not wachter.draait("test_named_only.py", blob))
    geval("a test named in a step `name:` does not count as run",
          not wachter.draait("test_commented_only.py", blob))
    geval("a test in a comment inside `run:` does not count as run",
          not wachter.draait("test_in_a_comment.py", blob))
    geval("a test actually invoked does count", wachter.draait("test_really_run.py", blob))

    # 4. A namesake must not cover the shorter name -- the bug every_guard_has_a_job
    #    already carries a comment about.
    geval("a longer namesake does not cover the shorter file",
          not wachter.draait("infra_health.py", "python3 scripts/ci/test_infra_health.py"))

    # 5. Unparseable YAML is an error, not an empty set of commands.
    geval("YAML that does not parse returns None rather than nothing",
          wachter.uitvoerregels("jobs:\n  a:\n   - [unclosed\n") is None)

    # 6. The floor: an empty glob must not report a clean tree.
    geval("the floor is above zero", wachter.VLOER > 0)

    print(f"\n  {4 + 8} assertion(s) defined, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
