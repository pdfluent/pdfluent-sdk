#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The register guards must not be able to touch the repository they run in.

They fetch with `+refs/heads/*:refs/heads/*` -- a force-update of every branch.
Aimed at their own cache that is correct; aimed at the real repository it resets
local branches to whatever the remote has, and unpushed commits are gone.

Under a git hook `GIT_DIR` and `GIT_WORK_TREE` are exported, and git then obeys
them and ignores `cwd=`. So the aim is decided by the environment, which is why
`schone_omgeving()` exists.

Both directions are checked here, because a test that cannot fail measures
nothing: the first attempt at this reproduction used a branch that did not exist
on the remote, the refspec never touched it, and the commit survived for reasons
that had nothing to do with the fix.

Exit codes:
  0  the guards cannot reach the real repository
  1  they can
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GIT = "/usr/bin/git"


def run(*args: str, cwd: Path, env: dict[str, str] | None = None):
    """Run git for the fixtures, with `GIT_*` stripped unless asked otherwise.

    This defaulted to inheriting the environment, and that was a real hazard
    rather than a tidiness point. Wired into the local gate, this test runs from
    the pre-push hook, where `GIT_DIR` points at the repository being pushed --
    so every `init` and `commit` meant for a throwaway fixture operated on the
    real repository instead. It rewrote this branch to a fixture commit and left
    the working tree carrying a file called `f`.

    The test that exists to prove the guards cannot reach the real repository
    reached it, and by exactly the mechanism it documents. Only the deliberate
    control below gets a dirty environment, and it names its target explicitly.
    """
    return subprocess.run(
        [GIT, *args], cwd=cwd, capture_output=True, text=True, check=False,
        env=env if env is not None else schone_omgeving(),
    )


def schone_omgeving() -> dict[str, str]:
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def een_repo_met_ongepusht_werk(tmp: Path) -> tuple[Path, dict[str, str]]:
    """A repository whose side branch exists on the remote *and* has a commit on top.

    Both halves matter. Without the branch existing upstream the refspec has
    nothing to force onto it; without the local commit there is nothing to lose.
    """
    tmp.mkdir(parents=True, exist_ok=True)
    upstream = tmp / "upstream.git"
    work = tmp / "work"
    run("init", "-q", "--bare", str(upstream), cwd=tmp)
    run("clone", "-q", str(upstream), str(work), cwd=tmp)
    run("config", "user.email", "t@t", cwd=work)
    run("config", "user.name", "t", cwd=work)
    (work / "f").write_text("one\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "one", cwd=work)
    run("push", "-q", "origin", "HEAD:master", cwd=work)
    run("checkout", "-q", "-b", "feature", cwd=work)
    (work / "f").write_text("shared\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "shared, pushed", cwd=work)
    run("push", "-q", "origin", "feature", cwd=work)
    (work / "f").write_text("local\n")
    run("add", "f", cwd=work)
    run("commit", "-qm", "UNPUSHED WORK", cwd=work)
    # Detached, which is what `actions/checkout` leaves behind. With a branch
    # checked out git refuses the fetch and the damage never lands -- which is
    # exactly the accident that hid this.
    run("checkout", "-q", "--detach", cwd=work)
    hook_env = dict(os.environ, GIT_DIR=str(work / ".git"), GIT_WORK_TREE=str(work))
    return work, hook_env


def tip(work: Path) -> str:
    return run("log", "-1", "--format=%s", "feature", cwd=work, env=schone_omgeving()).stdout.strip()


def main() -> int:
    if not (ROOT / "scripts/ci").is_dir():
        print(f"SKIPPED (not a pass): {ROOT}/scripts/ci is missing", file=sys.stderr)
        return 3

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)

        # --- the control: without the fix, the commit is destroyed ----------
        work, hook_env = een_repo_met_ongepusht_werk(tmp / "a")
        run("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*",
            cwd=tmp / "a" / "upstream.git", env=hook_env)
        if tip(work) == "UNPUSHED WORK":
            failures.append(
                "the control did not reproduce the damage, so this test proves nothing: "
                "the unpushed commit survived a fetch that should have destroyed it"
            )

        # --- and with a clean environment it survives -----------------------
        work, hook_env = een_repo_met_ongepusht_werk(tmp / "b")
        # `hook_env` is handed over explicitly rather than pushed into
        # `os.environ`: mutating this process's environment leaks into every
        # later fixture call, and `schone_omgeving()` would then be scrubbing a
        # variable this test had set itself.
        run("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*",
            cwd=tmp / "b" / "upstream.git", env=schone_omgeving())
        if tip(work) != "UNPUSHED WORK":
            failures.append("a clean environment did not protect the unpushed commit")

    # --- the second lock must be stricter than the environment, not the user --
    #
    # Both clone shapes are legitimate: the cache this fetches for itself is
    # bare, and a clone a developer points `HAYRO_CLONE` at is an ordinary
    # checkout whose git-dir is `<clone>/.git`. The first version of the check
    # knew only the bare shape and refused the other, failing the run before it
    # scored anything.
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "reg", ROOT / "scripts/ci/the_fork_register_is_verifiable.py"
    )
    reg = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reg)
    HAYRO = reg.UPSTREAMS["hayro"][0]

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        bare = tmp / "cache.git"
        nonbare = tmp / "hayro"
        run("init", "-q", "--bare", str(bare), cwd=tmp)
        run("init", "-q", str(nonbare), cwd=tmp)

        # Both need a hayro origin: the lock requires the target to be the
        # upstream, not merely somewhere that is not us.
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=bare)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=nonbare)

        if reg.de_fetch_gaat_naar_de_cache(bare, HAYRO) is not None:
            failures.append("the bare cache was refused as a fetch target")
        if reg.de_fetch_gaat_naar_de_cache(nonbare, HAYRO) is not None:
            failures.append(
                "a non-bare HAYRO_CLONE checkout was refused, though the script "
                "documents it as supported"
            )
        if reg.de_fetch_gaat_naar_de_cache(ROOT, HAYRO) is None:
            failures.append(
                "the repository root was accepted as a fetch target -- the second "
                "lock is not locking"
            )

        # A plain checkout, which is what ROOT is outside a worktree. The
        # previous version of this test passed on ROOT only because it ran in a
        # worktree, whose git-dir is `<main>/.git/worktrees/<name>` and so did
        # not match the shape being accepted. In a plain clone the repository
        # was accepted outright, and this test said nothing.
        plain = tmp / "plain"
        run("init", "-q", str(plain), cwd=tmp)
        if reg.de_fetch_gaat_naar_de_cache(plain, HAYRO) is None:
            failures.append(
                "a plain checkout with no hayro origin was accepted as a fetch target"
            )

        # The case the origin check alone cannot see: a repository that *does*
        # have hayro as its origin and is also the one the script is running
        # inside. That is what these scripts vendored into a hayro fork would
        # look like, and there the origin test passes while a force-fetch would
        # rewrite the developer's own branches. Only "the target is not us"
        # refuses it.
        vendored = tmp / "hayro-fork"
        (vendored / "scripts" / "ci").mkdir(parents=True)
        run("init", "-q", str(vendored), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=vendored)
        script = ROOT / "scripts/ci/the_fork_register_is_verifiable.py"
        (vendored / "scripts/ci" / script.name).write_bytes(script.read_bytes())
        spec_v = importlib.util.spec_from_file_location(
            "reg_vendored", vendored / "scripts/ci" / script.name
        )
        reg_v = importlib.util.module_from_spec(spec_v)
        spec_v.loader.exec_module(reg_v)
        if reg_v.de_fetch_gaat_naar_de_cache(vendored, HAYRO) is None:
            failures.append(
                "a hayro-origin repository that is the script's own checkout was "
                "accepted -- the origin check passes there, so only the "
                "'not this repository' clause can refuse it"
            )

        # A linked worktree handed over as HAYRO_CLONE is a legitimate checkout.
        # Its git-dir is `<main>/.git/worktrees/<name>`, which matches neither
        # `<clone>` nor `<clone>/.git`, so comparing git-dirs refused it before
        # anything was scored. Common-dirs are the same for main and linked
        # worktrees, which is why the identity question is asked that way.
        wt = tmp / "linked"
        run("worktree", "add", "-q", str(wt), cwd=nonbare)
        if reg.de_fetch_gaat_naar_de_cache(wt, HAYRO) is not None:
            failures.append(
                "a linked worktree of a hayro clone was refused as a fetch target"
            )

        # The case common-dirs exist for: the guard running inside a worktree
        # while the *main* checkout is handed to it as the target. Their
        # git-dirs differ (`<main>/.git` against
        # `<main>/.git/worktrees/<name>`), so a git-dir comparison calls them
        # different repositories and lets the main checkout through. Their
        # common-dirs are equal, which is the whole point.
        hoofd = tmp / "guarded"
        (hoofd / "scripts" / "ci").mkdir(parents=True)
        run("init", "-q", str(hoofd), cwd=tmp)
        run("remote", "add", "origin", "https://github.com/LaurenzV/hayro.git", cwd=hoofd)
        (hoofd / "f").write_text("x")
        run("add", "f", cwd=hoofd)
        run("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "x", cwd=hoofd)
        zijtak = tmp / "guarded-wt"
        run("worktree", "add", "-q", str(zijtak), cwd=hoofd)
        # the guard lives in the worktree, and is asked about the main checkout
        (zijtak / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
        (zijtak / "scripts/ci" / script.name).write_bytes(script.read_bytes())
        spec_w = importlib.util.spec_from_file_location(
            "reg_worktree", zijtak / "scripts/ci" / script.name
        )
        reg_w = importlib.util.module_from_spec(spec_w)
        spec_w.loader.exec_module(reg_w)
        if reg_w.de_fetch_gaat_naar_de_cache(hoofd, HAYRO) is None:
            failures.append(
                "running from a worktree, the main checkout was accepted as a fetch "
                "target -- git-dirs differ between the two, only common-dirs match"
            )

        # Per registered upstream, not a hardcoded name. The same loop refreshes
        # the lopdf cache, whose legitimate origin is `J-F-Liu/lopdf.git`; a
        # check for "hayro" refused it and left the next lopdf fork-point update
        # unverifiable.
        for naam, (upstream_url, _) in reg.UPSTREAMS.items():
            cache = tmp / f"cache-{naam}"
            run("init", "-q", "--bare", str(cache), cwd=tmp)
            run("remote", "add", "origin", upstream_url, cwd=cache)
            if reg.de_fetch_gaat_naar_de_cache(cache, upstream_url) is not None:
                failures.append(
                    f"the {naam} cache was refused although its origin is the one "
                    f"the register names ({upstream_url})"
                )
        # and a cache whose origin is a *different* registered upstream
        kruis = tmp / "cache-crossed"
        run("init", "-q", "--bare", str(kruis), cwd=tmp)
        run("remote", "add", "origin", reg.UPSTREAMS["hayro"][0], cwd=kruis)
        if reg.de_fetch_gaat_naar_de_cache(kruis, reg.UPSTREAMS["lopdf"][0]) is None:
            failures.append(
                "a cache with hayro as origin was accepted as the lopdf cache"
            )
        # URL spellings that name the same repository must not be refused
        for spelling in (
            "git@github.com:LaurenzV/hayro.git",
            "https://github.com/LaurenzV/hayro",
            "https://github.com/LaurenzV/hayro.git/",
        ):
            alt = tmp / f"cache-alt-{abs(hash(spelling))}"
            run("init", "-q", "--bare", str(alt), cwd=tmp)
            run("remote", "add", "origin", spelling, cwd=alt)
            if reg.de_fetch_gaat_naar_de_cache(alt, reg.UPSTREAMS["hayro"][0]) is not None:
                failures.append(f"origin spelled {spelling} was refused")

        # And somewhere that *is* a hayro clone by name but not by origin.
        impostor = tmp / "hayro-lookalike"
        run("init", "-q", str(impostor), cwd=tmp)
        run("remote", "add", "origin", "https://example.invalid/other.git", cwd=impostor)
        if reg.de_fetch_gaat_naar_de_cache(impostor, HAYRO) is None:
            failures.append(
                "a repository named hayro but with a different origin was accepted"
            )

    if failures:
        print("[registerwachters] the guards can reach the real repository:\n", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print("[registerwachters] OK: the fetch is destructive without a clean "
          "environment and harmless with one")
    return 0


if __name__ == "__main__":
    sys.exit(main())
