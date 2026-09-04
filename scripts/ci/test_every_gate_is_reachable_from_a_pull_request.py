#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the reachability guard must catch, and what it must not.

The case that matters most is the stale exemption. A register that keeps
excusing a gate nobody runs any more, or one that a workflow does reach, reads as
a considered decision while describing nothing -- and that is the failure mode
this whole family of guards exists to refuse.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
GUARD = CI / "every_gate_is_reachable_from_a_pull_request.py"

PR_WORKFLOW = """name: Guards
on:
  pull_request:
    branches: [master]
jobs:
  g:
    runs-on: ubuntu-latest
    steps:
      - run: python3 scripts/ci/alpha.py
"""

PUSH_ONLY_WORKFLOW = """name: Nightly
on:
  push:
    branches: [master]
jobs:
  g:
    runs-on: ubuntu-latest
    steps:
      - run: python3 scripts/ci/beta.py
"""


def tree(tmp: pathlib.Path, *, gate_lines: list[str], workflows: dict[str, str],
         exemptions: str) -> pathlib.Path:
    root = tmp / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / ".github" / "workflows").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)
    shutil.copy(GUARD, root / "scripts" / "ci" / GUARD.name)
    # Above the floor, so the fixture tests behaviour rather than the refusal.
    filler = [f"run f{i:03d}   python3 scripts/ci/f{i:03d}.py" for i in range(45)]
    (root / "scripts" / "ci" / "local_ci_gate.sh").write_text(
        "\n".join(filler + gate_lines) + "\n", encoding="utf-8")
    pr = PR_WORKFLOW + "".join(
        f"      - run: python3 scripts/ci/f{i:03d}.py\n" for i in range(45))
    workflows = {"guards.yml": pr, **workflows}
    for name, body in workflows.items():
        (root / ".github" / "workflows" / name).write_text(body, encoding="utf-8")
    (root / "docs" / "GATES_REACHABLE_FROM_A_PULL_REQUEST.toml").write_text(
        exemptions, encoding="utf-8")
    return root


def run_guard(root: pathlib.Path):
    return subprocess.run([sys.executable, str(root / "scripts" / "ci" / GUARD.name)],
                          capture_output=True, text=True, env=dict(os.environ))


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    ok_all = True
    EMPTY = "# no exemptions\n"
    EXCUSE_BETA = '[[exempt]]\ngate = "beta.py"\nwhy = "needs a token a fork must not have"\n'

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run alpha  python3 scripts/ci/alpha.py"],
                    workflows={}, exemptions=EMPTY)
        u = run_guard(root)
        ok_all &= case("a gate a pull-request workflow reaches is green",
                       u.returncode == 0, (u.stdout + u.stderr)[:220])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run beta   python3 scripts/ci/beta.py"],
                    workflows={"nightly.yml": PUSH_ONLY_WORKFLOW}, exemptions=EMPTY)
        u = run_guard(root)
        ok_all &= case("a gate only a push workflow reaches is red",
                       u.returncode == 1 and "beta.py" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run beta   python3 scripts/ci/beta.py"],
                    workflows={"nightly.yml": PUSH_ONLY_WORKFLOW}, exemptions=EXCUSE_BETA)
        u = run_guard(root)
        ok_all &= case("and green once it is excused with a reason",
                       u.returncode == 0, (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run beta   python3 scripts/ci/beta.py"],
                    workflows={"nightly.yml": PUSH_ONLY_WORKFLOW},
                    exemptions='[[exempt]]\ngate = "beta.py"\nwhy = ""\n')
        u = run_guard(root)
        ok_all &= case("an empty reason is not a reason",
                       u.returncode == 1 and "empty reason" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # THE case: a register that outlives its subject.
    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run alpha  python3 scripts/ci/alpha.py"],
                    workflows={}, exemptions=EXCUSE_BETA)
        u = run_guard(root)
        ok_all &= case("an exemption for a gate nobody runs is red",
                       u.returncode == 1 and "does not run it any more" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run alpha  python3 scripts/ci/alpha.py"],
                    workflows={},
                    exemptions='[[exempt]]\ngate = "alpha.py"\nwhy = "stale"\n')
        u = run_guard(root)
        ok_all &= case("an exemption for a gate that IS reachable is red",
                       u.returncode == 1 and "does reach it" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # The floor: a parse that yields nothing must not read as nothing wrong.
    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=[], workflows={}, exemptions=EMPTY)
        (root / "scripts" / "ci" / "local_ci_gate.sh").write_text("# empty\n",
                                                                  encoding="utf-8")
        u = run_guard(root)
        ok_all &= case("too few gates parsed is SKIPPED, not green",
                       u.returncode == 1 and "SKIPPED (not a pass)" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), gate_lines=["run alpha  python3 scripts/ci/alpha.py"],
                    workflows={}, exemptions=EMPTY)
        (root / "docs" / "GATES_REACHABLE_FROM_A_PULL_REQUEST.toml").unlink()
        u = run_guard(root)
        ok_all &= case("a missing register is SKIPPED, not green",
                       u.returncode != 0 and "SKIPPED (not a pass)" in (u.stdout + u.stderr),
                       (u.stdout + u.stderr)[:250])

    print("test_every_gate_is_reachable_from_a_pull_request: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
