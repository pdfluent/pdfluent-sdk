#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which workspace crates a landing touches, and which others could feel it (#343).

WHY

Every push to master runs the full lane, and the full lane compiles and tests the
whole workspace in the landing worktree. Measured 05-09-2026 over the landings of
that day: 45 to 77 minutes each, one at a time, so the machine landed at most one
pull request an hour no matter how many were ready.

Nearly all of that is spent on crates the landing does not touch. A change to
`scripts/ci` compiles forty-nine crates to prove nothing about itself; a change
to one leaf crate compiles the other forty-eight for the same reason.

WHAT IT ANSWERS

Given the files a landing changes, the set of workspace crates that must be
built and tested for that landing: the crates those files belong to, plus every
workspace crate that depends on one of them, directly or transitively. Test
dependencies count -- a change in A can break B's tests without touching B's
library -- so the graph is read over dependencies of every kind.

WHEN IT ANSWERS "EVERYTHING"

Three cases print `*`, the token for the whole workspace:

  * the root manifest, the lockfile, the toolchain file or `.cargo/config*`
    changed. Those decide what the compiler sees for every crate, so no subset
    of crates is a safe answer.
  * a changed file sits in a directory that holds crates and belongs to no crate
    this metadata knows. The likeliest cause is a crate added in the same
    change, and answering "no crates" there would skip the new one entirely.
  * cargo metadata could not be read (exit 3, announced). A caller that cannot
    get an answer must build everything, not nothing.

WHAT IT IS NOT

It is not a claim that the unbuilt crates are fine. It is a claim about WHERE
that is established: the whole workspace is compiled and tested on the push to
master, on the runner, and master goes red there if a landing broke something
outside the crates it touched. This trades the moment of the finding, not the
finding -- the same trade the never-green guard made, and it is only true while
that job on master exists.
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]

# The whole workspace, spelled as one token so a caller can tell it from a crate
# name. A crate may not be called this, and cargo would refuse the name anyway.
ALL = "*"

# Files that decide what the compiler sees for every crate at once. A subset of
# crates is never a safe answer to a change in one of these: the lockfile moves
# a dependency version under all of them, and the toolchain file moves the
# compiler itself.
WORKSPACE_WIDE = re.compile(
    r"^(Cargo\.toml|Cargo\.lock|rust-toolchain(\.toml)?|\.cargo/config(\.toml)?)$"
)

# The crates CI does not build, spelled here because the scoped lane cannot use
# `--exclude`: it names its packages with `-p`, and a `-p` list that includes
# these would build what the workspace lane deliberately leaves out.
#
# ONE LIST, TWO PLACES, CHECKED. This duplicates the `--exclude` flags in
# scripts/ci/run_build.sh, run_clippy.sh and run_test.sh, and a duplicate that
# nobody compares is a second answer waiting to disagree -- so `excludes_agree()`
# reads those files and says so when they drift. That check is the reason the
# duplication is allowed to exist at all.
EXCLUDED = ("pdf-desktop", "xfa-wasm")

RUN_SCRIPTS = ("run_build.sh", "run_clippy.sh", "run_test.sh")


def metadata(cwd: pathlib.Path | None = None) -> dict | None:
    """`cargo metadata --no-deps`, or None if it cannot be read.

    `--no-deps` is enough and it is what makes this cheap: 0.09 s on this
    workspace, because it resolves nothing off the registry. The dependency
    edges between workspace members are in it already -- each package lists its
    dependencies by name, and a name that is a member is an edge.
    """
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=str(cwd or ROOT), capture_output=True, text=True, check=False,
        )
    except OSError as exc:  # no cargo on PATH
        print(f"[touched-crates] cargo metadata could not be started: {exc}",
              file=sys.stderr)
        return None
    if out.returncode != 0:
        print("[touched-crates] cargo metadata failed:\n"
              + "\n".join(f"  {r}" for r in out.stderr.splitlines()[-8:]),
              file=sys.stderr)
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError as exc:
        print(f"[touched-crates] cargo metadata is not JSON: {exc}", file=sys.stderr)
        return None


def workspace(meta: dict) -> tuple[dict[str, str], dict[str, set[str]]]:
    """(crate name -> its directory relative to the workspace root,
        crate name -> the workspace crates that depend on it).

    The second map is the reverse graph, not the forward one: what the callers
    need is "who could feel a change in this crate", and that is the direction
    nobody can read off a manifest without walking every other manifest.
    """
    root = pathlib.Path(meta["workspace_root"])
    members = {p["name"] for p in meta["packages"]}
    dirs: dict[str, str] = {}
    dependents: dict[str, set[str]] = {name: set() for name in members}
    for package in meta["packages"]:
        directory = pathlib.Path(package["manifest_path"]).parent
        dirs[package["name"]] = os.path.relpath(directory, root)
        for dep in package.get("dependencies") or []:
            # Every kind counts -- normal, dev and build. A change in A breaking
            # B's TESTS is the case a normal-only graph would miss, and it is
            # the likeliest one: the tests are where crates reach into each
            # other hardest.
            if dep["name"] in members:
                dependents[dep["name"]].add(package["name"])
    return dirs, dependents


def crate_of(path: str, dirs: dict[str, str]) -> str | None:
    """The crate a file belongs to: the DEEPEST crate directory containing it.

    Deepest, not first: a workspace where one crate sits inside another's
    directory would otherwise attribute the inner crate's files to the outer
    one, and quietly build the wrong package.
    """
    best: str | None = None
    for name, directory in dirs.items():
        if path == directory or path.startswith(directory + "/"):
            if best is None or len(directory) > len(dirs[best]):
                best = name
    return best


def crate_roots(dirs: dict[str, str]) -> set[str]:
    """The top directories that hold crates -- `crates`, `tools`, and whatever
    is added later. A changed file under one of these that belongs to no known
    crate is the "a crate was added in this change" case."""
    return {directory.split("/")[0] for directory in dirs.values() if "/" in directory}


def select(changed: list[str], dirs: dict[str, str],
           dependents: dict[str, set[str]],
           excluded: tuple[str, ...] = EXCLUDED) -> list[str] | str:
    """The crates to build for these changed files, or ALL.

    Returns a sorted list -- possibly empty, which is a real answer and means
    the change touches no crate at all (documentation, CI scripts, workflows).
    """
    roots = crate_roots(dirs)
    direct: set[str] = set()
    for path in changed:
        if WORKSPACE_WIDE.match(path):
            return ALL
        name = crate_of(path, dirs)
        if name is not None:
            direct.add(name)
        elif path.split("/")[0] in roots:
            # Under a directory that holds crates, in none of them. Either a
            # crate is being added in this same change, or the metadata is
            # older than the tree. Both mean the map cannot answer.
            return ALL

    # Transitive closure over the reverse graph.
    reachable = set(direct)
    queue = list(direct)
    while queue:
        name = queue.pop()
        for dependent in dependents.get(name, ()):
            if dependent not in reachable:
                reachable.add(dependent)
                queue.append(dependent)
    return sorted(reachable - set(excluded))


def excludes_agree(ci: pathlib.Path | None = None) -> list[str]:
    """Complaints about EXCLUDED disagreeing with the `--exclude` flags in the
    scripts the pipeline runs. Empty when the two say the same thing."""
    directory = ci or (ROOT / "scripts" / "ci")
    problems: list[str] = []
    for name in RUN_SCRIPTS:
        path = directory / name
        if not path.is_file():
            problems.append(f"{name} is missing, so its excludes could not be read")
            continue
        found = set(re.findall(r"--exclude\s+([A-Za-z0-9_.-]+)", path.read_text()))
        if found != set(EXCLUDED):
            problems.append(
                f"{name} excludes {sorted(found) or 'nothing'} while this file "
                f"expects {sorted(EXCLUDED)}")
    return problems


def changed_files(base: str, cwd: pathlib.Path | None = None) -> list[str] | None:
    """The paths this landing changes against `base`, or None if git cannot say.

    Two dots and not three. A landing is a fast-forward onto the ref it is
    compared with, so the merge base IS the base; asking for `base...HEAD` on a
    branch that has not been rebased yet would hide files master changed
    underneath it, and those are exactly the files whose crates need rebuilding.
    """
    out = subprocess.run([
        "git", "diff", "--name-only", f"{base}..HEAD"],
        cwd=str(cwd or ROOT), capture_output=True, text=True, check=False)
    if out.returncode != 0:
        print("[touched-crates] could not diff against "
              f"{base}: {out.stderr.strip()}", file=sys.stderr)
        return None
    return [r for r in out.stdout.splitlines() if r]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--base", default=os.environ.get("LANDING_BASE") or "",
                    help="revision the landing is measured against; "
                         "defaults to $LANDING_BASE, then to the first of "
                         "github/master, origin/master that exists")
    ap.add_argument("--file", action="append", default=[], metavar="PATH",
                    help="judge these paths instead of asking git (for tests)")
    ap.add_argument("--check-excludes", action="store_true",
                    help="only report whether EXCLUDED still matches the run scripts")
    args = ap.parse_args()

    problems = excludes_agree()
    if args.check_excludes:
        for problem in problems:
            print(f"[touched-crates] {problem}", file=sys.stderr)
        if problems:
            print("\n  The scoped lane names its packages with `-p`, so it cannot use\n"
                  "  `--exclude`; this list is how it leaves the same crates out. Two\n"
                  "  answers to one question is what this check refuses.", file=sys.stderr)
            return 1
        print(f"[touched-crates] OK: {', '.join(EXCLUDED)} excluded in "
              f"{len(RUN_SCRIPTS)} run script(s) and here")
        return 0
    if problems:
        # Not fatal to a selection, but never silent: a drifted list builds a
        # crate the workspace lane leaves out, or leaves one out that it builds.
        for problem in problems:
            print(f"[touched-crates] WARNING: {problem}", file=sys.stderr)

    meta = metadata()
    if meta is None:
        return 3
    dirs, dependents = workspace(meta)

    if args.file:
        changed: list[str] | None = list(args.file)
    else:
        base = args.base
        if not base:
            for candidate in ("github/master", "origin/master"):
                found = subprocess.run(["git", "rev-parse", "--verify", "-q", candidate],
                                       cwd=str(ROOT), capture_output=True, text=True)
                if found.returncode == 0:
                    base = candidate
                    break
        if not base:
            print("[touched-crates] no base to compare against: neither "
                  "$LANDING_BASE nor github/master nor origin/master resolves.",
                  file=sys.stderr)
            return 3
        changed = changed_files(base)
    if changed is None:
        return 3

    chosen = select(changed, dirs, dependents)
    if chosen == ALL:
        print(ALL)
    else:
        for name in chosen:
            print(name)
    print(f"[touched-crates] {len(changed)} changed file(s) -> "
          + (f"{len(chosen)} crate(s)" if chosen != ALL else "the whole workspace"),
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
