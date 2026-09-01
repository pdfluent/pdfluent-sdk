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
    return subprocess.run(
        [GIT, *args], cwd=cwd, capture_output=True, text=True, check=False,
        env=env if env is not None else os.environ.copy(),
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
        os.environ.update(hook_env)
        run("fetch", "--quiet", "--filter=blob:none", "origin", "+refs/heads/*:refs/heads/*",
            cwd=tmp / "b" / "upstream.git", env=schone_omgeving())
        for key in ("GIT_DIR", "GIT_WORK_TREE"):
            os.environ.pop(key, None)
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

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        bare = tmp / "cache.git"
        nonbare = tmp / "hayro"
        run("init", "-q", "--bare", str(bare), cwd=tmp)
        run("init", "-q", str(nonbare), cwd=tmp)

        if reg.de_fetch_gaat_naar_de_cache(bare) is not None:
            failures.append("the bare cache was refused as a fetch target")
        if reg.de_fetch_gaat_naar_de_cache(nonbare) is not None:
            failures.append(
                "a non-bare HAYRO_CLONE checkout was refused, though the script "
                "documents it as supported"
            )
        if reg.de_fetch_gaat_naar_de_cache(ROOT) is None:
            failures.append(
                "the repository root was accepted as a fetch target -- the second "
                "lock is not locking"
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
