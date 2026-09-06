#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The protection guard refuses what it should, and only that (#233).

Two halves, and the second is the one that keeps this honest. The fixtures below
prove the verdicts -- an unprotected branch, a force-push left open, a required
check nobody reports -- and the last block runs the real register against the
real workflow directory, so renaming the job in
`.github/workflows/public-pull-request.yml` or deleting the workflow turns this
red rather than silently emptying what the register promises.

No network. Every judgement in the guard that needs GitHub takes what GitHub
said as an argument, which is why it can be tested at all.
"""
from __future__ import annotations

import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import public_repos_are_protected as guard  # noqa: E402

failures: list[str] = []
ran = 0


def case(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
    if not ok:
        failures.append(what)


def rij(**kw) -> dict:
    basis = dict(name="org/repo", branch="main", from_this_tree=False,
                 contexts=["check"], why="because", pending="")
    basis.update(kw)
    return basis


def prot(force=False, delete=False, linear=True, contexts=()) -> dict:
    return {
        "allow_force_pushes": {"enabled": force},
        "allow_deletions": {"enabled": delete},
        "required_linear_history": {"enabled": linear},
        "required_status_checks": {"contexts": list(contexts)},
    }


def workflow(pad: pathlib.Path, naam: str, tekst: str) -> None:
    (pad / naam).write_text(tekst, encoding="utf-8")


def main() -> int:
    # --- what GitHub says about one repository -------------------------------
    fataal, _ = guard.beoordeel_repo(rij(), None, set())
    case("a branch with no protection at all is fatal", len(fataal) == 1, str(fataal))

    fataal, _ = guard.beoordeel_repo(rij(pending="waiting"), prot(force=True), {"check"})
    case("a force-push left open is fatal even while the row is pending",
         any("force-push" in f for f in fataal), str(fataal))

    fataal, _ = guard.beoordeel_repo(rij(pending="waiting"), prot(delete=True), {"check"})
    case("a deletable default branch is fatal",
         any("deleted" in f for f in fataal), str(fataal))

    fataal, _ = guard.beoordeel_repo(rij(pending="waiting"), prot(linear=False), {"check"})
    case("a branch that allows merge commits is fatal",
         any("linear" in f for f in fataal), str(fataal))

    # --- the pending window, in both directions ------------------------------
    fataal, gemeld = guard.beoordeel_repo(
        rij(pending="set after the seeding"), prot(), {"check"})
    case("a pending row with nothing required yet is reported, not failed",
         not fataal and len(gemeld) == 1, f"{fataal} {gemeld}")

    fataal, gemeld = guard.beoordeel_repo(rij(pending=""), prot(), {"check"})
    case("the same gap without a reason is fatal",
         len(fataal) == 1 and not gemeld, f"{fataal} {gemeld}")

    # THE DIRECTION THAT KEEPS THE REGISTER ALIVE. Once the requirement is set,
    # the reason it was not set has to go with it; a register that only grows
    # describes a repository that no longer exists.
    fataal, _ = guard.beoordeel_repo(
        rij(pending="set after the seeding"), prot(contexts=["check"]), {"check"})
    case("a row still pending after the checks are required is fatal",
         any("pending" in f for f in fataal), str(fataal))

    fataal, gemeld = guard.beoordeel_repo(rij(), prot(contexts=["check"]), {"check"})
    case("the settled state passes clean", not fataal and not gemeld,
         f"{fataal} {gemeld}")

    # --- the trap: requiring a check that never arrives ----------------------
    fataal, _ = guard.beoordeel_repo(
        rij(contexts=["check"]), prot(contexts=["check", "ghost"]), {"check"})
    case("a required context nobody has ever reported green is fatal",
         any("ghost" in f and "waits" in f for f in fataal), str(fataal))

    fataal, _ = guard.beoordeel_repo(
        rij(from_this_tree=True, contexts=["check"]), prot(contexts=["check"]), set())
    case("a repository seeded from this tree is judged by the tree, not by its "
         "history of green runs", not fataal, str(fataal))

    # --- coverage of the organisation ----------------------------------------
    repos = [{"full_name": "org/repo"}, {"full_name": "org/new"}]
    case("a public repository in no row is fatal",
         len(guard.beoordeel_dekking([rij()], repos)) == 1)
    case("and a repository with a row is not",
         guard.beoordeel_dekking([rij(), rij(name="org/new")], repos) == [])

    # --- the register against a tree -----------------------------------------
    with tempfile.TemporaryDirectory() as d:
        flows = pathlib.Path(d)
        workflow(flows, "pr.yml", "on:\n  pull_request:\njobs:\n"
                                  "  ident:\n    name: A named job\n    steps: []\n"
                                  "  bare:\n    steps: []\n")
        workflow(flows, "push.yml", "on:\n  push:\n    branches: [main]\n"
                                    "jobs:\n  only-on-push:\n    steps: []\n")
        boom = guard.contexts_uit_de_boom(flows)
        case("a job's name is the context, not its id", "A named job" in boom, str(boom))
        case("a job without a name reports its id", "bare" in boom, str(boom))
        case("a workflow no pull request triggers contributes nothing",
             "only-on-push" not in boom, str(boom))

        goed = [rij(from_this_tree=True, contexts=["A named job"])] * 4
        case("a register whose contexts the tree provides passes",
             guard.beoordeel_de_boom(goed, boom) == [])
        slecht = [rij(from_this_tree=True, contexts=["Renamed job"])] * 4
        case("a context no pull-request job in the tree reports is fatal",
             any("block every" in f for f in guard.beoordeel_de_boom(slecht, boom)),
             str(guard.beoordeel_de_boom(slecht, boom)))
        case("a row without a reason is fatal",
             any("no reason" in f
                 for f in guard.beoordeel_de_boom([rij(why="")] * 4, boom)))
        case("a register below the floor is fatal -- an empty one approves "
             "everything in it",
             any("floor" in f for f in guard.beoordeel_de_boom([rij()], boom)))

    # --- and the same question asked of the real thing -----------------------
    #
    # The fixtures above prove the verdicts. This proves they are pointed at
    # something: rename the job in .github/workflows/public-pull-request.yml,
    # drop the workflow, or empty the register, and this goes red here rather
    # than on the day a requirement is set against a check that does not exist.
    echt = guard.lees_register()
    case("the register in this tree holds at least the four public repositories",
         len(echt) >= guard.MINIMUM_ROWS, f"{len(echt)} row(s)")
    case("and every context it asks this tree for is a job a pull request runs",
         guard.beoordeel_de_boom(echt, guard.contexts_uit_de_boom()) == [],
         str(guard.beoordeel_de_boom(echt, guard.contexts_uit_de_boom())))
    uit_de_boom = {c for r in echt if r.get("from_this_tree")
                   for c in r["contexts"]}
    case("and the seeded repository requires more than nothing",
         len(uit_de_boom) >= 2, str(uit_de_boom))

    print(f"[test-protection] {ran} case(s), {len(failures)} failure(s)")
    if failures:
        for f in failures:
            print(f"  FAILED: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
