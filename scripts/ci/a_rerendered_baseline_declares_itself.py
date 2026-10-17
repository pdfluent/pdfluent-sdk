#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A commit that moves a visual baseline says which pages it moved (#293).

WHAT THIS GUARDS THAT THE SUITE CANNOT

`crates/visual-regression` compares the renderer against committed images of its
own output, so it catches any drift in what we draw. It cannot catch drift in
what it compares against. Change `crates/pdf-render`, run
`UPDATE_VISUAL_BASELINE=1`, commit both, and every gate is green: the reference
moved to meet the regression and the suite agreed with itself.

The crate's README already names the rule -- "a pull request touching
`baseline/` must explain each changed image" -- and it is a rule addressed to a
reviewer. Under the working method in force since 04-09-2026 there is no
reviewer: master receives fast-forwards of branches that passed the local gate,
and nobody reads the diff before it lands. A rule with no reader is the shape
#293 opened with: `avrt.yml` reported success on 42 runs having compared nothing,
because the thing that would have noticed was a human who never looked.

WHY A TRAILER

Same reason as `a_deletion_declares_itself.py`, whose form this follows. A
register drifts: the baseline change moves between rebases and the entry does
not follow it. A trailer lives in the commit that performs the change, travels
with it through every rebase and cherry-pick, and cannot be true of a commit
that no longer moves that image:

    Rerenders-deliberately: shading-axial-p1.png

Both directions, because that is what makes a declaration worth reading:

  * a moved baseline page no commit names FAILS -- the thing this exists for;
  * a trailer naming a page this range does not move FAILS too. An explanation
    that outlives its subject reads as current and quietly covers whatever next
    occupies that name.

A page is named by its file name; one trailer may name several, comma-separated.
Naming forty-two of them for a deliberate whole-suite re-render is not friction
to be designed away -- it is the review, written down by the person who has
actually looked at forty-two images.

WHAT IT DELIBERATELY DOES NOT DO

It does not judge whether the new image is right. Nothing in this repository can:
that is the judgement route 2 of #293 gave up when it took our own output as the
reference. It makes the change impossible to make silently, which is the whole of
what a self-referential suite can be defended by.

Usage: a_rerendered_baseline_declares_itself.py [--base github/master] [--head HEAD]
                                                [--exact-base]
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys

# The directory whose contents are the reference. Spelled once; `paths_under`
# is the only thing that reads it.
BASELINE = "crates/visual-regression/baseline/"
TRAILER = "rerenders-deliberately:"


def clean_env() -> dict[str, str]:
    """git must read the repository we are standing in, not the caller's.

    Inside a hook GIT_DIR and GIT_WORK_TREE name the real repository and git
    ignores where you point it -- the same trap `a_deletion_declares_itself.py`
    and `touched_crates.py` both document, and the reason a test can otherwise
    make its verdict about the developer's own checkout.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str, cwd: str | None = None) -> tuple[int, str]:
    # `diff.renames` changes what a diff MEANS, and a baseline page renamed is a
    # page whose old name no longer has an image behind it. Renames off is the
    # stricter reading and the one that matches what a reader of `baseline/`
    # sees.
    r = subprocess.run(
        ["git", "-c", "diff.renames=false", "-c", "core.quotePath=false", *args],
        capture_output=True, text=True, cwd=cwd,
        env=dict(clean_env(), GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull))
    return r.returncode, r.stdout


def paths_under(paths: set[str]) -> set[str]:
    """The baseline images among `paths`, as bare file names."""
    return {p[len(BASELINE):] for p in paths
            if p.startswith(BASELINE) and p.endswith(".png")}


def declared_in(body: str) -> set[str]:
    """The page names a commit message declares, from its trailers.

    A trailer starts at column zero, which is what makes it a trailer rather
    than prose. This guard's own first commit message quoted the format as an
    indented example, and a version that stripped the line first read that
    example as a declaration and refused the commit that introduced it. An
    indented line is text ABOUT a trailer; git reads it that way too.
    """
    named: set[str] = set()
    for line in body.splitlines():
        if line[:1].isspace() or not line.lower().startswith(TRAILER):
            continue
        for piece in line.split(":", 1)[1].split(","):
            piece = piece.strip()
            if not piece:
                continue
            # A trailer may spell the path in full; it means the same page.
            named.add(piece[len(BASELINE):] if piece.startswith(BASELINE) else piece)
    return named


def judge(moved: set[str], commits: list[tuple[str, set[str], set[str]]]
          ) -> list[str]:
    """Every complaint about one range.

    `commits` is (sha, pages that commit moves, pages that commit declares), in
    order. The commit that must carry the trailer is the LAST one to move a
    page, because that is the one whose effect survives to the tip -- an earlier
    commit's declaration can belong to a change a later commit undid.
    """
    problems: list[str] = []

    last_mover: dict[str, str] = {}
    for sha, pages, _ in commits:
        for page in pages:
            last_mover[page] = sha
    declared_by: dict[str, set[str]] = {sha: names for sha, _, names in commits}

    for page in sorted(moved):
        sha = last_mover.get(page)
        if sha is None:
            # The range's net diff moves a page that no commit in it moves. A
            # range read one way and a history read another disagree, and the
            # answer to that is never "clean".
            problems.append(
                f"{page} differs across the range, but no commit in it changes "
                f"that file -- the range and the history disagree, so nothing "
                f"here can be trusted to have been declared")
            continue
        if page not in declared_by.get(sha, set()):
            problems.append(
                f"{page} is re-rendered by {sha[:9]}, which does not name it. "
                f"Add `Rerenders-deliberately: {page}` to that commit, after "
                f"looking at the image")

    for sha, pages, names in commits:
        for name in sorted(names - pages):
            problems.append(
                f"{sha[:9]} declares {name}, which it does not change. A "
                f"declaration that outlives its subject reads as current and "
                f"covers whatever next occupies that name")
    return problems


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="github/master")
    ap.add_argument("--head", default="HEAD")
    ap.add_argument("--exact-base", action="store_true",
                    help="compare against --base itself instead of the merge "
                         "base. A push event reports where the branch actually "
                         "WAS, and deriving a merge base from it loses whatever "
                         "was added between the common ancestor and that tip.")
    args = ap.parse_args(argv[1:])

    if args.exact_base:
        rc, base = git("rev-parse", "--verify", f"{args.base}^{{commit}}")
    else:
        rc, base = git("merge-base", args.base, args.head)
    base = base.strip()
    if rc != 0 or not base:
        # Never a silent pass. Without a base there is no question to answer,
        # and "no baseline moved" would be a verdict about a comparison that did
        # not happen -- the exact sentence #293 was opened over.
        print(f"[baseline] FATAL: no base between {args.base} and {args.head}. "
              f"Fetch the base branch first; refusing to report a clean result "
              f"for a comparison that never ran.", file=sys.stderr)
        return 2

    head = git("rev-parse", args.head)[1].strip()
    if base == head:
        print(f"[baseline] FATAL: the base resolves to {args.head} itself, so "
              f"the range is empty and no re-render could be found. Pass the "
              f"revision this work started from, not the one it produced.",
              file=sys.stderr)
        return 2

    rc, out = git("diff", "--name-only", "-z", f"{base}..{args.head}")
    if rc != 0:
        print(f"[baseline] FATAL: `git diff {base}..{args.head}` failed (exit "
              f"{rc}). Refusing to report a clean range for a comparison that "
              f"did not happen.", file=sys.stderr)
        return 2
    moved = paths_under({p for p in out.split("\0") if p.strip()})

    rc, log = git("log", "--reverse", "--format=%H%x00%B%x1e", f"{base}..{args.head}")
    if rc != 0:
        print(f"[baseline] FATAL: `git log {base}..{args.head}` failed (exit "
              f"{rc}). The range could not be read, so no commit was examined.",
              file=sys.stderr)
        return 2

    commits: list[tuple[str, set[str], set[str]]] = []
    for entry in log.split("\x1e"):
        if not entry.strip():
            continue
        sha, _, body = entry.partition("\x00")
        sha = sha.strip()
        if not sha:
            continue
        # `--first-parent -m` so a merge is read against the side it landed on.
        # A merge inherits the other side's changes and its generated message
        # carries no trailer, so a correctly declared re-render on a branch would
        # otherwise fail the moment it was merged rather than fast-forwarded.
        parents = git("rev-list", "--parents", "-n", "1", sha)[1].split()[1:]
        if len(parents) > 1:
            continue
        _, own = git("show", "--name-only", "--format=", "-z", sha)
        commits.append((sha,
                        paths_under({p for p in own.split("\0") if p.strip()}),
                        declared_in(body)))

    problems = judge(moved, commits)
    if not problems:
        if moved:
            print(f"[baseline] OK: {len(moved)} baseline page(s) re-rendered, "
                  f"each named by the commit that moves it.")
        else:
            print(f"[baseline] OK: this range moves no baseline page.")
        return 0
    for problem in problems:
        print(f"[baseline] FATAL: {problem}", file=sys.stderr)
    print(f"[baseline] {len(problems)} problem(s). The suite compares the "
          f"renderer with these images; a change to them is a change to what "
          f"'unchanged' means, so it is declared by hand or not at all.",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
