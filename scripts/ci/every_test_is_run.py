#!/usr/bin/env python3
"""Every `scripts/ci/test_*.py` is executed by something, or says why not.

Found while fixing #1672, where a check enumerated three file paths by hand
while five files needed covering. This is the same shape one level up: the test
files that police this repository were themselves wired in by hand, one `run:`
line each, and nothing established that a new one runs anywhere at all. A test
file that no job executes is indistinguishable from a test that passes -- and
that difference is what three months of unrun binding tests cost (#307).

So the set is DERIVED from the tree and compared against what the workflows and
the local gate actually RUN.

WHY IT READS COMMANDS AND NOT TEXT

Scanning the raw YAML would count a file named in a comment, in a job `name:`,
or in an epitaph like `# was: python3 scripts/ci/test_x.py`. That is precisely
the defect this guard exists to catch, and `ci_dekking` had it until #1685: the
real step was replaced with `run: true`, the old command left as a comment, and
the guard stayed green. A guard that can be satisfied by a sentence about a
test is not a guard. So only `run`, `script`, `before_script` and `after_script`
are read, and a file that will not parse is an error rather than a fallback to
its bytes.

WHY THE EXCEPTIONS COME FROM THE EXISTING REGISTER

`every_guard_has_a_job.py` already records which guards run nowhere and why,
with the condition for removing each entry. A second list would be a second
answer to one question, which is the defect the register guards exist to catch.
This imports that one.
"""
from __future__ import annotations

import pathlib
import re
import sys

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / "scripts" / "ci"
WORKFLOWS = REPO / ".github" / "workflows"
GITLAB = REPO / ".gitlab-ci.yml"
GATE = CI / "local_ci_gate.sh"

# A search that finds nothing is broken, not clean. Well below the 54 present
# on 03-09-2026, so ordinary deletion does not trip it, but far enough above
# zero to catch a glob that stopped matching.
VLOER = 25

UITVOERSLEUTELS = ("run", "script", "before_script", "after_script")


def uitvoerregels(tekst: str) -> list[str] | None:
    """What a workflow RUNS, or None if it will not parse.

    Returning None rather than falling back to the raw text is the point: a file
    whose jobs cannot be established is not a file whose jobs are empty.
    """
    try:
        doc = yaml.safe_load(tekst)
    except yaml.YAMLError:
        return None
    uit: list[str] = []

    def loop(knoop, sleutel=None):
        if isinstance(knoop, dict):
            for k, v in knoop.items():
                loop(v, k)
        elif isinstance(knoop, list):
            for v in knoop:
                loop(v, sleutel)
        elif isinstance(knoop, str) and sleutel in UITVOERSLEUTELS:
            uit.append(knoop)

    loop(doc)
    return uit


def zonder_commentaar(regel: str) -> str:
    """A shell line with its trailing comment removed, quotes respected.

    `#` only starts a comment at a word boundary, so `sha#deadbeef` and
    `echo "# hi"` survive. Same reader as the one in
    guards_do_not_hide_behind_each_other.py, and for the same reason (#321).
    """
    uit: list[str] = []
    quote: str | None = None
    na_spatie = True
    for teken in regel:
        if quote:
            uit.append(teken)
            if teken == quote:
                quote = None
        elif teken in ("'", '"'):
            quote = teken
            uit.append(teken)
        elif teken == "#" and na_spatie:
            break
        else:
            uit.append(teken)
        na_spatie = teken.isspace()
    return "".join(uit)


def draait(naam: str, blob: str) -> bool:
    """Does `naam` appear as a whole path, not as the tail of a longer one?

    `test_infra_health.py` contains `infra_health.py`, so a plain substring
    match would report a guard as covered because its namesake runs. The same
    regex as `every_guard_has_a_job._runs`, for the same reason.
    """
    patroon = r"(?<![\w.\-])(?:[\w./\-]*/)?" + re.escape(naam) + r"(?![\w.\-])"
    return re.search(patroon, blob) is not None


def uitgezonderd() -> dict[str, str]:
    """The exceptions already recorded in every_guard_has_a_job.py."""
    sys.path.insert(0, str(CI))
    try:
        from every_guard_has_a_job import ALLOWED
    except Exception as fout:  # pragma: no cover - the import is the check
        print(f"[tests-run] SKIPPED (not a pass): could not read the register in "
              f"every_guard_has_a_job.py, so no exception could be honoured: {fout}",
              file=sys.stderr)
        raise SystemExit(1)
    return dict(ALLOWED)


def main() -> int:
    bestanden = sorted(p.name for p in CI.glob("test_*.py"))
    if len(bestanden) < VLOER:
        print(f"[tests-run] FAIL: found {len(bestanden)} test file(s) under "
              f"{CI.relative_to(REPO)}, expected at least {VLOER}. The search is "
              "broken, not the tree.", file=sys.stderr)
        return 1

    commando: list[str] = []
    for wf in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        regels = uitvoerregels(wf.read_text())
        if regels is None:
            print(f"[tests-run] FAIL: {wf.relative_to(REPO)} does not parse as YAML, "
                  "so what it runs cannot be established.", file=sys.stderr)
            return 1
        commando.extend(regels)

    if GITLAB.exists():
        regels = uitvoerregels(GITLAB.read_text())
        if regels is None:
            print(f"[tests-run] FAIL: {GITLAB.relative_to(REPO)} does not parse as YAML.",
                  file=sys.stderr)
            return 1
        commando.extend(regels)

    if GATE.exists():
        commando.extend(zonder_commentaar(r) for r in GATE.read_text().splitlines())

    blob = "\n".join(commando)
    uitzondering = uitgezonderd()

    ongedekt: list[str] = []
    met_reden: list[tuple[str, str]] = []
    for naam in bestanden:
        if draait(naam, blob):
            continue
        if naam in uitzondering:
            met_reden.append((naam, uitzondering[naam]))
            continue
        ongedekt.append(naam)

    # An exception that is no longer needed is a register entry that outlives
    # its subject -- the shape this repository keeps finding. Say so.
    achterhaald = [n for n, _ in met_reden if draait(n, blob)]
    for naam in sorted(uitzondering):
        if naam.startswith("test_") and naam in bestanden and draait(naam, blob):
            achterhaald.append(naam)

    print(f"[tests-run] {len(bestanden)} test file(s), "
          f"{len(bestanden) - len(ongedekt) - len(met_reden)} run by a workflow or the "
          f"local gate, {len(met_reden)} recorded as not running")

    if achterhaald:
        print(f"\nFAIL: {len(achterhaald)} file(s) run now and are still recorded as not "
              f"running: {', '.join(sorted(set(achterhaald)))}. Remove the entry from "
              "ALLOWED in every_guard_has_a_job.py so the next one that stops running "
              "is noticed.", file=sys.stderr)
        return 1

    if ongedekt:
        print(f"\nFAIL: {len(ongedekt)} test file(s) are executed by nothing:\n",
              file=sys.stderr)
        for naam in ongedekt:
            print(f"  - {naam}", file=sys.stderr)
        print("\n  A test no job runs is indistinguishable from a test that passes.\n"
              "  Add a `run:` line in .github/workflows/, a line in local_ci_gate.sh,\n"
              "  or -- if it genuinely cannot run yet -- an entry in ALLOWED in\n"
              "  every_guard_has_a_job.py saying why and what would remove it.",
              file=sys.stderr)
        return 1

    for naam, reden in met_reden:
        print(f"  recorded as not running: {naam} -- {reden}")
    print("[tests-run] OK: every test file is executed by something, or says why not.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
