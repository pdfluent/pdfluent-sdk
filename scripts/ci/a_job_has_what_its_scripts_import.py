#!/usr/bin/env python3
"""A job installs what the scripts it runs import (#1660).

`baseline-hardware-guard` ran a script that had just grown `import yaml`, and
nothing installed PyYAML. The failure is a ModuleNotFoundError on the first
runner without a preinstall -- so it does not fail on the machine where the
change was written, which is the worst place for a dependency to be missing.

Third time a guard has grown this import and its job did not know, so it is
checked rather than remembered. The rule is narrow on purpose: only
third-party imports that this repository actually installs somewhere are
considered, because the point is to catch a job that fell behind its scripts,
not to build a package manager.
"""
from __future__ import annotations
import pathlib, re, sys

import yaml

WORTEL = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = WORTEL / ".github" / "workflows"
SCRIPTS = WORTEL / "scripts" / "ci"

# import name -> what installs it. Only modules this repository installs in at
# least one job: a name nobody installs anywhere is a different problem and
# would make this guard guess.
PAKKET = {"yaml": "pyyaml", "requests": "requests", "tomli": "tomli"}


def zonder_commentaar(script: str) -> str:
    """A `run:` block with its full-line shell comments removed.

    A comment naming a script is not an invocation of it. Measured on
    02-09-2026: this guard read the sentence explaining WHY a workflow avoids
    naming a script as the workflow running that script, and demanded a
    dependency for it -- which is precisely the mistake it was written next to,
    and the third time today a guard in this repository has read a comment as
    wiring (a runner label, a job, and now an import).

    Full-line comments only. A trailing `# ...` after a command cannot be
    stripped safely without parsing the shell -- `"${X#Y}"` is not a comment --
    and a script named in a trailing comment on the same line as a real command
    is rare enough to leave to a reader.
    """
    return "\n".join(r for r in script.splitlines()
                      if not r.lstrip().startswith("#"))


def importeert(pad: pathlib.Path) -> set[str]:
    tekst = pad.read_text(errors="replace")
    uit = set()
    for naam in PAKKET:
        if re.search(rf"^\s*(?:import {naam}\b|from {naam}\b)", tekst, re.M):
            uit.add(naam)
    return uit


def main() -> int:
    per_script = {p.name: importeert(p) for p in SCRIPTS.glob("*.py")}
    problemen: list[str] = []
    bekeken = 0

    paden = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as e:
            print(f"[job-imports] FATAL: {pad.name} will not parse: {e}", file=sys.stderr)
            return 2
        if not isinstance(doc, dict):
            continue
        for jn, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            stappen = [s for s in (job.get("steps") or []) if isinstance(s, dict)]
            if not stappen:
                continue
            bekeken += 1
            tekst = " ".join(zonder_commentaar(str(s.get("run", "")))
                             for s in stappen)
            geinstalleerd = tekst.lower()
            for script, modules in per_script.items():
                if not modules or script not in tekst:
                    continue
                for m in sorted(modules):
                    if PAKKET[m] not in geinstalleerd:
                        problemen.append(
                            f"{pad.name}:{jn} runs {script}, which imports "
                            f"`{m}`, and the job never installs {PAKKET[m]}. It "
                            "fails with ModuleNotFoundError on a runner without "
                            "a preinstall -- which is not the machine the change "
                            "was written on.")

    if bekeken == 0:
        print("[job-imports] FATAL: no job with steps was read. A scan that "
              "looked at nothing cannot report agreement.", file=sys.stderr)
        return 2
    if problemen:
        print(f"[job-imports] FAIL: {len(problemen)} job(s) miss a dependency:",
              file=sys.stderr)
        for p in sorted(set(problemen)):
            print(f"    {p}", file=sys.stderr)
        return 1
    print(f"[job-imports] OK: {bekeken} job(s); each installs what its scripts import")
    return 0


if __name__ == "__main__":
    sys.exit(main())
