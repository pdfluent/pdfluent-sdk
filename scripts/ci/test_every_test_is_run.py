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
import every_test_is_run as guard_mod  # noqa: E402

failures: list[str] = []
ran = 0


def case(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        failures.append(what)


def run_guard() -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(WACHT)], capture_output=True, text=True)


def main() -> int:
    # 1. The tree as it stands passes. Without this the red cases below prove
    #    only that the guard can fail, not that it tells the two apart.
    clean = run_guard()
    case("the repository as it stands passes", clean.returncode == 0,
          (clean.stdout + clean.stderr)[:300])

    # 2. THE MUTATION. A test file that nothing runs must turn it red BY NAME.
    extra = HIER / "test_zzz_deliberately_unwired.py"
    try:
        extra.write_text("#!/usr/bin/env python3\nraise SystemExit(0)\n")
        red = run_guard()
        case("an unwired test file turns the guard red", red.returncode == 1,
              f"exit={red.returncode}")
        case("and the failure names the file",
              extra.name in (red.stdout + red.stderr),
              (red.stdout + red.stderr)[:200])
    finally:
        extra.unlink(missing_ok=True)

    restored = run_guard()
    case("removing it makes the guard green again", restored.returncode == 0,
          (restored.stdout + restored.stderr)[:200])

    # 3. A MENTION IS NOT A RUN. This is the defect the guard exists to catch,
    #    so it must not be committable by the guard itself: a test named only in
    #    a comment, or in a job `name:`, is not run by anything.
    lines = guard_mod.command_lines(
        "jobs:\n"
        "  a:\n"
        "    name: python3 scripts/ci/test_named_only.py\n"
        "    steps:\n"
        "      - name: was python3 scripts/ci/test_commented_only.py\n"
        "        run: |\n"
        "          # python3 scripts/ci/test_in_a_comment.py\n"
        "          python3 scripts/ci/test_really_run.py\n")
    blob = "\n".join(guard_mod.without_comment(r) for r in "\n".join(lines or []).splitlines())
    case("a test named in a job `name:` does not count as run",
          not guard_mod.runs("test_named_only.py", blob))
    case("a test named in a step `name:` does not count as run",
          not guard_mod.runs("test_commented_only.py", blob))
    case("a test in a comment inside `run:` does not count as run",
          not guard_mod.runs("test_in_a_comment.py", blob))
    case("a test actually invoked does count", guard_mod.runs("test_really_run.py", blob))

    # 4. A namesake must not cover the shorter name -- the bug every_guard_has_a_job
    #    already carries a comment about.
    case("a longer namesake does not cover the shorter file",
          not guard_mod.runs("infra_health.py", "python3 scripts/ci/test_infra_health.py"))

    # 5. Unparseable YAML is an error, not an empty set of commands.
    case("YAML that does not parse returns None rather than nothing",
          guard_mod.command_lines("jobs:\n  a:\n   - [unclosed\n") is None)

    # 6. The floor: an empty glob must not report a clean tree.
    case("the floor is above zero", guard_mod.FLOOR > 0)

    print(f"\n  {ran} assertion(s) ran, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
