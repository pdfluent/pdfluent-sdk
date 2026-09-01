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


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["/usr/bin/git", *args],
        cwd=cwd,
        capture_output=True,
        text=True,
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
    out = git("clone", "--bare", "--filter=blob:none", "--quiet", url, str(clone))
    if out.returncode != 0:
        return f"could not clone {url}: {out.stderr.strip()[:200]}"
    return None


def have_commit(clone: Path, commit: str) -> bool:
    return git("cat-file", "-e", f"{commit}^{{commit}}", cwd=clone).returncode == 0


def refresh_for(clone: Path, commits: list[str]) -> str | None:
    """Fetch once if the register points at something the cache predates.

    Only when needed: a fetch on every run is a network round-trip to learn
    nothing, and this check runs on every push.
    """
    if all(have_commit(clone, c) for c in commits):
        return None
    out = git("fetch", "--quiet", "--filter=blob:none", "origin",
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
        if (why := refresh_for(cache_for(url), wanted)) is not None:
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
