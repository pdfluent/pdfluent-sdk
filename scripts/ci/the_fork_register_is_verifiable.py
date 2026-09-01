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

UPSTREAM_URL = "https://github.com/LaurenzV/hayro.git"

# Where to keep the history between runs. Outside the checkout on purpose: a
# runner that reuses its workspace keeps it warm, and one that does not is only
# paying a cold clone.
CACHE = Path(
    os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")
) / "pdfluent" / "hayro-register.git"

# A clone somebody already has wins: developers usually have one from the last
# upgrade, and using it avoids a second copy of 100-odd megabytes.
_EXPLICIT = os.environ.get("HAYRO_CLONE")
CLONE = Path(_EXPLICIT) if _EXPLICIT else CACHE


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["/usr/bin/git", *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def usable(path: Path) -> bool:
    """Is this a git directory we can read history from?

    Both shapes count: a bare repository (the cache) and a normal checkout
    (whatever a developer pointed HAYRO_CLONE at).
    """
    if not path.exists():
        return False
    return git("rev-parse", "--git-dir", cwd=path).returncode == 0


def ensure_clone() -> str | None:
    """Make sure CLONE has upstream's history. Returns a reason on failure.

    Blobless and bare: this only ever reads a few Cargo.toml files, so fetching
    every blob in the repository would be paying for history nobody looks at.
    The blobs it does need are fetched on demand from the promisor remote.
    """
    if usable(CLONE):
        return None

    if _EXPLICIT:
        # An explicit path that is not a clone is a mistake worth naming, rather
        # than silently replacing with our own.
        return f"HAYRO_CLONE={CLONE} is not a git repository"

    CLONE.parent.mkdir(parents=True, exist_ok=True)
    out = git(
        "clone", "--bare", "--filter=blob:none", "--quiet", UPSTREAM_URL, str(CLONE)
    )
    if out.returncode != 0:
        return f"could not clone {UPSTREAM_URL}: {out.stderr.strip()[:200]}"
    return None


def have_commit(commit: str) -> bool:
    return git("cat-file", "-e", f"{commit}^{{commit}}", cwd=CLONE).returncode == 0


def refresh_for(commits: list[str]) -> None:
    """Fetch once if the register points at something the cache predates.

    Only when needed: a fetch on every run is a network round-trip to learn
    nothing, and this check runs on every push.
    """
    if all(have_commit(c) for c in commits):
        return
    git("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*", cwd=CLONE)


def version_at(commit: str, crate_dir: str) -> str | None:
    out = subprocess.run(
        ["/usr/bin/git", "show", f"{commit}:{crate_dir}/Cargo.toml"],
        cwd=CLONE,
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

    if (why := ensure_clone()) is not None:
        print(
            f"SKIPPED (not a pass): {why}.\n"
            "  Without upstream's history the register's claims cannot be checked,\n"
            "  only repeated.",
            file=sys.stderr,
        )
        return 3

    refresh_for([f["forkpunt"] for f in with_point])

    problems: list[str] = []
    checked = 0
    for f in with_point:
        crate, point, claimed = f["onze_crate"], f["forkpunt"], f["gelijk_met"]
        actual = version_at(point, f["upstream"])
        if actual is None:
            problems.append(
                f"{crate}: fork point {point} does not resolve, or has no {f['upstream']}/Cargo.toml. "
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
