#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The startup-HOME guard has to reject what it claims to reject.

A guard is only worth its runtime if it goes red on the tree it was written
against: a startup script that reaches `$HOME` under `set -u` with no HOME
exported (run a-20261003-2212, 03-10). So this test builds such trees from
scratch and runs the guard over them -- a missing export, a use before the
set, a bare `HOME=` that never reaches the toolchain's children -- and each
case names the defect and asserts a non-zero exit.

The repaired shapes must come back clean: `export HOME=` before the first
use, in both the unconditional and the `${HOME:-/root}` form, and a script
that never mentions HOME at all. A guard that fails on everything is as
useless as one that fails on nothing, and it is the pair that shows the
verdict tracks the input.

Nothing here touches the real tree except the last case, which runs the
guard over the checkout itself and must come back clean. Every other case
runs in a temporary directory.

Exit codes:
    0  the guard behaves
    1  the guard let something through, or rejected something it should not
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

GUARD = (pathlib.Path(__file__).resolve().parent
         / "every_startup_script_sets_home.py")
REPO = pathlib.Path(__file__).resolve().parents[2]

MISSING = """#!/bin/bash
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.94.0
"""

USE_BEFORE_SET = """#!/bin/bash
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
export HOME=/root
"""

BARE_SET = """#!/bin/bash
set -euo pipefail
HOME=/root
export PATH="$HOME/.cargo/bin:$PATH"
"""

UNCONDITIONAL = """#!/bin/bash
set -euo pipefail
export HOME=/root
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.94.0
"""

WITH_DEFAULT = """#!/bin/bash
set -euo pipefail
# GCE startup scripts run as root with NO HOME set.
export HOME="${HOME:-/root}"
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.94.0
"""

NO_HOME_AT_ALL = """#!/bin/bash
set -euo pipefail
python3 -m venv "$WORK/.venv"
"""


def run_guard(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), "--repo", str(root)],
        capture_output=True, text=True, timeout=120,
    )


def run_case(body: str) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as raw:
        root = pathlib.Path(raw)
        gcp = root / "scripts" / "gcp"
        gcp.mkdir(parents=True)
        # Three scripts: fewer than the floor is its own verdict, not this
        # case's, so the shape under test always travels with company.
        (gcp / "case_startup.sh").write_text(body)
        (gcp / "other_startup.sh").write_text(UNCONDITIONAL)
        (gcp / "third_startup.sh").write_text(NO_HOME_AT_ALL)
        return run_guard(root)


def main() -> int:
    failures: list[str] = []

    def check(name: str, proc: subprocess.CompletedProcess[str],
              red: bool) -> None:
        ok = (proc.returncode != 0) if red else (proc.returncode == 0)
        if not ok:
            failures.append(
                "%s: expected exit %s, got %d (%s%s)" % (
                    name,
                    "non-zero" if red else "0",
                    proc.returncode,
                    proc.stdout.strip()[-160:],
                    proc.stderr.strip()[-160:],
                )
            )

    check("a $HOME with no export", run_case(MISSING), True)
    proc = run_case(MISSING)
    if "case_startup.sh" not in proc.stdout:
        failures.append("the verdict must name the script that uses HOME"
                        " without exporting it")
    check("a use before the set", run_case(USE_BEFORE_SET), True)
    check("a bare HOME= that children never see", run_case(BARE_SET), True)
    check("export HOME=/root before the use",
          run_case(UNCONDITIONAL), False)
    check('export HOME="${HOME:-/root}" before the use',
          run_case(WITH_DEFAULT), False)
    check("no HOME mention at all", run_case(NO_HOME_AT_ALL), False)

    with tempfile.TemporaryDirectory() as raw:
        empty = run_guard(pathlib.Path(raw))
    if empty.returncode == 0:
        failures.append("an empty tree must trip the floor, not pass")
    elif "FLOOR" not in empty.stderr:
        failures.append("the floor must say FLOOR, not fail in silence")

    live = run_guard(REPO)
    if live.returncode != 0:
        failures.append(
            "the guard must pass on the checkout itself: %s%s" % (
                live.stdout.strip()[-300:], live.stderr.strip()[-160:]))

    for failure in failures:
        print("FAIL:", failure)
    print("startup-scripts-set-home-guard: "
          + ("OK" if not failures else "%d failure(s)" % len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
