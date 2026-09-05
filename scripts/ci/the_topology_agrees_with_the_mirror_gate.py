#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The written topology and the gate that enforces it must say the same thing.

WHY WRITING IT DOWN WAS NOT ENOUGH

`mirror_has_not_drifted.py` decides a direction: `MIRROR_SOURCE` is pushed onto
`MIRROR_TARGET`, and any amount of the target being ahead is a failure. That is
the whole safety property -- get the direction backwards and the guard cheerfully
protects the copy from the original.

The direction was also written in prose, in a table of repositories and roles.
Nothing tied the two statements together, and the prose was wrong for three days:
it called GitLab a CI executor after the pipelines there had been switched off
(#231, 28-08-2026). A table can be wrong without anything happening; a defaulted
environment variable can be changed without anyone reading the table.

So this guard makes them one statement. The table names the git remote per row;
the gate's defaults name a remote per role. Flipping either alone fails here,
which turns "which repository is the source" back into a decision that comes past
review rather than a setting.

WHAT IT CHECKS

  * the topology section exists in CLAUDE.md and carries at least ROW_FLOOR rows
  * exactly one row names the gate's source remote, and its role says `source`
  * exactly one row names the gate's mirror remote, and its role says `backup`
  * the mirror's row does not describe it as a CI executor -- the sentence that
    was wrong on 28-08 and that nobody could see was wrong

Exit codes:
  0  the table and the gate agree
  1  they do not, and the row and the role are named
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
TOPOLOGY = REPO / "CLAUDE.md"
GATE = REPO / "scripts" / "ci" / "mirror_has_not_drifted.py"

HEADING = "## Repository topology"

# FLOOR: a table this guard reads must have rows in it. A section trimmed to two
# lines would otherwise pass every rule below by having nothing to break, which
# is the shape of every measurement this repository has had to withdraw.
ROW_FLOOR = 5

# What the mirror may not be called. GitLab ran the heavy CI until 28-08-2026 and
# the table went on saying so afterwards; these are the words that sentence used.
NOT_A_ROLE_FOR_A_BACKUP = ("ci executor", "ci runner", "pipeline", "runs the heavy")

DEFAULT = re.compile(
    r'os\.environ\.get\(\s*"MIRROR_(SOURCE|TARGET)"\s*,\s*"([^"/]+)/([^"]+)"\s*\)')


def gate_remotes() -> tuple[str, str]:
    """The remote names the gate defaults to, as (source, mirror)."""
    if not GATE.is_file():
        raise SystemExit(
            "the_topology_agrees_with_the_mirror_gate: SKIPPED (not a pass) -- "
            f"{GATE.relative_to(REPO)} is missing, so nothing was compared.")
    found = {m.group(1): m.group(2) for m in DEFAULT.finditer(GATE.read_text())}
    if set(found) != {"SOURCE", "TARGET"}:
        raise SystemExit(
            "the_topology_agrees_with_the_mirror_gate: SKIPPED (not a pass) -- "
            f"{GATE.relative_to(REPO)} no longer spells its defaults as "
            '`os.environ.get("MIRROR_SOURCE", "<remote>/<branch>")`, so the '
            "direction could not be read. Nothing was compared.")
    return found["SOURCE"], found["TARGET"]


def rows() -> list[tuple[str, str, str]]:
    """The (repository, remote, role) rows of the topology table."""
    text = TOPOLOGY.read_text(encoding="utf-8") if TOPOLOGY.is_file() else ""
    if HEADING not in text:
        return []
    section = text.split(HEADING, 1)[1].split("\n## ", 1)[0]
    out = []
    for line in section.splitlines():
        line = line.strip()
        if not line.startswith("|") or set(line) <= set("|-: "):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if len(cells) < 3 or cells[0].lower().startswith("repository"):
            continue
        out.append((cells[0], cells[1].strip("`"), cells[2]))
    return out


def main() -> int:
    source_remote, mirror_remote = gate_remotes()
    table = rows()
    complaints: list[str] = []

    if len(table) < ROW_FLOOR:
        print(f"[topology] FATAL: {TOPOLOGY.name} has {len(table)} row(s) under "
              f"'{HEADING}', fewer than the {ROW_FLOOR} this guard reads. Either the "
              "section was removed or its table changed shape; nothing below it was "
              "checked.", file=sys.stderr)
        return 1

    for remote, role_word in ((source_remote, "source"),
                              (mirror_remote, "backup")):
        named = [r for r in table if r[1] == remote]
        if len(named) != 1:
            complaints.append(
                f"the gate pushes {source_remote} onto {mirror_remote}, and the table "
                f"names the remote '{remote}' in {len(named)} row(s) -- it has to be "
                "exactly one, or the roles cannot be read off it")
            continue
        repo, _, role = named[0]
        # The role is the word in bold at the head of the cell, not any mention
        # of it in the sentence. Reading the whole cell made "a copy of the
        # source" count as calling the backup the source, which is a guard
        # failing on its own prose rather than on the topology.
        spelled = re.match(r"\s*\*\*(\w+)\*\*", role)
        primary = spelled.group(1).lower() if spelled else ""
        if primary != role_word:
            complaints.append(
                f"the gate treats '{remote}' as the {role_word}, and the table gives "
                f"{repo} the role \"{role}\" -- a row for one of the two remotes has "
                f"to open with **{role_word}**, and this one opens with "
                f"{'**' + primary + '**' if primary else 'no role in bold'}")
        if role_word == "backup":
            for phrase in NOT_A_ROLE_FOR_A_BACKUP:
                if phrase in role.lower():
                    complaints.append(
                        f"{repo} is described as \"{role}\", and the automatic "
                        "pipelines there were switched off on 28-08-2026. A backup "
                        "that is also written down as a CI executor is the sentence "
                        "that stayed wrong for three days (#231)")

    if complaints:
        print(f"[topology] FATAL: {len(complaints)} disagreement(s) between "
              f"{TOPOLOGY.name} and {GATE.name}:", file=sys.stderr)
        for c in complaints:
            print(f"  - {c}", file=sys.stderr)
        print("\nOne of the two was changed without the other. Decide which is "
              "right and change both in the same commit -- that is what makes the "
              "direction a decision instead of a default.", file=sys.stderr)
        return 1

    print(f"[topology] OK: {len(table)} rows; '{source_remote}' is the source and "
          f"'{mirror_remote}' the backup, in the table and in the gate.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
