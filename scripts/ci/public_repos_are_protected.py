#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No public repository of the organisation stands on an unprotected branch (#233).

WHAT WENT WRONG WITHOUT IT

Nothing yet, and that is the whole argument. The four public repositories were
protected by hand on 31-08-2026, and nothing anywhere reads
`branches/*/protection` -- measured over `scripts/ci/`, no file did. So the
fifth repository is created unprotected, `main` takes a force-push, and the
first person to find out is whoever goes looking for a commit that is no longer
there. This is the Definition-of-Done pattern one layer out: not a check that
nothing runs, but a setting nothing checks.

WHAT IT ASKS, IN ORDER

  1. Is every public repository of the organisation in the register?   FATAL
  2. Does its default branch carry protection at all -- force-push off,
     deletions off, linear history on?                                 FATAL
  3. Do the required status checks match the register?                 see below
  4. Would a required context ever be reported?                        FATAL

Four is the trap that makes three careful. A required check that no workflow
produces blocks every pull request for good: the branch waits for a report
nobody will send. So a context is only allowed to be required once it exists --
in this tree, as a job on a `pull_request` trigger, or on that repository, as a
check it has already reported.

WHY THREE IS NOT SIMPLY FATAL TODAY

Every row in the register is `pending`: the requirements are named here and set
by `--apply`, which is an owner's action on a public repository and, for
`pdfluent/pdfluent-sdk`, one that has to wait for the seeding in #222 to put the
workflow there at all. A gate that fails until somebody performs an action it
cannot perform teaches people to push past it.

So a `pending` row reports the gap and does not fail -- and fails the moment the
gap closes without the row being removed. That direction matters more than it
looks: a register of excuses that only grows describes a repository that no
longer exists, and the reason a row is here has to be removed by the change that
repairs it.

WHERE IT RUNS, AND WHY NOT EVERYWHERE

Reading another repository's protection needs `administration:read` on that
repository. `GITHUB_TOKEN` cannot hold it -- it is not a settable scope -- so on
a hosted runner this half can only announce SKIPPED (not a pass), the same wall
`every_label_has_a_runner.py` hit on #290. It runs in `scripts/ci/local_ci_gate.sh`,
where `gh` has a keyring and genuinely checks; `--tree-only` is what the workflow
runs, and it says so rather than reporting a green nobody obtained.

# FLOOR: the register must hold >= 4 rows -- the organisation has had four
# public repositories since July 2026. A register that has quietly emptied would
# otherwise approve every repository in it, which is none.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REGISTER = REPO / "docs" / "PUBLIC_BRANCH_PROTECTION.toml"
WORKFLOWS = REPO / ".github" / "workflows"
ORG = "pdfluent"

MINIMUM_ROWS = 4

# Repositories the organisation owns publicly that are deliberately outside this
# register, with the reason. Empty, and a row here should be an argument
# somebody makes in writing rather than a way to quieten question 1.
BUITEN: dict[str, str] = {}


# --------------------------------------------------------------------------- #
# the register and the tree
# --------------------------------------------------------------------------- #
def lees_register(pad: pathlib.Path = REGISTER) -> list[dict]:
    if not pad.is_file():
        raise SystemExit(
            f"[protection] FATAL: {pad} is missing, so nothing was checked.")
    return tomllib.loads(pad.read_text(encoding="utf-8")).get("repo", [])


def contexts_uit_de_boom(map_: pathlib.Path = WORKFLOWS) -> set[str]:
    """Job names a pull request in THIS tree would report.

    The name and not the job id, because a required status check is matched by
    the name GitHub prints next to the check. A job without one reports its id,
    so that is what is collected for it.
    """
    import yaml

    namen: set[str] = set()
    for bestand in sorted(map_.glob("*.yml")) + sorted(map_.glob("*.yaml")):
        try:
            doc = yaml.safe_load(bestand.read_text(encoding="utf-8")) or {}
        except yaml.YAMLError:
            continue
        if not isinstance(doc, dict):
            continue
        # `on:` is the YAML 1.1 boolean True once parsed, which is why this is
        # not simply doc.get("on").
        triggers = doc.get("on", doc.get(True))
        if isinstance(triggers, str):
            triggers = [triggers]
        if isinstance(triggers, list):
            triggers = {t: None for t in triggers}
        if not isinstance(triggers, dict) or "pull_request" not in triggers:
            continue
        for jid, job in (doc.get("jobs") or {}).items():
            if isinstance(job, dict):
                namen.add(str(job.get("name") or jid))
    return namen


def beoordeel_de_boom(register: list[dict], boom: set[str]) -> list[str]:
    """The offline half: the register against the workflows in this tree."""
    fataal: list[str] = []
    if len(register) < MINIMUM_ROWS:
        fataal.append(
            f"the register holds {len(register)} row(s), below the floor of "
            f"{MINIMUM_ROWS} -- a register that has emptied approves everything in it")
    for rij in register:
        naam = rij.get("name", "<nameless>")
        if not str(rij.get("why", "")).strip():
            fataal.append(f"{naam}: no reason written for what it requires")
        if not rij.get("contexts"):
            fataal.append(f"{naam}: no required context named")
        if not rij.get("from_this_tree"):
            continue
        for context in rij.get("contexts", []):
            if context not in boom:
                fataal.append(
                    f"{naam}: requires {context!r}, which no pull-request job in "
                    f"this tree reports -- that requirement would block every "
                    f"pull request there for good")
    return fataal


# --------------------------------------------------------------------------- #
# what GitHub says
# --------------------------------------------------------------------------- #
def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True,
                          text=True, timeout=120).stdout


def publieke_repos(org: str = ORG) -> list[dict]:
    rauw = gh("api", "--paginate", f"orgs/{org}/repos?type=public&per_page=100")
    # --paginate concatenates one JSON array per page.
    uit: list[dict] = []
    ontleder = json.JSONDecoder()
    tekst, i = rauw.strip(), 0
    while i < len(tekst):
        blok, eind = ontleder.raw_decode(tekst, i)
        uit.extend(blok)
        i = eind
        while i < len(tekst) and tekst[i] in " \n\r\t":
            i += 1
    return [r for r in uit if not r.get("archived")]


def bescherming(repo: str, tak: str) -> dict | None:
    try:
        return json.loads(gh("api", f"repos/{repo}/branches/{tak}/protection"))
    except subprocess.CalledProcessError:
        return None


# How far back the evidence for question 4 is gathered. One commit is too few:
# a head pushed while Actions was disabled -- which happened here on 05-09-2026
# when the budget ran out -- carries no check runs at all, and reading only the
# head would then call every required check "never reported" and fail on it.
COMMITS_TERUG = 5


def groen_gezien(repo: str, tak: str, terug: int = COMMITS_TERUG) -> set[str]:
    """Check names that have reported success on the last few commits of `tak`.

    The evidence for question 4 on a repository this tree does not seed: a check
    that has been green there is a check that arrives.
    """
    namen: set[str] = set()
    try:
        commits = json.loads(gh("api", f"repos/{repo}/commits?sha={tak}&per_page={terug}"))
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return namen
    for commit in commits[:terug]:
        try:
            runs = json.loads(gh("api", f"repos/{repo}/commits/{commit['sha']}/check-runs"))
        except (subprocess.CalledProcessError, KeyError, json.JSONDecodeError):
            continue
        namen |= {r["name"] for r in runs.get("check_runs", [])
                  if r.get("conclusion") == "success"}
    return namen


def beoordeel_repo(rij: dict, prot: dict | None, groen: set[str],
                   ) -> tuple[list[str], list[str]]:
    """One repository against its row. Returns (fatal, reported)."""
    naam = rij["name"]
    wacht = str(rij.get("pending", "")).strip()
    fataal: list[str] = []
    gemeld: list[str] = []

    if prot is None:
        fataal.append(f"{naam}: `{rij['branch']}` carries no branch protection at all")
        return fataal, gemeld

    if prot.get("allow_force_pushes", {}).get("enabled"):
        fataal.append(f"{naam}: force-pushes to `{rij['branch']}` are allowed")
    if prot.get("allow_deletions", {}).get("enabled"):
        fataal.append(f"{naam}: `{rij['branch']}` can be deleted")
    if not prot.get("required_linear_history", {}).get("enabled"):
        fataal.append(f"{naam}: `{rij['branch']}` does not require a linear history")

    verplicht = set(prot.get("required_status_checks", {}).get("contexts", []) or [])
    gewenst = set(rij.get("contexts", []))

    # Question 4, for a repository this tree does not seed: never require a
    # context that repository has never reported.
    if not rij.get("from_this_tree"):
        for context in sorted(verplicht - groen):
            fataal.append(
                f"{naam}: requires {context!r}, which has never been reported "
                f"green there -- every pull request waits for it")

    if verplicht == gewenst:
        if wacht:
            fataal.append(
                f"{naam}: the register still calls this pending, and the checks "
                f"it names are required -- remove the `pending` reason")
        return fataal, gemeld

    ontbreekt = sorted(gewenst - verplicht)
    teveel = sorted(verplicht - gewenst)
    boodschap = f"{naam}: required checks are {sorted(verplicht) or 'none'}"
    if ontbreekt:
        boodschap += f", the register asks for {ontbreekt}"
    if teveel:
        boodschap += f", and {teveel} is required without a row"
    (gemeld if wacht and not teveel else fataal).append(boodschap)
    return fataal, gemeld


def beoordeel_dekking(register: list[dict], repos: list[dict]) -> list[str]:
    bekend = {r["name"] for r in register} | set(BUITEN)
    return [f"{r['full_name']}: public and in no row of the register -- a new "
            f"repository stands unprotected until somebody looks"
            for r in repos if r["full_name"] not in bekend]


# --------------------------------------------------------------------------- #
# setting it
# --------------------------------------------------------------------------- #
def zet(rij: dict, contexts: list[str], droog: bool) -> None:
    """PUT the protection this row describes.

    `enforce_admins` stays off deliberately: with one maintainer, turning it on
    means the owner cannot repair a broken `main` without first switching the
    protection off, and #230's history rewrite needs exactly that. It is a
    decision for the day there is a second maintainer, and it is written here so
    that day has something to change.
    """
    lichaam = {
        "required_status_checks": {"strict": False, "contexts": contexts},
        "enforce_admins": False,
        "required_pull_request_reviews": None,
        "restrictions": None,
        "required_linear_history": True,
        "allow_force_pushes": False,
        "allow_deletions": False,
    }
    doel = f"repos/{rij['name']}/branches/{rij['branch']}/protection"
    print(f"  PUT {doel}\n      contexts: {contexts}")
    if droog:
        print("      (dry run -- drop --dry-run to send it)")
        return
    subprocess.run(["gh", "api", "--method", "PUT", doel, "--input", "-"],
                   input=json.dumps(lichaam), text=True, check=True,
                   capture_output=True, timeout=120)
    print("      set")


# --------------------------------------------------------------------------- #
def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--tree-only", action="store_true",
                   help="check the register against this tree and stop; what a "
                        "runner without an administration token can honestly do")
    p.add_argument("--apply", action="store_true",
                   help="set the protection the register describes")
    p.add_argument("--dry-run", action="store_true",
                   help="with --apply: print what would be sent and send nothing")
    p.add_argument("--repo", help="limit --apply to one repository")
    args = p.parse_args()

    register = lees_register()
    fataal = beoordeel_de_boom(register, contexts_uit_de_boom())
    for r in fataal:
        print(f"[protection] FATAL: {r}")
    if args.tree_only:
        if fataal:
            return 1
        namen = [r["name"] for r in register]
        print(f"[protection] OK (tree only): {len(register)} row(s) -- {namen} -- "
              f"every context they require is a job a pull request here reports. "
              f"The repositories themselves were not read; run without "
              f"--tree-only where `gh` is authenticated.")
        return 0

    try:
        repos = publieke_repos()
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired) as e:
        print(f"[protection] SKIPPED (not a pass): could not reach the GitHub API "
              f"({type(e).__name__}), so no repository was read. The register "
              f"itself was checked against this tree and is "
              f"{'not ' if fataal else ''}consistent.")
        return 1 if fataal else 0

    fataal += beoordeel_dekking(register, repos)
    gemeld: list[str] = []
    for rij in register:
        prot = bescherming(rij["name"], rij["branch"])
        groen = groen_gezien(rij["name"], rij["branch"])
        f, g = beoordeel_repo(rij, prot, groen)
        fataal += f
        gemeld += g
        if args.apply and (not args.repo or args.repo == rij["name"]):
            toegestaan = [c for c in rij["contexts"]
                          if rij.get("from_this_tree") or c in groen]
            geweigerd = [c for c in rij["contexts"] if c not in toegestaan]
            if geweigerd:
                print(f"[protection] REFUSED for {rij['name']}: {geweigerd} has "
                      f"never been reported there; requiring it would block every "
                      f"pull request. Open one that runs it first.")
            else:
                zet(rij, toegestaan, droog=args.dry_run)

    for r in gemeld:
        print(f"[protection] pending: {r}")
    for r in fataal:
        print(f"[protection] FATAL: {r}")
    if fataal:
        return 1
    print(f"[protection] OK: {len(repos)} public repo(s) read, {len(register)} "
          f"in the register, every default branch protected against force-pushes "
          f"and deletion with a linear history, {len(gemeld)} requirement(s) "
          f"named here and not yet set.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
