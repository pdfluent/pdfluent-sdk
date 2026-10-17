#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No workflow calls apt-get without waiting for the lock (#448).

`scripts/ci/apt_get.sh` retries while the dpkg lock is held and fails
immediately on anything else. A step that calls `apt-get` directly does not
wait, and on a throwaway instance -- which spends the whole of a wave in the
first minutes of its uptime, with `unattended-upgrades` still running -- that is
a coin toss. Wave 34163246411 lost it three times in seventy seconds: the
veraPDF install, the PDF/A gate behind it, and the extraction step.

So the rule is mechanical rather than remembered: inside `.github/workflows`,
the words `apt-get` and `apt install` may appear in a comment, or in a line that
calls the wrapper. Anywhere else they are a step that will one day run on a
machine that is still booting.

NOT IN SCOPE: the provisioning scripts (`scripts/infra/build_ci_snapshot.sh`,
`scripts/vps-setup.sh`). Those build the image rather than run on it; they own
the machine at that moment, and nothing else is installing.

Exit codes:
  0  every apt call in a workflow goes through the wrapper
  1  one does not, or the scan found so little that it would approve anything
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"
WRAPPER = "scripts/ci/apt_get.sh"

# `apt` on its own is a different command (interactive, unstable output) and
# nothing here uses it; `apt-get` and `apt install` are the two spellings that
# have been in this tree.
APT = re.compile(r"\bapt-get\b|\bapt\s+install\b")

# FLOORS. A scan that finds nothing approves everything, which is the failure
# this repository keeps re-finding. There are around thirty workflows and the
# ephemeral wave alone calls the wrapper three times.
MIN_WORKFLOWS = 20
MIN_WRAPPER_CALLS = 3


def offending_lines(text: str) -> list[tuple[int, str]]:
    """Lines that name apt-get outside a comment and outside a wrapper call."""
    found = []
    for n, line in enumerate(text.splitlines(), 1):
        if not APT.search(line):
            continue
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if WRAPPER in line:
            continue
        found.append((n, stripped))
    return found


def wrapper_calls(text: str) -> int:
    return sum(1 for line in text.splitlines() if WRAPPER in line)


def main() -> int:
    if not WORKFLOWS.is_dir():
        print(f"[apt-lock] FATAL: {WORKFLOWS} is not there, so nothing was "
              "checked. That is not the same as a pass.", file=sys.stderr)
        return 1

    files = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if len(files) < MIN_WORKFLOWS:
        print(f"[apt-lock] FATAL: found {len(files)} workflow file(s), floor is "
              f"{MIN_WORKFLOWS}. The scan is broken, not the tree.",
              file=sys.stderr)
        return 1

    bad: list[str] = []
    calls = 0
    for path in files:
        text = path.read_text(encoding="utf-8")
        calls += wrapper_calls(text)
        for n, line in offending_lines(text):
            bad.append(f"{path.relative_to(REPO)}:{n}: {line}")

    if calls < MIN_WRAPPER_CALLS:
        print(f"[apt-lock] FATAL: {calls} call(s) to {WRAPPER} in the workflows, "
              f"floor is {MIN_WRAPPER_CALLS}. Either the wrapper stopped being "
              "used or this check stopped being able to see it.", file=sys.stderr)
        return 1

    if bad:
        print("[apt-lock] FAIL: apt is called without waiting for the lock.",
              file=sys.stderr)
        for line in bad:
            print(f"    {line}", file=sys.stderr)
        print(f"\n  Call `bash {WRAPPER} <arguments>` instead. It retries while "
              "the dpkg lock is held and fails at once on anything else, so the "
              "step no longer depends on whether unattended-upgrades has "
              "finished. (#448)", file=sys.stderr)
        return 1

    print(f"[apt-lock] OK: {len(files)} workflows, {calls} apt call(s), all "
          "through the wrapper")
    return 0


if __name__ == "__main__":
    sys.exit(main())
