#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A workflow that runs a script which is not there is caught (#358).

`ci_config_lint.py` has checked this for `.gitlab-ci.yml` since a release job
was found calling a script that had never been committed. Nothing checked the
GitHub workflows, and on this guard's first run one of them was doing exactly
that -- behind a secret that is never set, so it had never failed and nobody
had ever found out.

The cases that matter are the ones that make a guard like this wrong rather
than absent: a path relative to `working-directory`, a path assembled from a
variable, a path inside a shell comment, and an exception that has outlived the
debt it records. Each of those is the difference between a guard people keep
and a guard people switch off.
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile
import textwrap

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/a_workflow_runs_no_script_that_is_not_there.py"
KNOWN = "scripts/ci/workflow_scripts_not_in_the_tree.toml"

MINIMUM_CASES = 16  # FLOOR


def fixture(root: pathlib.Path, workflow: str, *, scripts: list[str],
            known: str | None = None) -> None:
    (root / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
    shutil.copy2(REPO / GUARD, root / GUARD)
    (root / ".github" / "workflows").mkdir(parents=True, exist_ok=True)
    (root / ".github" / "workflows" / "w.yml").write_text(
        textwrap.dedent(workflow), encoding="utf-8")
    for rel in scripts:
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("#!/bin/sh\n", encoding="utf-8")
    if known is not None:
        (root / KNOWN).write_text(textwrap.dedent(known), encoding="utf-8")


def run(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(root / GUARD)],
                          capture_output=True, text=True)


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    present = """
        on: push
        jobs:
          j:
            runs-on: ubuntu-latest
            steps:
              - run: bash scripts/release/there.sh
    """

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp) / "ok"
        fixture(root, present, scripts=["scripts/release/there.sh"])
        done = run(root)
        case("a script that is in the tree passes", done.returncode == 0,
             done.stdout + done.stderr)

        root = pathlib.Path(tmp) / "missing"
        fixture(root, present, scripts=[])
        done = run(root)
        case("a script that is not in the tree fails, by name",
             done.returncode == 1 and "scripts/release/there.sh" in done.stderr,
             done.stdout + done.stderr)

        # The false positive that would have made this guard unusable.
        step_dir = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: bash scripts/release/publish.sh
                    working-directory: crates/pdf-node
        """
        root = pathlib.Path(tmp) / "stepdir"
        fixture(root, step_dir, scripts=["crates/pdf-node/scripts/release/publish.sh"])
        done = run(root)
        case("a path relative to the step's working-directory passes",
             done.returncode == 0, done.stdout + done.stderr)

        job_dir = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                defaults:
                  run:
                    working-directory: crates/pdf-node
                steps:
                  - run: bash scripts/release/publish.sh
        """
        root = pathlib.Path(tmp) / "jobdir"
        fixture(root, job_dir, scripts=["crates/pdf-node/scripts/release/publish.sh"])
        done = run(root)
        case("a path relative to the job's defaults passes",
             done.returncode == 0, done.stdout + done.stderr)

        # A root copy is not the one the step needs. The runner resolves the
        # command beneath working-directory and nowhere else, so a fallback to
        # the repository root makes this guard green about a workflow that
        # fails. (codex, #1915)
        root = pathlib.Path(tmp) / "rootfallback"
        fixture(root, step_dir, scripts=["scripts/release/publish.sh"])
        done = run(root)
        case("a root copy does not stand in for the working-directory one",
             done.returncode == 1 and "scripts/release/publish.sh" in done.stderr,
             done.stdout + done.stderr)

        # Things a file check cannot judge, and must not pretend to.
        assembled = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: bash scripts/${{ matrix.dir }}/go.sh
        """
        root = pathlib.Path(tmp) / "assembled"
        fixture(root, assembled, scripts=[])
        done = run(root)
        case("a path assembled from a variable is not judged",
             done.returncode == 0, done.stdout + done.stderr)

        commented = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: |
                      # scripts/release/gone.sh was removed in #123
                      echo done
        """
        root = pathlib.Path(tmp) / "commented"
        fixture(root, commented, scripts=[])
        done = run(root)
        case("a path inside a shell comment is not judged",
             done.returncode == 0, done.stdout + done.stderr)

        inline = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: |
                      echo done  # scripts/release/removed.sh went in #123
                      bash scripts/release/there.sh
        """
        root = pathlib.Path(tmp) / "inline"
        fixture(root, inline, scripts=["scripts/release/there.sh"])
        done = run(root)
        case("a path in an inline shell comment is not judged",
             done.returncode == 0, done.stdout + done.stderr)

        # But a `#` inside quotes is not a comment, and the command after it
        # still runs.
        quoted = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: |
                      echo "a # b" && bash scripts/release/gone.sh
        """
        root = pathlib.Path(tmp) / "quoted"
        fixture(root, quoted, scripts=[])
        done = run(root)
        case("a hash inside quotes does not hide the rest of the line",
             done.returncode == 1 and "scripts/release/gone.sh" in done.stderr,
             done.stdout + done.stderr)

        # An escaped quote does not close the string, so the `#` stays inside
        # it and the command after `&&` still runs. (codex, #1915)
        escaped = """
            on: push
            jobs:
              j:
                runs-on: ubuntu-latest
                steps:
                  - run: |
                      echo "a \\" # still quoted" && bash scripts/release/gone.sh
        """
        root = pathlib.Path(tmp) / "escaped"
        fixture(root, escaped, scripts=[])
        done = run(root)
        case("an escaped quote does not close the string and hide the command",
             done.returncode == 1 and "scripts/release/gone.sh" in done.stderr,
             done.stdout + done.stderr)

        # The same written path under two working directories is two files.
        two_places = """
            on: push
            jobs:
              a:
                runs-on: ubuntu-latest
                steps:
                  - run: bash scripts/release/there.sh
              b:
                runs-on: ubuntu-latest
                defaults:
                  run:
                    working-directory: crates/pdf-node
                steps:
                  - run: bash scripts/release/there.sh
        """
        scoped = """
            [["missing"]]
            workflow = "w.yml"
            path = "scripts/release/there.sh"
            reden = "the root one is behind a secret that is never set"
            ticket = 358
        """
        root = pathlib.Path(tmp) / "twoplaces"
        fixture(root, two_places, scripts=[], known=scoped)
        done = run(root)
        case("an exception for the root path does not exempt the working-directory one",
             done.returncode == 1
             and "beneath crates/pdf-node" in done.stderr
             and "SKIPPED (not a pass)" in done.stderr,
             done.stdout + done.stderr)

        registered = """
            [["missing"]]
            workflow = "w.yml"
            path = "scripts/release/there.sh"
            reden = "the step is behind a secret that is never set"
            ticket = 358
        """
        root = pathlib.Path(tmp) / "registered"
        fixture(root, present, scripts=[], known=registered)
        done = run(root)
        case("a registered exception passes and says so on stderr",
             done.returncode == 0 and "SKIPPED (not a pass)" in done.stderr,
             done.stdout + done.stderr)

        # An exception without a reason or a ticket is a bare pair that
        # exempts a missing executable and says nothing. (codex, #1915)
        bare = """
            [["missing"]]
            workflow = "w.yml"
            path = "scripts/release/there.sh"
        """
        root = pathlib.Path(tmp) / "noreason"
        fixture(root, present, scripts=[], known=bare)
        done = run(root)
        case("an exception with no reden is refused",
             done.returncode != 0 and "reden" in (done.stdout + done.stderr),
             done.stdout + done.stderr)

        no_ticket = """
            [["missing"]]
            workflow = "w.yml"
            path = "scripts/release/there.sh"
            reden = "the step is behind a secret that is never set"
        """
        root = pathlib.Path(tmp) / "noticket"
        fixture(root, present, scripts=[], known=no_ticket)
        done = run(root)
        case("an exception with no ticket is refused",
             done.returncode != 0 and "ticket" in (done.stdout + done.stderr),
             done.stdout + done.stderr)

        # bool is a subclass of int, so `ticket = true` passed an isinstance
        # check. (codex, #1915)
        boolean_ticket = """
            [["missing"]]
            workflow = "w.yml"
            path = "scripts/release/there.sh"
            reden = "the step is behind a secret that is never set"
            ticket = true
        """
        root = pathlib.Path(tmp) / "booleanticket"
        fixture(root, present, scripts=[], known=boolean_ticket)
        done = run(root)
        case("a boolean ticket is refused",
             done.returncode != 0 and "ticket" in (done.stdout + done.stderr),
             done.stdout + done.stderr)

        # An exception nobody retires stops describing anything.
        root = pathlib.Path(tmp) / "stale"
        fixture(root, present, scripts=["scripts/release/there.sh"], known=registered)
        done = run(root)
        case("an exception whose file is back fails as stale",
             done.returncode == 1 and "stale" in done.stderr,
             done.stdout + done.stderr)

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: a workflow cannot run a script that is not there")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
