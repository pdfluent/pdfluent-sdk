#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Does the identity guard refuse the right addresses, and only those?

Both directions, and the second is the one that matters here. A guard that
refuses everything gets switched off within a week, and this one sits in a
pre-commit hook where being wrong is expensive: it stands between the writer and
every commit they make.

The end-to-end half runs against a repository built in a temporary directory.
That is deliberate -- the range logic and the floor are where this guard can go
quietly blind, and a table of strings cannot exercise either.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import os
import subprocess
import sys
import tempfile
from pathlib import Path

HIER = Path(__file__).resolve().parent
GUARD = HIER / "commits_use_the_noreply_alias.py"

sys.path.insert(0, str(HIER))
from commits_use_the_noreply_alias import (  # noqa: E402
    ALIAS,
    CUTOVER,
    LEGACY_BUDGET,
    is_allowed,
)

# A date safely on either side of the cutover, in the form `git commit --date`
# takes. The past one stands for the published history the owner has not decided
# about yet; the later one for anything written from here on. Both are written
# out rather than derived from CUTOVER, so moving the cutover cannot move the
# fixtures with it and leave the test proving nothing.
VOOR_CUTOVER = "2026-01-15T09:00:00+01:00"
NA_CUTOVER = "2026-12-01T12:00:00+01:00"

ACCEPT = [
    ("the account's own alias", ALIAS),
    ("the login-only noreply form", "jasperdew@users.noreply.github.com"),
    ("another account's alias", "99+someone@users.noreply.github.com"),
    ("GitHub's web-flow committer", "noreply@github.com"),
    ("release tooling", "noreply@pdfluent.com"),
    ("the CI gate account", "gate-ci@pdfluent.com"),
    ("the same address with stray whitespace", f"  {ALIAS}  "),
    ("the noreply domain in capitals", ALIAS.upper()),
]

REFUSE = [
    # The placeholder that 219 commits in this repository carry, because one
    # worktree was configured with it and nobody looked again.
    ("the t@t placeholder", "t@t"),
    ("a personal mailbox", "someone@example.com"),
    ("a personal mailbox at a company domain", "firstname@example.co.uk"),
    ("nothing at all", ""),
    ("whitespace only", "   "),
    # The domain has to end the address. Without the anchor a look-alike host
    # walks straight through, and that is the whole weight this pattern carries.
    ("a look-alike host", "harvest@users.noreply.github.com.example.com"),
    ("the domain as a prefix", "users.noreply.github.com@example.com"),
]


def _git_env() -> dict[str, str]:
    """git without the caller's GIT_* variables."""
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a
    # fixture READING the real repository, not WRITING to the real config.
    # A fixture's `git config user.email t@t` reached a real worktree that
    # way and stamped a test identity onto every later rebase there. (#297)
    return sealed_env()


def _git(wd: Path, *args: str, extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(wd),
         # No hook of the outer clone, and no signing: both would make this
         # test's outcome depend on the machine it runs on.
         "-c", "core.hooksPath=/nonexistent",
         "-c", "commit.gpgsign=false",
         *args],
        capture_output=True, text=True, env={**_git_env(), **(extra_env or {})},
    )


def _commit(wd: Path, email: str, subject: str, datum: str = NA_CUTOVER,
            committer: str | None = None) -> None:
    """One empty commit. `committer` splits the two identities apart.

    Git has no `-c committer.email`, so the committer half only reaches git
    through the environment -- which is also how it drifts apart from the author
    in real life: a rebase, a cherry-pick, a `git commit --author`. A guard
    reading only `%ae` misses it, and master alone has 253 commits where the two
    differ.
    """
    r = _git(wd, "-c", f"user.email={email}", "-c", "user.name=test",
             "commit", "--allow-empty", "--no-verify", "--date", datum, "-m", subject,
             extra_env=({"GIT_COMMITTER_EMAIL": committer, "GIT_COMMITTER_NAME": "test"}
                        if committer else None))
    if r.returncode != 0:
        raise SystemExit(f"[test-commit-identity] could not build the fixture: {r.stderr}")


def _rev(wd: Path, ref: str) -> str:
    return _git(wd, "rev-parse", ref).stdout.strip()


def _run(wd: Path, *args: str, extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    # GITHUB_EVENT_PATH would send the guard at this run's own event payload
    # instead of at the fixture, and GITHUB_HEAD_REF/GITHUB_REF_NAME would tell
    # it which branch it is on -- which is this run's branch, not the fixture's.
    # All three are taken out, and put back deliberately where the case is about
    # the branch.
    weg = {"GITHUB_EVENT_PATH", "GITHUB_HEAD_REF", "GITHUB_REF_NAME"}
    omgeving = {k: v for k, v in os.environ.items() if k not in weg}
    return subprocess.run(
        [sys.executable, str(GUARD), *args],
        cwd=str(wd), capture_output=True, text=True,
        env={**omgeving, **(extra_env or {})},
    )


def einde_tot_eind(fouten: list[str]) -> None:
    """A real range over a real repository: clean, dirty, and empty."""
    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            fouten.append("could not create the fixture repository")
            return

        _commit(wd, ALIAS, "chore: the first commit")
        basis = _rev(wd, "HEAD")
        _commit(wd, ALIAS, "chore: a second, also aliased")
        _commit(wd, "noreply@pdfluent.com", "chore: one from release tooling")
        schoon = _rev(wd, "HEAD")

        r = _run(wd, "--range", f"{basis}..{schoon}")
        if r.returncode != 0:
            fouten.append(
                f"refused a range whose every identity is an alias "
                f"(exit {r.returncode}): {r.stderr.strip()[:200]}"
            )

        _commit(wd, "someone@example.com", "chore: one from a personal address")
        vuil = _rev(wd, "HEAD")
        r = _run(wd, "--range", f"{basis}..{vuil}")
        if r.returncode == 0:
            fouten.append("accepted a range containing a personal address")
        elif "someone@example.com" not in r.stderr:
            fouten.append("refused the range without naming the offending address")

        # A commit carries two identities and both are published. Checking only
        # the author is a mutation this test did not catch until 30-08-2026, and
        # it is the likelier of the two to go wrong: the author survives a
        # rebase, the committer is rewritten to whoever ran it.
        _commit(wd, ALIAS, "chore: aliased author, personal committer",
                committer="someone@example.com")
        gesplitst = _rev(wd, "HEAD")
        r = _run(wd, "--range", f"{vuil}..{gesplitst}")
        if r.returncode == 0:
            fouten.append("accepted a commit whose committer is a personal address")
        vuil = gesplitst

        # The floor. An empty range is the state where this guard reports success
        # over nothing, and it has to be a failure rather than a pass.
        r = _run(wd, "--range", f"{vuil}..{vuil}")
        if r.returncode == 0:
            fouten.append("passed an empty range instead of failing on the floor")
        elif "floor" not in r.stderr.lower():
            fouten.append("failed on an empty range without naming the floor")

        # THE CUTOVER, AND THE BUDGET THAT MAKES IT FINITE.
        #
        # A date on its own fails open in two directions: everything old reads as
        # published history, and `git commit --date=<something old>` makes a
        # commit old on request. So a pre-cutover offender is refused like any
        # other unless the branch it is on is named in LEGACY_BUDGET.
        #
        # Both halves are asserted, because only the pair is the rule. If the
        # budget stopped being consulted the first case would still pass; if the
        # cutover stopped being consulted the second one would.
        _commit(wd, "someone@example.com", "chore: an old one", datum=VOOR_CUTOVER)
        oud = _rev(wd, "HEAD")

        r = _run(wd, "--range", f"{vuil}..{oud}",
                 extra_env={"GITHUB_HEAD_REF": "feature/no-budget-here"})
        if r.returncode == 0:
            fouten.append(
                "accepted a pre-cutover personal address on a branch with no "
                "budget -- which is `git commit --date=<old>` walking through"
            )
        elif "budget" not in r.stderr.lower():
            fouten.append("refused it without saying the budget was the reason")

        tak, budget = next(iter(LEGACY_BUDGET.items()))
        if budget < 1:
            fouten.append(f"LEGACY_BUDGET[{tak}] is {budget}, so this case proves nothing")
        r = _run(wd, "--range", f"{vuil}..{oud}", extra_env={"GITHUB_HEAD_REF": tak})
        if r.returncode != 0:
            fouten.append(
                f"refused a pre-cutover address on `{tak}`, which has a budget of "
                f"{budget}: {r.stderr.strip()[:200]}"
            )

        # And the cutover itself still separates the two: the same address,
        # authored after it, is a new mistake on any branch including the one
        # with a budget.
        _commit(wd, "someone@example.com", "chore: a new one", datum=NA_CUTOVER)
        if _run(wd, "--range", f"{oud}..HEAD",
                extra_env={"GITHUB_HEAD_REF": tak}).returncode == 0:
            fouten.append("accepted a personal address authored after the cutover")

        # And the pre-commit mode, over the same repository, both ways.
        omgeving = _git_env()
        for email, moet_falen in ((ALIAS, False), ("t@t", True)):
            _git(wd, "config", "user.email", email)
            r = subprocess.run(
                [sys.executable, str(GUARD), "--pending"],
                cwd=str(wd), capture_output=True, text=True, env=omgeving,
            )
            if moet_falen and r.returncode == 0:
                fouten.append(f"--pending accepted a commit authored as {email}")
            if not moet_falen and r.returncode != 0:
                fouten.append(f"--pending refused a commit authored as the alias: "
                              f"{r.stderr.strip()[:200]}")

        # THE TWO ROUTES ROUND THE CONFIG. A commit's identity does not have to
        # come from `user.email`: the environment sets it directly, and `git -c
        # user.email=...` reaches a hook as GIT_CONFIG_PARAMETERS. Both beat a
        # check that reads the config, and both beat a check that strips every
        # GIT_* variable before asking git -- which is what this guard did until
        # 31-08-2026, when `git -c user.email=<personal> commit` walked past the
        # hook and produced a commit carrying that address.
        #
        # The config here is left on the alias on purpose, so the only thing
        # that can make these fail is the override being seen.
        _git(wd, "config", "user.email", ALIAS)
        for naam, extra in (
            ("GIT_AUTHOR_EMAIL", {"GIT_AUTHOR_EMAIL": "someone@example.com"}),
            ("GIT_COMMITTER_EMAIL", {"GIT_COMMITTER_EMAIL": "someone@example.com"}),
            ("git -c user.email",
             {"GIT_CONFIG_PARAMETERS": "'user.email=someone@example.com'"}),
        ):
            r = subprocess.run(
                [sys.executable, str(GUARD), "--pending"],
                cwd=str(wd), capture_output=True, text=True,
                env={**omgeving, **extra},
            )
            if r.returncode == 0:
                fouten.append(
                    f"--pending accepted a commit whose address comes from {naam}, "
                    "which is the route that beats reading the config"
                )

        # And the mirror image: the location variables still have to go, or a
        # hook's GIT_DIR sends every git call in here at the real repository.
        r = subprocess.run(
            [sys.executable, str(GUARD), "--pending"],
            cwd=str(wd), capture_output=True, text=True,
            env={**omgeving, "GIT_DIR": "/nonexistent/objects", "GIT_WORK_TREE": "/nonexistent"},
        )
        if r.returncode != 0:
            fouten.append(
                "--pending broke when handed a hook's GIT_DIR/GIT_WORK_TREE; those "
                "have to be stripped (#240)"
            )


def dichtstbijzijnde_basis(fouten: list[str]) -> None:
    """Two remotes that disagree: the range is measured from the nearer one.

    This checkout has `origin` on GitLab and `github` on GitHub, and on
    31-08-2026 the GitLab mirror was 99 commits behind. Taking the first
    candidate that resolved made a 4-commit branch look like a 103-commit one
    and dragged 52 published offenders into a range with a budget of zero.
    """
    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            fouten.append("could not create the base fixture")
            return

        _commit(wd, ALIAS, "chore: one")
        ver = _rev(wd, "HEAD")
        _commit(wd, ALIAS, "chore: two")
        _commit(wd, ALIAS, "chore: three")
        dichtbij = _rev(wd, "HEAD")
        _commit(wd, ALIAS, "chore: four")
        _commit(wd, ALIAS, "chore: five")

        # BOTH ARRANGEMENTS, AND THAT IS THE POINT.
        #
        # With the near base on whichever remote happens to be tried first, a
        # guard that simply takes the first candidate is right by luck and the
        # case proves nothing. Running it the other way round too means no
        # candidate order can pass both -- only actually choosing the nearest
        # does.
        for stale_ref, verse_ref in (("origin/master", "github/master"),
                                     ("github/master", "origin/master")):
            _git(wd, "update-ref", f"refs/remotes/{stale_ref}", ver)
            _git(wd, "update-ref", f"refs/remotes/{verse_ref}", dichtbij)

            r = _run(wd)
            if r.returncode != 0:
                fouten.append(
                    f"the derived range failed on a clean fixture with the near "
                    f"base on {verse_ref}: {r.stderr.strip()[:200]}"
                )
            elif ver in r.stdout:
                fouten.append(
                    f"measured from the stale {stale_ref} rather than the nearer "
                    f"{verse_ref}, which is the bug that spends a zero budget on "
                    "published history"
                )
            elif dichtbij not in r.stdout:
                fouten.append(
                    f"did not measure from either candidate base: {r.stdout.strip()[:200]}"
                )


def main() -> int:
    fouten: list[str] = []

    for naam, adres in ACCEPT:
        if not is_allowed(adres):
            fouten.append(f"refused {naam}: {adres!r}")
    for naam, adres in REFUSE:
        if is_allowed(adres):
            fouten.append(f"accepted {naam}: {adres!r}")

    einde_tot_eind(fouten)
    dichtstbijzijnde_basis(fouten)

    if fouten:
        print(f"[test-commit-identity] FAIL: {len(fouten)} case(s):", file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"[test-commit-identity] OK: {len(ACCEPT)} address(es) accepted, "
        f"{len(REFUSE)} refused, and the range, the nearest base, the floor, "
        "the cutover, the legacy budget and the pre-commit mode answer over a "
        "real repository."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
