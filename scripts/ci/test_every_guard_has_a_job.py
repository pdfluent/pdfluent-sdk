#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does the guard on the guards still bite?

`every_guard_has_a_job.py` is the one check that decides whether the other
seventy-nine mean anything, so it is also the one whose silent breakage would be
invisible: a parser that stops recognising jobs reports a clean tree in exactly
the same words as a clean tree.

Each case builds a whole synthetic repository -- a .gitlab-ci.yml, a workflow,
and a scripts/ci/ -- and runs a copy of the real script inside it with the
ratchet rewritten to the number that case needs. Nothing here reads the real
repository, so a case cannot pass because the real tree happens to be in the
right shape.

Run: python3 scripts/ci/test_every_guard_has_a_job.py
"""
from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from pathlib import Path

REAL = Path(__file__).resolve().parent / "every_guard_has_a_job.py"

try:
    import yaml  # noqa: F401  the script under test needs it
except ImportError:
    print(
        "SKIPPED (not a pass): pyyaml is not installed, so the script under test "
        "cannot read a pipeline and none of these cases would mean anything.",
        file=sys.stderr,
    )
    sys.exit(0)

try:
    import tomllib  # noqa: F401  the register is TOML
except ModuleNotFoundError:
    print(
        "SKIPPED (not a pass): tomllib is missing (Python < 3.11), so the register "
        "half of this script is unreachable.",
        file=sys.stderr,
    )
    sys.exit(0)


def build(
    tmp: Path,
    *,
    on_github: list[str],
    on_gitlab: list[str],
    register: str,
    ratchet: int,
    unreferenced: list[str] | None = None,
    dispatch_only: list[str] | None = None,
    parked: list[str] | None = None,
    min_scripts: int = 2,
    excused_to_the_gate: str | None = None,
    in_the_gate: list[str] | None = None,
) -> Path:
    """A synthetic repository, and the path to the script inside it.

    `dispatch_only` guards get a `run:` in a workflow that fires on
    workflow_dispatch alone; `parked` guards get one in the pull-request
    workflow, in a job behind `needs: parked`. Both are the shape from #1633: a
    real `run:` line that no pull request will ever execute.
    """
    ci = tmp / "scripts" / "ci"
    ci.mkdir(parents=True)
    flows = tmp / ".github" / "workflows"
    flows.mkdir(parents=True)

    names = (set(on_github) | set(on_gitlab) | set(unreferenced or [])
             | set(dispatch_only or []) | set(parked or []))
    for name in names:
        (ci / name).write_text("#!/bin/sh\nexit 0\n")

    (tmp / ".gitlab-ci.yml").write_text(
        "stages: [sanity]\n"
        + "".join(
            f"job{i}:\n  stage: sanity\n  script:\n    - python3 scripts/ci/{n}\n"
            for i, n in enumerate(on_gitlab)
        )
    )
    (flows / "ci.yml").write_text(
        "name: CI\non:\n  pull_request:\n  push:\n    branches: [master]\n"
        "jobs:\n  guard:\n    runs-on: ubuntu-latest\n    steps:\n"
        + "".join(
            f"      - name: step {i}\n        run: python3 scripts/ci/{n}\n"
            for i, n in enumerate(on_github)
        )
        # The parked shape from crash-guard.yml and gate-ci.yml: a job that
        # always fails, and a real job behind it that therefore never starts.
        + "  parked:\n    runs-on: ubuntu-latest\n    steps:\n"
        "      - run: exit 1\n"
        "  behind:\n    needs: parked\n    runs-on: ubuntu-latest\n    steps:\n"
        + "".join(
            f"      - name: parked step {i}\n        run: python3 scripts/ci/{n}\n"
            for i, n in enumerate(parked or [])
        )
    )
    (flows / "dispatch.yml").write_text(
        "name: By hand\non:\n  workflow_dispatch:\n"
        "jobs:\n  guard:\n    runs-on: ubuntu-latest\n    steps:\n"
        + "".join(
            f"      - name: step {i}\n        run: python3 scripts/ci/{n}\n"
            for i, n in enumerate(dispatch_only or [])
        )
    )
    (ci / "mirror_only_guards.toml").write_text(register)

    # The local gate as a fixture. `excused_to_the_gate` is a guard whose ALLOWED
    # reason claims the gate runs it; `in_the_gate` is what the gate actually
    # calls. Splitting the two is the point -- the entry for
    # mirror_has_not_drifted.py claimed a place for months and the gate had never
    # named it (#231).
    if in_the_gate is not None:
        (ci / "local_ci_gate.sh").write_text(
            "#!/usr/bin/env bash\n"
            + "".join(f"run x{i} python3 scripts/ci/{n}\n"
                      for i, n in enumerate(in_the_gate))
        )

    source = REAL.read_text()
    source = re.sub(r"^SPIEGEL_RATEL = \d+$", f"SPIEGEL_RATEL = {ratchet}", source, flags=re.M)
    source = re.sub(r"^MIN_SCRIPTS = \d+$", f"MIN_SCRIPTS = {min_scripts}", source, flags=re.M)
    # The fixture carries none of the real repository's hand-run tools -- only
    # the copy of the script itself, which is in ALLOWED for the same reason it
    # is in the real one.
    allowed = {"every_guard_has_a_job.py": "this file itself"}
    if in_the_gate is not None:
        # The gate is not a guard, and in the real repository it is excused for
        # exactly that reason. Without this line the fixture's own gate file
        # reads as an orphan and the case fails on something it is not about.
        allowed["local_ci_gate.sh"] = "the gate itself"
    if excused_to_the_gate:
        allowed[excused_to_the_gate] = "runs in scripts/ci/local_ci_gate.sh"
    source = re.sub(
        r"^ALLOWED: dict\[str, str\] = \{.*?^\}$",
        f"ALLOWED: dict[str, str] = {allowed!r}",
        source,
        flags=re.M | re.S,
    )
    target = ci / "every_guard_has_a_job.py"
    target.write_text(source)
    return target


def run(**kwargs) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as d:
        script = build(Path(d), **kwargs)
        r = subprocess.run(
            [sys.executable, str(script)], capture_output=True, text=True, timeout=120
        )
        return r.returncode, r.stdout + r.stderr


ENTRY = """
[[guard]]
script = "{name}"
reason = "corpus"
issue = "#288"
why = "needs the corpus machine"
"""

failures: list[str] = []
passes = 0


def case(label: str, expect_rc: int, expect_text: str | None = None, **kwargs) -> None:
    global passes
    rc, out = run(**kwargs)
    if rc != expect_rc or (expect_text and expect_text not in out):
        failures.append(f"{label}: rc={rc} (expected {expect_rc})\n{out}")
    else:
        passes += 1
        print(f"  ok    {label}")


# The shape that must pass: one guard on the mirror, named in the register, and
# the ratchet standing at one. If this fails every other case is meaningless.
case(
    "a declared mirror guard at the written number passes",
    0,
    on_github=["a.py", "b.py"],
    on_gitlab=["c.py"],
    register=ENTRY.format(name="c.py"),
    ratchet=1,
)

# The failure this whole issue is about. Two guards on the mirror, one of them
# declared, the ratchet standing at two -- so the count is right and the register
# is valid, and the only thing wrong is the missing reason. A case that trips
# three checks at once would still go red with two of them removed.
case(
    "a mirror-only guard with no written reason fails",
    1,
    "no reason for that is written down",
    on_github=["a.py"],
    on_gitlab=["c.py", "d.py"],
    register=ENTRY.format(name="c.py"),
    ratchet=2,
)

# Upwards: one more guard slid onto the mirror.
case(
    "one more on the mirror than written down fails",
    1,
    "more than the",
    on_github=["a.py"],
    on_gitlab=["c.py", "d.py"],
    register=ENTRY.format(name="c.py") + ENTRY.format(name="d.py"),
    ratchet=1,
)

# Downwards: the win has to be booked, or the room fills again unnoticed. One
# guard is left on the mirror and declared, so the register is valid and the
# count is the only thing that moved.
case(
    "one fewer on the mirror than written down also fails",
    1,
    "fewer than the",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    register=ENTRY.format(name="c.py"),
    ratchet=2,
)

# A reason that outlived its situation reads as current.
case(
    "a register entry for a guard that now runs on GitHub fails",
    1,
    "now runs in GitHub Actions",
    on_github=["a.py", "c.py"],
    on_gitlab=["c.py", "d.py"],
    register=ENTRY.format(name="c.py") + ENTRY.format(name="d.py"),
    ratchet=1,
)

# A register entry naming a script nobody kept.
case(
    "a register entry for a script that no longer exists fails",
    1,
    "the script no longer exists",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    register=ENTRY.format(name="c.py") + ENTRY.format(name="ghost.py"),
    ratchet=1,
)

# Free text would approve every reason, including "later".
case(
    "an unknown reason fails",
    1,
    "is none of",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    register='\n[[guard]]\nscript = "c.py"\nreason = "later"\nissue = "#288"\nwhy = "no time"\n',
    ratchet=1,
)

# A required field left empty is a decision that was not taken.
case(
    "an entry without a why fails",
    1,
    "field `why` is missing",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    register='\n[[guard]]\nscript = "c.py"\nreason = "corpus"\nissue = "#288"\nwhy = ""\n',
    ratchet=1,
)

# An unreadable register must not read as an empty one.
case(
    "a register that is not TOML fails rather than counting as empty",
    1,
    "is not valid TOML",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    register="[[guard]\nscript =\n",
    ratchet=1,
)

# The finding from #1633: a `run:` line in a job that no pull request ever
# executes. Behind `needs: parked` the job never starts; in a dispatch-only
# workflow it starts when somebody remembers. Either way the guard is counted
# with the mirror guards -- declared, and inside the ratchet -- not as a job.
case(
    "a guard behind needs: parked is judged with the mirror guards",
    0,
    on_github=["a.py"],
    on_gitlab=[],
    parked=["d.py"],
    register=ENTRY.format(name="d.py"),
    ratchet=1,
)

case(
    "a guard only in a dispatch-only workflow with no written reason fails",
    1,
    "dispatch-only or parked",
    on_github=["a.py"],
    on_gitlab=["c.py"],
    dispatch_only=["d.py"],
    register=ENTRY.format(name="c.py"),
    ratchet=2,
)

# A guard nobody runs at all -- the older failure this file also covers.
case(
    "a guard with no job anywhere fails",
    1,
    "nothing runs them",
    on_github=["a.py", "b.py"],
    on_gitlab=[],
    unreferenced=["orphan.py"],
    register="",
    ratchet=0,
)

# And the search itself: too few scripts means the glob broke, not that the tree
# is clean. An empty scan would approve everything.
case(
    "a search that finds almost nothing is not a green",
    1,
    "FLOOR",
    on_github=["a.py"],
    on_gitlab=[],
    register="",
    ratchet=0,
    min_scripts=99,
)

# THE EXEMPTION THAT NAMES A PLACE. Between them these two are the whole of #231:
# a guard excused to the local gate that the gate does not call is exactly the
# state mirror_has_not_drifted.py was in for months, booked as placed.
case(
    "an exemption that names the local gate, and the gate does not run it",
    1,
    "and it does not",
    on_github=["a.py", "b.py"],
    on_gitlab=[],
    unreferenced=["byhand.py"],
    excused_to_the_gate="byhand.py",
    in_the_gate=["a.py"],
    register="",
    ratchet=0,
)

case(
    "the same exemption passes once the gate really calls it",
    0,
    None,
    on_github=["a.py", "b.py"],
    on_gitlab=["c.py"],
    unreferenced=["byhand.py"],
    excused_to_the_gate="byhand.py",
    in_the_gate=["byhand.py"],
    register=ENTRY.format(name="c.py"),
    ratchet=1,
)

print()
if failures:
    print(f"{len(failures)} case(s) failed:\n", file=sys.stderr)
    for f in failures:
        print(f"  - {f}\n", file=sys.stderr)
    sys.exit(1)

print(f"{passes} passed, 0 failed")
