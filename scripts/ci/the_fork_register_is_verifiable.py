#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""`gelijk_met` must be checkable against a commit, not asserted by hand.

`upstream_has_not_moved_on.py` decides whether a fork is behind by comparing
`gelijk_met` in docs/UPSTREAM_FORKS.toml against crates.io. That makes the
register the only input, and on 31-08-2026 the register was wrong: `hayro-ccitt`
claimed to be level with upstream 0.3.0 while the code sat on the 0.2.0 fork
point, seven commits back. The guard reported it as current, because a guard is
exactly as honest as its baseline and this baseline was typed in by a person.

So each entry now records the upstream COMMIT it corresponds to, and this check
verifies that the version declared in that commit's Cargo.toml is the version
claimed. A typo, a stale entry or an optimistic update fails here instead of
becoming a fact the other guard repeats.

It needs upstream's history. Until 01-09-2026 it expected somebody else to have
provided a clone, and on the CI runner nobody had -- so it exited 3 with an
honest message on every single run. An honest message that never changes is a
red step everybody learns to scroll past, which is the same end state as no
check at all.

So it fetches its own, into a cache directory, and only the objects it needs: a
bare blobless clone, with the handful of Cargo.toml blobs pulled on demand. That
is seconds on a warm cache and well under a minute cold. `HAYRO_CLONE` still
wins if it is set, so a developer with a clone lying around pays nothing.

Exit 3 now means the fetch itself failed -- no network, upstream gone -- which
is a real "cannot check" rather than a missing prerequisite.

WHAT THIS CANNOT SEE, MEASURED RATHER THAN GUESSED

It checks that the version declared at the recorded commit is the version
claimed. It does NOT check that the commit is where our code actually forked,
and those are different questions.

Mutated on 01-09-2026 by moving `pdf-syntax`'s fork point from `3bda7cbc3` to
`758948489` -- the value master carried, ninety commits too late, proven wrong
by content. This check stayed green, because upstream did not bump the manifest
between the two: both commits carry `version = "0.5.0"`, and four commits touch
that Cargo.toml in between without changing it.

So a fork point that is wrong inside one version window is invisible here. That
is the third kind of error in this register's history and the most common: five
of the six wrong entries were of exactly that shape.

What catches it is content -- windows of files byte-identical to upstream,
intersected. Automating that means scoring our tree against every upstream
revision touching each crate (515 for hayro-syntax, 579 for hayro-interpret),
which is minutes rather than the second this check takes, so it belongs in a
scheduled job rather than on every push. Until then the method is manual and
recorded on #262, and this check should not be read as confirming a fork point.

Exit codes:
  0  every entry with a fork point matches the version at that commit
  1  an entry claims a version its fork point does not carry
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTER = ROOT / "docs/UPSTREAM_FORKS.toml"

# These forks come from three different upstreams, which the first version of
# this check did not know: it looked everything up in the hayro clone. `lopdf`
# had no fork point then, so nothing failed. #1614 landed one, and this check
# immediately reported "the register points at a commit that is not there" for
# a commit that is perfectly real -- in another repository.
#
# Keyed by the `upstream` field. The value is the repository and the directory
# the crate sits in, which is a subdirectory in hayro's monorepo and the root
# everywhere else.
UPSTREAMS: dict[str, tuple[str, str]] = {
    "hayro": ("https://github.com/LaurenzV/hayro.git", "hayro"),
    "hayro-syntax": ("https://github.com/LaurenzV/hayro.git", "hayro-syntax"),
    "hayro-interpret": ("https://github.com/LaurenzV/hayro.git", "hayro-interpret"),
    "hayro-jbig2": ("https://github.com/LaurenzV/hayro.git", "hayro-jbig2"),
    "hayro-jpeg2000": ("https://github.com/LaurenzV/hayro.git", "hayro-jpeg2000"),
    "hayro-ccitt": ("https://github.com/LaurenzV/hayro.git", "hayro-ccitt"),
    "lopdf": ("https://github.com/J-F-Liu/lopdf.git", ""),
    "cff-parser": ("https://github.com/jrmuizel/cff-parser.git", ""),
}

# Where to keep the history between runs. Outside the checkout on purpose: a
# runner that reuses its workspace keeps it warm, and one that does not is only
# paying a cold clone.
CACHE_ROOT = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "pdfluent"

# A hayro clone somebody already has wins: developers usually have one from the
# last upgrade, and using it avoids a second copy of 100-odd megabytes.
_EXPLICIT = os.environ.get("HAYRO_CLONE")


import importlib.util as _ilu, pathlib as _pl
_spec = _ilu.spec_from_file_location(
    "fixture_env", _pl.Path(__file__).resolve().parent / "fixture_env.py")
_fx = _ilu.module_from_spec(_spec)
_spec.loader.exec_module(_fx)


def de_fetch_gaat_naar_de_cache(clone, upstream_url: str) -> str | None:
    """Refuse to fetch unless the target really is the cache.

    A second lock, deliberately not resting on the environment being clean. The
    refspec below force-updates every branch, so pointing it at the wrong
    repository destroys unpushed work; `schone_omgeving()` stops that happening,
    and this stops it happening if someone adds a call and forgets.

    `--git-dir` is asked of git itself rather than assumed from `cwd`, because
    the whole failure being guarded against is git disagreeing with `cwd`.
    """
    from pathlib import Path as _P

    # `--git-common-dir`, not `--absolute-git-dir`: a linked worktree's git-dir
    # is `<main>/.git/worktrees/<name>` while its common-dir is `<main>/.git`.
    # Comparing git-dirs both rejected a worktree handed over as `HAYRO_CLONE`
    # -- a legitimate checkout, refused before anything was scored -- and let
    # this repository through whenever the guard itself ran from a worktree,
    # which is how the first version of this check passed its own ROOT test.
    # Common-dirs make both cases structural rather than incidental.
    #
    # Asked in the same environment the fetch will use, which means scrubbed.
    #
    # Review suggested reading the *inherited* environment instead, so the probe
    # could catch a caller that forgot to strip. Measured before taking it, and
    # the measurement said no: `git push` from a linked worktree exports
    # `GIT_DIR`, so under our own pre-push hook every target resolves to this
    # repository and the lock refuses the bare cache, a valid `HAYRO_CLONE` and
    # a worktree alike -- five legitimate cases, all wrongly refused.
    #
    # (An earlier measurement here said hooks do not export `GIT_DIR`. That was
    # taken in a plain clone, where it is true, and it is false in a worktree --
    # which is where all our work happens.)
    #
    # The hazard that suggestion aimed at -- a call that forgets `env=` -- is
    # real, and is caught mechanically one layer up by
    # `test_no_test_can_touch_the_real_repo.py`, which requires `env=` on every
    # git invocation in these scripts. That is the right place for it: a lint
    # over call sites, not a probe guessing at how it will be called.
    out = git("rev-parse", "--git-common-dir", cwd=clone)
    if out.returncode != 0:
        return f"cannot tell which repository {clone} is, so the fetch is refused"
    doel = _P(out.stdout.strip())
    if not doel.is_absolute():
        doel = (_P(clone) / doel)
    doel = doel.resolve()

    # (a) Never this repository, whatever the configuration says.
    #
    # Comparing the target against the path it was *asked* to be is not a lock:
    # for any ordinary checkout `--absolute-git-dir` is `<checkout>/.git`, so
    # accepting that shape accepts the very repository the guard exists to
    # protect. The first version of this check passed its own ROOT test only
    # because it ran in a worktree, whose git-dir is
    # `<main>/.git/worktrees/<name>` and therefore happened not to match.
    # Measured in a plain checkout, the repository root was accepted.
    #
    # So the question is not "is the target the path we wanted" but "is the
    # target us", asked of git from the script's own location.
    hier = _P(__file__).resolve().parent
    ons = git("rev-parse", "--git-common-dir", cwd=hier)
    onze = None
    if ons.returncode == 0:
        onze = _P(ons.stdout.strip())
        # git answers relatively when asked from inside the work tree
        if not onze.is_absolute():
            onze = hier / onze
        onze = onze.resolve()
    if onze is not None and onze == doel:
        return (
            f"refusing to fetch: the target is this repository ({doel}). The refspec "
            "force-updates every branch, so this would rewrite local branches."
        )

    # (b) And it must be the upstream *this cache is for*, not merely somewhere
    # that is not us. The criterion is the URL the register names for this
    # entry: the same loop refreshes the lopdf cache, whose legitimate origin is
    # `J-F-Liu/lopdf.git`, so a hardcoded "hayro" refused it and left the next
    # lopdf fork-point update unverifiable.
    # Any remote may name the upstream, not `origin` specifically.
    #
    # A remote's *name* is a local preference, not a property of the repository:
    # the ordinary fork layout is `origin` = the developer's fork and `upstream`
    # = the real project, and insisting on `origin` refused a checkout that
    # holds exactly the history this guard needs.
    #
    # A local mirror with no remote naming the upstream is still refused, and
    # that is deliberate: this guard verifies our fork points against upstream,
    # and a repository that cannot say it is upstream is not something to verify
    # against silently. The refusal says which URLs were seen.
    # `git remote -v`, not `git config --get remote.origin.url`.
    #
    # This is what makes the check see a rewritten URL. `insteadOf` in the
    # repository's own config redirects fetches, and the seal cannot switch that
    # off -- `GIT_CONFIG_NOSYSTEM` and `GIT_CONFIG_GLOBAL` reach the system and
    # global files only. Measured on a cache whose local config redirects to a
    # decoy:
    #
    #     config --get remote.origin.url  ->  the real hayro URL   (raw)
    #     remote -v                       ->  file:///tmp/decoy    (rewritten)
    #
    # So the raw read approves while the fetch goes elsewhere; asking `remote`
    # compares against what git will actually contact. A separate check for
    # local `insteadOf` entries was written first and then removed: its mutation
    # showed the case already refused here, and the refusal already names the
    # decoy it saw.
    #
    # Asked under the *sealed* environment, the one the fetch uses. Asking it
    # unsealed was the mirror image of the bug above: the probe read the global
    # config and reported the rewritten URL while `git_verzegeld` ignores that
    # file and fetches the raw one. Measured with a cache whose origin is a
    # mirror and a global `insteadOf` pointing at the canonical URL:
    #
    #     probe, unsealed  ->  canonical.git   (approved)
    #     fetch, sealed    ->  mirror-COMMIT   (what actually arrived)
    #
    # A probe in a different environment than the operation it guards is not a
    # probe. Local `insteadOf` still shows here, because sealing does not reach
    # repository config -- which is exactly why the previous round's finding
    # stands.
    remotes = git_verzegeld("remote", "-v", cwd=clone)
    urls = sorted({
        regel.split()[1]
        for regel in remotes.stdout.splitlines()
        if len(regel.split()) >= 2
    })
    if not any(_zelfde_upstream(u, upstream_url) for u in urls):
        gezien = ", ".join(urls) if urls else "<no remotes>"
        return (
            f"refusing to fetch: no remote of {clone} names {upstream_url} "
            f"(saw: {gezien}), so this is not the cache it is for."
        )
    return None


def _zelfde_upstream(a: str, b: str) -> bool:
    """Compare two remote URLs by the repository they name.

    `https://github.com/X/y.git`, `git@github.com:X/y` and a trailing slash all
    name the same repository; comparing the strings would refuse a cache that is
    perfectly correct.
    """
    def kern(u: str) -> str:
        # Trailing slashes first: `.../hayro.git/` ends in a slash, so stripping
        # `.git` before them leaves it in place and two spellings of one
        # repository compare unequal.
        u = u.strip().lower().rstrip("/").removesuffix(".git").rstrip("/")
        u = u.replace("git@github.com:", "github.com/")
        for prefix in ("https://", "http://", "ssh://git@", "ssh://", "git://"):
            u = u.removeprefix(prefix)
        return u
    return kern(a) == kern(b)


def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    When `GIT_DIR` and `GIT_WORK_TREE` are set, git works on the repository
    they name and ignores `cwd=` entirely. (Measured 02-09: our own
    `pre-commit` and `pre-push` hooks do not export them, so the earlier
    wording here was too strong -- but `git rebase`, CI runners and any
    wrapper that exports them do, and one incident already came of it.) For a read-only command
    that is merely wrong; for the fetch in the register guards it was
    destructive, because the refspec is a force-update of every branch.

    Reproduced in a throwaway repository: a branch with an unpushed commit on
    top of a pushed one lost that commit outright. What hid it is luck -- git
    refuses to fetch into a branch that is checked out in a worktree and aborts
    the whole fetch, and one of ours always is, which is why this surfaced in
    the logs as `SKIPPED (not a pass)` rather than as damage. A detached HEAD
    has no such protection, and detached HEAD is what `actions/checkout`
    produces and what half of our worktrees are.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def verzegelde_omgeving() -> dict[str, str]:
    """A clean environment, and a git that cannot be redirected by config.

    Delegates to `fixture_env.sealed_env()`, which landed in #1647 doing exactly
    this and two things more: it strips `GIT_*` from the inherited environment
    with a prefix filter rather than by name, and it points
    `GIT_CONFIG_GLOBAL` at an empty file instead of /dev/null, so a stray
    `git config --global` write lands somewhere harmless instead of failing.

    Kept as a name here because the reason it exists is local to the register
    guards, and it is not the same reason a fixture seals its environment:
    `url.<base>.insteadOf` rewrites clone and fetch URLs silently. Measured with
    a global config redirecting hayro's URL to a local decoy, cloning the real
    URL produced the decoy's single commit while `remote.origin.url` still read
    `https://github.com/LaurenzV/hayro.git` -- so the origin check would have
    approved, and the register would have been verified against substituted
    content and reported green. A guard that can be aimed at a decoy is worse
    than no guard, because it reports success.

    One helper for one job: two sealings that drift apart is how a guard ends up
    protecting the version of the rule nobody reads. (#292)
    """
    return _fx.sealed_env()


def git_verzegeld(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    """git for clone and fetch: no repository *and* no config can redirect it."""
    return subprocess.run(
        ["/usr/bin/git", *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        env=verzegelde_omgeving(),
        check=False,
    )


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["/usr/bin/git", *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        env=schone_omgeving(),
        check=False,
    )


def cache_for(url: str) -> Path:
    """One bare cache per upstream, named after the repository."""
    if _EXPLICIT and url.endswith("hayro.git"):
        return Path(_EXPLICIT)
    return CACHE_ROOT / (url.rstrip("/").rsplit("/", 1)[-1].removesuffix(".git") + "-register.git")


def usable(path: Path) -> bool:
    """Is this a git directory we can read history from?

    Both shapes count: a bare repository (the cache) and a normal checkout
    (whatever a developer pointed HAYRO_CLONE at).
    """
    if not path.exists():
        return False
    return git("rev-parse", "--git-dir", cwd=path).returncode == 0


def ensure_clone(url: str) -> str | None:
    """Make sure this upstream's history is cached. Returns a reason on failure.

    Blobless and bare: this only ever reads a few Cargo.toml files, so fetching
    every blob in the repository would be paying for history nobody looks at.
    The blobs it does need are fetched on demand from the promisor remote.
    """
    clone = cache_for(url)
    if usable(clone):
        return None

    if _EXPLICIT and clone == Path(_EXPLICIT):
        # An explicit path that is not a clone is a mistake worth naming, rather
        # than silently replacing with our own.
        return f"HAYRO_CLONE={clone} is not a git repository"

    clone.parent.mkdir(parents=True, exist_ok=True)
    out = git_verzegeld("clone", "--bare", "--filter=blob:none", "--quiet", url, str(clone))
    if out.returncode != 0:
        return f"could not clone {url}: {out.stderr.strip()[:200]}"
    return None


def have_commit(clone: Path, commit: str) -> bool:
    """Whether the cache holds this commit, judged on the object, not a ref.

    The ``^{commit}`` peel is load-bearing and must not be simplified away.
    A commit SHA is a content hash, so an object with the register's SHA and a
    different tree cannot be produced without a SHA-1 collision -- but an
    object can be *planted* at that SHA's path, and the two spellings disagree
    about it. Measured on a bare repo with a mismatched object planted at a
    genuine SHA::

        git cat-file -e <sha>^{commit}   ->  rc 128, "error: hash mismatch"
        git cat-file -e <sha>            ->  rc 0
        git show -s --format=%s <sha>    ->  the planted content

    Without the peel the guard would accept the plant and version_at() would
    then read it, because the read path does not rehash either. The peel is
    what makes the cache location being environment-chosen (XDG_CACHE_HOME)
    harmless here. Pinned by
    `test_een_geplant_object_wordt_niet_geaccepteerd`.
    """
    return git("cat-file", "-e", f"{commit}^{{commit}}", cwd=clone).returncode == 0


def refresh_for(clone: Path, commits: list[str], upstream_url: str) -> str | None:
    """Fetch once if the register points at something the cache predates.

    Only when needed: a fetch on every run is a network round-trip to learn
    nothing, and this check runs on every push.
    """
    # The identity check runs before the early return, not only on the fetch
    # path. CACHE_ROOT is chosen by XDG_CACHE_HOME, so a cache that is really
    # this repository would otherwise be read without ever being questioned:
    # every commit present, early return, and the register then verified
    # against ourselves instead of against upstream. That is a false green
    # rather than damage, but it is the same door #308 closed on the fetch.
    if (waarom := de_fetch_gaat_naar_de_cache(clone, upstream_url)) is not None:
        return waarom
    if all(have_commit(clone, c) for c in commits):
        return None
    out = git_verzegeld("fetch", "--quiet", "--filter=blob:none", "origin",
              "+refs/heads/*:refs/heads/*", cwd=clone)
    if out.returncode != 0:
        # Not swallowed. Ignoring it made version_at() treat a still-missing
        # commit as a bad register entry and return 1 -- so a network outage was
        # reported as bad fork data and sent the reader at the wrong problem.
        # (Codex, #1609.)
        return f"could not refresh {clone.name}: {out.stderr.strip()[:200]}"
    return None


def version_at(clone: Path, commit: str, crate_dir: str) -> str | None:
    manifest = f"{crate_dir}/Cargo.toml" if crate_dir else "Cargo.toml"
    out = subprocess.run(
        ["/usr/bin/git", "show", f"{commit}:{manifest}"],
        cwd=clone,
        capture_output=True,
        text=True,
        env=schone_omgeving(),
        check=False,
    )
    if out.returncode != 0:
        return None
    for line in out.stdout.splitlines():
        if line.startswith("version"):
            return line.split('"')[1]
    return None


def main() -> int:
    if not REGISTER.exists():
        print(f"SKIPPED (not a pass): {REGISTER} is missing", file=sys.stderr)
        return 3

    forks = tomllib.loads(REGISTER.read_text()).get("fork", [])
    with_point = [f for f in forks if f.get("forkpunt")]

    if not with_point:
        print(
            "SKIPPED (not a pass): no entry in UPSTREAM_FORKS.toml records a fork point,\n"
            "  so there is nothing to verify `gelijk_met` against.",
            file=sys.stderr,
        )
        return 3

    # One clone per upstream, and only the ones this register actually names.
    for upstream in sorted({f["upstream"] for f in with_point}):
        if upstream not in UPSTREAMS:
            print(
                f"SKIPPED (not a pass): no repository recorded for upstream "
                f"`{upstream}`. Add it to UPSTREAMS; a fork point that cannot be\n"
                "  looked up is not a fork point that is right.",
                file=sys.stderr,
            )
            return 3
        url, _ = UPSTREAMS[upstream]
        if (why := ensure_clone(url)) is not None:
            print(
                f"SKIPPED (not a pass): {why}.\n"
                "  Without upstream's history the register's claims cannot be checked,\n"
                "  only repeated.",
                file=sys.stderr,
            )
            return 3
        wanted = [f["forkpunt"] for f in with_point if f["upstream"] == upstream]
        if (why := refresh_for(cache_for(url), wanted, url)) is not None:
            print(f"SKIPPED (not a pass): {why}", file=sys.stderr)
            return 3

    problems: list[str] = []
    checked = 0
    for f in with_point:
        crate, point, claimed = f["onze_crate"], f["forkpunt"], f["gelijk_met"]
        url, crate_dir = UPSTREAMS[f["upstream"]]
        actual = version_at(cache_for(url), point, crate_dir)
        if actual is None:
            problems.append(
                f"{crate}: fork point {point} does not resolve in {url}, or carries no "
                f"{crate_dir + '/' if crate_dir else ''}Cargo.toml. "
                "The register points at a commit that is not there."
            )
            continue
        checked += 1
        if actual != claimed:
            problems.append(
                f"{crate}: register says gelijk_met = {claimed}, but {point} carries {actual}. "
                "One of the two is wrong, and the other guard trusts this one."
            )

    if problems:
        print(f"Fork register: {len(problems)} entry/entries do not match their fork point\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    print(f"✓ {checked} fork entry/entries match the version at their recorded fork point")
    return 0


if __name__ == "__main__":
    sys.exit(main())
