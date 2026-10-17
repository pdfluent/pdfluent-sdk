#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The two halves of a split lane cover every gate, once.

A gate run on a throwaway instance is judged in two places: five gates that need
this operator's keyring and private lists stay on the Mac, everything else goes
to the instance (#474). Two ways for that to go quietly wrong, and neither shows
up as a red build:

  a name in the list that no longer names a gate -- the list looks maintained,
  the gate it meant to keep here went to the instance, and it answers "SKIPPED
  (not a pass)" there, which is a red for a reason nobody can act on;

  a name in the list that also runs remotely, or a gate that runs in neither
  half -- the second is the expensive one, because a lane that judges nothing
  reports the same PASS as a lane that judged everything.

So the names are read out of the register and out of the gate script, and
compared. The register is the single source; this is what stops it drifting from
the file it describes.
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys
import tempfile

WORTEL = pathlib.Path(__file__).resolve().parents[2]
GATE = WORTEL / "scripts" / "ci" / "local_ci_gate.sh"
REGISTER = WORTEL / "scripts" / "ci" / "gates_that_need_this_machine.txt"
sys.path.insert(0, str(WORTEL / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402

fouten: list[str] = []
gedaan = 0


def verwacht(wat: str, ok: bool, detail: str = "") -> None:
    global gedaan
    gedaan += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fouten.append(wat)


def namen_in_de_poort() -> set[str]:
    """Every short name local_ci_gate.sh gives to a gate.

    Read from the call, not from a list somewhere: the four helpers all take the
    name first, and a name that is only mentioned in a comment is exactly what a
    text scan would wrongly accept.
    """
    patroon = re.compile(r"^\s*(?:run|zwaar|scoped)\s+(\S+)|^\s*crate_gate\s+(\S+)", re.M)
    gevonden = set()
    for a, b in patroon.findall(GATE.read_text()):
        naam = a or b
        if naam and not naam.startswith("$"):
            gevonden.add(naam)
    return gevonden


def namen_in_het_register() -> list[str]:
    namen = []
    for regel in REGISTER.read_text().splitlines():
        regel = regel.split("#", 1)[0].strip()
        if regel:
            namen.append(regel)
    return namen


poort = namen_in_de_poort()
# A scan that finds nothing approves everything, which is the mistake every
# guard in this directory is written against.
verwacht("the gate script yields a plausible number of gate names", len(poort) >= 100, str(len(poort)))

register = namen_in_het_register()
verwacht("the register holds no duplicate", len(register) == len(set(register)), str(register))
for naam in register:
    verwacht(f"'{naam}' is a gate the lane actually runs", naam in poort,
             "not found in local_ci_gate.sh")
for naam in register:
    verwacht(f"'{naam}' carries a reason", "#" in next(
        r for r in REGISTER.read_text().splitlines() if r.strip().startswith(naam)))

# Formatting is judged on the Mac, before any machine is queued for (23-09-2026):
# T4's 1948 waited two hours thirty-eight in the machine queue and was refused by
# `cargo fmt` on the instance. A register without it sends fmt back to the queue.
verwacht("fmt is judged before the queue, on this machine", "fmt" in register, str(register))

# And the behaviour, driven rather than read: an unknown place must refuse rather
# than silently run everything, because `GATE_LANE_PLACE=remoet` would otherwise
# be a full lane on a machine that cannot judge five of its gates -- and it must
# refuse in the first second, before the lock, or the refusal arrives hours
# after the mistake.
#
# In a throwaway repository holding nothing but the script: this file must not
# be able to start a real gate in the real checkout.
with tempfile.TemporaryDirectory() as tijdelijk:
    nep = pathlib.Path(tijdelijk)
    (nep / "scripts" / "ci").mkdir(parents=True)
    (nep / "scripts" / "ci" / "local_ci_gate.sh").write_text(GATE.read_text())
    omgeving = sealed_env(cwd=nep)
    subprocess.run(["git", "init", "-q", "."], cwd=nep, env=omgeving, check=True)
    omgeving["GATE_LANE_PLACE"] = "nowhere"
    r = subprocess.run(["bash", "scripts/ci/local_ci_gate.sh", "--fast"],
                       cwd=nep, env=omgeving, capture_output=True, text=True, timeout=60)
verwacht("an unknown GATE_LANE_PLACE is refused", r.returncode == 2,
         f"rc={r.returncode}: {r.stderr[-200:]}")
verwacht("and it says nothing was checked", "NOTHING WAS CHECKED" in r.stderr, r.stderr[-200:])

print(f"test_gate_places_partition_the_lane: {gedaan} case(s), {len(fouten)} failure(s)")
sys.exit(1 if fouten else 0)
