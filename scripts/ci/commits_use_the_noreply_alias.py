#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""No new commit publishes a personal address.

A commit carries two identities, author and committer, and both are readable by
anyone who can read the repository -- no login, no clone, one API call. This
repository is going public (LC10) and the editor repository already is, so every
address in a commit made from today on is an address published on purpose.

Harvesting them from public history is routine and automated. GitHub's answer is
a noreply alias, and it is derived rather than invented:

    <numeric id>+<login>@users.noreply.github.com

Both halves come from `gh api user --jq '{id: .id, login: .login}'`. It is the
default for commits made through the web interface and off for anything pushed
from a local git that carries a real address -- which is the state this
repository was in.

WHAT THIS ACCEPTS, AND WHY IT IS AN ALLOWLIST

A denylist would have to name the address it refuses, and this file is published
with the tree. Naming it here would publish it once more, in the one place
guaranteed to be read. So the rule is stated the other way round: an address is
either a noreply alias, or one of the automation addresses below, or it does not
go in a commit. That also catches the placeholder `t@t`, which 219 commits in
this repository carry because one worktree was configured with it and nobody saw
it again.

TWO LAYERS, AND THE SECOND IS THE ONE THAT BITES

    1. `scripts/git-hooks/pre-commit` refuses before the commit exists, which is
       the last moment repair is free.
    2. This guard runs in CI over the commits a push or a pull request brings.
       A hook lives in `.git/`, does not travel with a clone, and `--no-verify`
       turns it off; CI is where that gets caught.

WHAT THIS DOES NOT DO

It says nothing about the history that is already published. That is a separate
decision, written up for the owner under #261, and his to take -- rewriting
thousands of commits is not a guard's business, and doing it uninvited is
explicitly not allowed. It is also deliberately deferred: rewriting master
underneath an open pull request of a few hundred commits breaks that pull
request, so the rewrite waits until #1543 is merged or closed.

Exit codes:
    0  every identity inspected is an alias
    1  an identity is not, or the range inspected was empty
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime

# The alias for this account, constructed from `gh api user`. It is written out
# rather than fetched so the guard needs no network and no `gh`: a check that
# only works when an API answers is a check that skips on the days it matters.
ALIAS = "10383561+jasperdew@users.noreply.github.com"

# Anything at GitHub's noreply domain. The account-scoped form above is the one
# to configure, but the domain as a whole is non-personal by construction, and
# pinning only one string would fail on a legitimate web-UI commit or on the day
# the login changes.
NOREPLY = re.compile(r"^[^@\s]+@users\.noreply\.github\.com$", re.I)

# Automation that already appears in this history and is not a person. Each one
# is an address that belongs to a machine or to a role, never to a mailbox
# someone reads.
AUTOMATION = {
    "noreply@github.com",        # GitHub's web-flow committer on merges
    "noreply@pdfluent.com",      # release tooling
    "gate-ci@pdfluent.com",      # the CI gate account
}

# FLOOR: commits inspected >= 1 -- an empty range is the failure mode this whole
# check has to survive. `git log` over a reference that does not resolve prints
# nothing and exits 0, so a broken range reads exactly like a clean one: "OK, 0
# commits". On a shallow CI checkout that is the normal state, not the rare one.
MIN_COMMITS = 1

# The moment the rule starts: the minute this guard was written, so there is no
# window between the rule existing and the rule applying.
#
# Commits authored before it are the published history, and that is a decision
# for the owner rather than a build to break. 3458 of the 3549 commits on master
# carry an address and 219 across all branches carry the placeholder, so a guard
# without a cutover fails on every branch from the hour it lands -- and a guard
# that always fails is switched off within the week, which leaves the future
# unguarded to make a point about the past.
#
# AUTHOR DATE, NOT COMMITTER DATE. A rebase rewrites the committer date to now
# and keeps the author date. Grandfathering on the committer date would turn 300
# old commits red the moment somebody rebased this branch, for a reason that has
# nothing to do with what they changed.
#
# AN INSTANT, NOT A DAY. Comparing `%aI` strings against a bare date is wrong
# inside the cutover day itself: `...T19:30:00+00:00` sorts before
# `...T21:00:00+02:00` as text and is later in fact. `%at` is a Unix timestamp
# and carries no zone at all, so the comparison is the one being reasoned about.
CUTOVER = "2026-08-30T18:03:00+00:00"
CUTOVER_EPOCH = int(datetime.fromisoformat(CUTOVER).timestamp())

# A CUTOVER ON ITS OWN IS NOT A GUARD, IT IS AN AMNESTY WITH A DATE ON IT.
#
# Measured on 31-08-2026, on the branch behind pull request #1543: of the 319
# commits `chore/test-reachability-gate` adds to master, 248 carry a pre-rule
# address and every one of them is older than the cutover. The guard passed.
# A rule that does not apply to 78% of what it is pointed at is not a rule that
# branch has to obey, and reading the green tick as "this branch is clean" is
# reading something the check never said.
#
# It is also the hole. `git commit --date=2026-01-01` sets the author date to
# whatever it is told, and the cutover then waves the commit through -- the one
# thing this file exists to stop, defeated by an argument anyone can type.
#
# So the amnesty is enumerated instead of open-ended. Every branch gets a budget
# of zero pre-cutover offenders unless it is named below, with the number
# measured on the day and the reason it is not zero. Over budget is a failure
# whatever the dates claim, which closes the backdating hole at the same time:
# a fabricated old commit still counts against a number that is already spent.
#
# The numbers may go down and never up. Deleting a line here is the goal, and
# it happens when that branch's history is rewritten -- deferred until #1543 is
# merged or closed, because rewriting the base under an open pull request of
# 319 commits breaks it.
LEGACY_BUDGET: dict[str, int] = {
    # #1543, DRAFT. 248 of 319 on 31-08-2026. Rewriting them is a force-push of
    # the branch a 319-commit pull request is built on.
    "chore/test-reachability-gate": 248,
}

# Everything not named above. Zero: a branch cut after the rule landed has no
# pre-rule commits of its own, so any it presents were either backdated or
# dragged in by a range that reaches further back than the branch does.
DEFAULT_BUDGET = 0


def _branch() -> str:
    """The branch this check is about.

    In Actions on a pull request, GITHUB_REF_NAME is `1588/merge` and says
    nothing; GITHUB_HEAD_REF is the branch. On a push there is no HEAD_REF and
    REF_NAME is the branch. Locally it is simply the checkout.
    """
    for var in ("GITHUB_HEAD_REF", "GITHUB_REF_NAME"):
        waarde = (os.environ.get(var) or "").strip()
        if waarde:
            return waarde
    r = _git("rev-parse", "--abbrev-ref", "HEAD")
    return r.stdout.strip() if r.returncode == 0 else ""


# The GIT_* variables that say WHICH repository git should work on. A hook is
# handed these as absolute paths and every subprocess inherits them, so git then
# works on the caller's repository instead of the one it was pointed at. On
# 25-08-2026 that set `core.bare = true` on the real repository and stopped all
# thirty worktrees (#240).
#
# Only these. Stripping every GIT_* variable is the obvious move and it is wrong
# here, because two of the ones it takes out are the identity being checked:
# GIT_AUTHOR_EMAIL/GIT_COMMITTER_EMAIL set it directly, and GIT_CONFIG_PARAMETERS
# is how `git -c user.email=...` reaches a hook. Strip those and the hook reads
# the config while the commit is built from the override -- it prints OK and the
# address lands anyway. Measured on 31-08-2026: `git -c user.email=<personal>
# commit` passed the hook and produced a commit carrying that address.
LOCATIE_VARIABELEN = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_PREFIX",
)


def _git_env() -> dict[str, str]:
    """The environment for git calls, minus the repository-location variables."""
    return {k: v for k, v in os.environ.items() if k not in LOCATIE_VARIABELEN}


def _git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, env=_git_env()
    )


def is_allowed(email: str) -> bool:
    """Is this an address that may be published in a commit?"""
    e = (email or "").strip()
    if not e:
        return False
    return bool(NOREPLY.match(e)) or e.lower() in AUTOMATION


def _address(ident: str) -> str:
    """The address out of `Name <addr> 1788113275 +0200`."""
    m = re.search(r"<([^>]*)>", ident)
    return m.group(1) if m else ""


def pending() -> list[tuple[str, str]]:
    """The identities the next commit here would carry, as (role, address).

    `git var` answers with the identity git has resolved from the config in
    force, which is the thing being checked -- reading `user.email` directly
    would miss the per-worktree config layer that `.git/worktrees/*/config.worktree`
    adds when `extensions.worktreeConfig` is on, and it is on here.

    Only the repository-location variables are stripped before this call (see
    LOCATIE_VARIABELEN), so GIT_AUTHOR_EMAIL, GIT_COMMITTER_EMAIL and the
    GIT_CONFIG_PARAMETERS that carries `git -c user.email=...` all still reach
    `git var` -- and `git var` resolves the identity from the same layers the
    commit will be built from. Taking those out is what made this check pass a
    commit it then let through.
    """
    uit = []
    for role, var in (("author", "GIT_AUTHOR_IDENT"), ("committer", "GIT_COMMITTER_IDENT")):
        r = _git("var", var)
        if r.returncode != 0:
            print(
                f"[commit-identity] FATAL: `git var {var}` failed, so the identity "
                f"this commit would carry is unknown. That is not a pass.\n"
                f"  {r.stderr.strip()}",
                file=sys.stderr,
            )
            sys.exit(1)
        uit.append((role, _address(r.stdout.strip())))
    return uit


def _event_range() -> str | None:
    """The range this GitHub Actions run introduces, from the event payload.

    A pull request brings `base.sha..head.sha`; a push brings `before..after`.
    Deriving it from the payload rather than from `origin/master` matters on a
    push to master, where merge-base with master is HEAD and the derived range
    would be empty -- the exact state the floor below refuses.
    """
    pad = os.environ.get("GITHUB_EVENT_PATH")
    if not pad or not os.path.isfile(pad):
        return None
    try:
        with open(pad, encoding="utf-8", errors="replace") as f:
            event = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        print(f"[commit-identity] the event payload could not be read: {e}", file=sys.stderr)
        return None

    pr = event.get("pull_request") or {}
    basis = (pr.get("base") or {}).get("sha")
    kop = (pr.get("head") or {}).get("sha")
    if basis and kop:
        return f"{basis}..{kop}"

    voor, na = event.get("before"), event.get("after")
    # A branch's first push reports an all-zero `before`; there is no range then.
    if voor and na and set(voor) != {"0"}:
        return f"{voor}..{na}"
    return None


def _derived_range() -> tuple[str, list[str]]:
    """A range for local use: what this branch adds on top of master.

    Falls back to HEAD alone when the branch adds nothing, which is the ordinary
    state on master and must not read as a broken range.
    """
    # THE TIGHTEST OF THE CANDIDATES, NOT THE FIRST ONE THAT RESOLVES.
    #
    # This checkout has two remotes and they do not agree: on 31-08-2026
    # `origin/master` (the GitLab mirror) was 99 commits behind `github/master`,
    # so taking the first candidate made this branch look like it added 103
    # commits when it adds 4 -- and 52 of the 99 it dragged in are published
    # master history that no branch is answerable for. With the legacy budget
    # below that is not cosmetic: the wrong base spends a budget of zero on
    # commits the branch never made.
    #
    # The branch adds what the nearest base says it adds, so: try them all, keep
    # the smallest non-empty answer.
    kandidaten: list[tuple[int, str, list]] = []
    for basis in ("github/master", "origin/master", "master", "github/main",
                  "origin/main", "main"):
        r = _git("merge-base", basis, "HEAD")
        if r.returncode != 0:
            continue
        bereik = f"{r.stdout.strip()}..HEAD"
        commits = _commits(bereik)
        if commits:
            kandidaten.append((len(commits), bereik, commits))
    if kandidaten:
        _, bereik, commits = min(kandidaten, key=lambda k: k[0])
        return bereik, commits
    return "HEAD (this branch adds nothing on top of master)", _commits("-1", "HEAD")


def _commits(*rev: str) -> list[tuple[str, str, str, str, str]]:
    """(sha, author address, committer address, author epoch, subject)."""
    r = _git("log", "--format=%H%x1f%ae%x1f%ce%x1f%at%x1f%s", *rev)
    if r.returncode != 0:
        return []
    uit = []
    for regel in r.stdout.splitlines():
        velden = regel.split("\x1f")
        if len(velden) == 5:
            uit.append(tuple(velden))  # type: ignore[arg-type]
    return uit


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--pending", action="store_true",
        help="check the identity the next commit would carry (pre-commit hook mode)",
    )
    p.add_argument("--range", default=None, help="an explicit commit range")
    a = p.parse_args()

    if a.pending:
        fout = [(rol, adres) for rol, adres in pending() if not is_allowed(adres)]
        if fout:
            print(
                "[commit-identity] this commit would publish an address that is "
                "not a noreply alias:\n",
                file=sys.stderr,
            )
            for rol, adres in fout:
                print(f"  {rol}: {adres}", file=sys.stderr)
            print(
                "\nEvery commit is readable by anyone with the repository URL, and "
                "an address\nin one is an address published on purpose.\n\n"
                "Set the alias for this repository:\n"
                f"  git config --local user.email {ALIAS}\n"
                "  git config --local user.name jasperdew\n\n"
                "Repository scope, not --global: it is this repository that "
                "publishes, and\nthe machine's own default is not this "
                "repository's business.",
                file=sys.stderr,
            )
            return 1
        print(f"[commit-identity] OK: this commit would be authored as {ALIAS}")
        return 0

    if a.range:
        bereik, commits = a.range, _commits(a.range)
    elif (uit_event := _event_range()) is not None:
        bereik, commits = uit_event, _commits(uit_event)
    else:
        bereik, commits = _derived_range()

    if len(commits) < MIN_COMMITS:
        print(
            f"[commit-identity] FATAL: `{bereik}` resolved to {len(commits)} "
            f"commit(s), below the floor of {MIN_COMMITS}.\n"
            "An empty range approves every commit in it without reading one, and "
            "prints\nthe same words as a clean range. Usually the checkout is "
            "shallow: this job\nneeds `fetch-depth: 0`.",
            file=sys.stderr,
        )
        return 1

    def vuil(c) -> bool:
        return not is_allowed(c[1]) or not is_allowed(c[2])

    nieuw = [c for c in commits if int(c[3]) >= CUTOVER_EPOCH]
    oud = [c for c in commits if int(c[3]) < CUTOVER_EPOCH]
    oud_vuil = [c for c in oud if vuil(c)]

    tak = _branch()
    budget = LEGACY_BUDGET.get(tak, DEFAULT_BUDGET)
    if len(oud_vuil) > budget:
        print(
            f"[commit-identity] {len(oud_vuil)} commit(s) in {bereik} predate the "
            f"cutover and carry an address that is not a noreply alias. The budget "
            f"for `{tak or '(unknown branch)'}` is {budget}.\n",
            file=sys.stderr,
        )
        for sha, auteur, committer, _datum, onderwerp in oud_vuil[:10]:
            print(f"  {sha[:9]}  {onderwerp[:56]}", file=sys.stderr)
        if len(oud_vuil) > 10:
            print(f"  ... and {len(oud_vuil) - 10} more", file=sys.stderr)
        print(
            "\nA date on its own is an amnesty, not a rule: `git commit "
            "--date=<something old>`\nwalks straight through one. The budget is "
            "what makes the amnesty finite --\nit is spent, and it may go down "
            "and never up.\n\n"
            "If these are genuinely pre-rule commits on a long-lived branch, they "
            "belong in\nLEGACY_BUDGET with the number and the reason. If they are "
            "yours, rewrite the\nidentity on the commits this branch adds:\n"
            "  git rebase --root --exec 'git commit --amend --no-edit --reset-author'\n\n"
            "And check the base: a range measured against a stale mirror drags in "
            "published\nmaster history the branch never made.",
            file=sys.stderr,
        )
        return 1

    fout = [c for c in nieuw if vuil(c)]
    if fout:
        print(
            f"[commit-identity] {len(fout)} of {len(nieuw)} commit(s) authored on "
            f"or after {CUTOVER} in {bereik} carry an address that is not a "
            "noreply alias:\n",
            file=sys.stderr,
        )
        for sha, auteur, committer, _datum, onderwerp in fout[:20]:
            print(f"  {sha[:9]}  {onderwerp[:56]}", file=sys.stderr)
            if not is_allowed(auteur):
                print(f"      author:    {auteur}", file=sys.stderr)
            if not is_allowed(committer):
                print(f"      committer: {committer}", file=sys.stderr)
        if len(fout) > 20:
            print(f"  ... and {len(fout) - 20} more", file=sys.stderr)
        print(
            "\nFix the configuration first, so the next commit is right:\n"
            f"  git config --local user.email {ALIAS}\n"
            "  git config --local user.name jasperdew\n"
            "  git config --local core.hooksPath .githooks\n\n"
            "Then rewrite the identity on the commits this branch adds -- only\n"
            "those, and only while they are unpushed:\n"
            "  git rebase --root --exec 'git commit --amend --no-edit --reset-author'\n\n"
            "Published history is a separate question and not this guard's. It is\n"
            "written up for the owner under #261.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[commit-identity] OK: {len(nieuw)} commit(s) in {bereik} authored on or "
        f"after {CUTOVER}, every identity a noreply alias. "
        f"{len(oud)} predate the cutover, of which {len(oud_vuil)} carry a "
        f"pre-rule address -- within the budget of {budget} for "
        f"`{tak or '(unknown branch)'}`, and the published-history question (#261) "
        "rather than this one."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
