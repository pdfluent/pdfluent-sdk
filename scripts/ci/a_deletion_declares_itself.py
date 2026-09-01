#!/usr/bin/env python3
"""A branch that deletes a file master has must say so in the commit that does it.

WHY (#297)

Three times on 01-09-2026 a stale branch would have silently reverted work on
master. The third was a pull request undoing files another had added twenty
minutes earlier, by the same author. Nothing conflicted, nothing was red,
GitHub reported MERGEABLE throughout.

That is not a gap in the merge machinery. A branch cut before some work exists
carries the ABSENCE of that work as content, and an absence never produces a
textual conflict -- so `MERGEABLE` is true precisely when it is least
informative. **A branch is dangerous in proportion to how long it has been open,
and that danger is invisible to every signal the interface shows.**

WHY A TRAILER AND NOT A FILE

Deliberate deletions are ordinary: #1610 removed three workflows on purpose,
#1617 three more. So the guard needs "explained" to be machine-checkable rather
than a comment somebody wrote in a description. A separate register would drift
-- the deletion moves between rebases and the entry does not follow it. A trailer
lives in the commit that performs the deletion, travels with it through every
rebase and cherry-pick, and cannot be true of a commit that no longer deletes
anything:

    Removes-deliberately: .github/workflows/avrt.yml

Both directions, because that is what makes a declaration worth reading:

  * a deletion with no trailer naming it FAILS -- the thing this exists for;
  * a trailer naming a path this branch does not delete FAILS too. An
    explanation that outlives its subject reads as current and quietly covers
    whatever next occupies that name.

Usage: a_deletion_declares_itself.py [--base github/master] [--head HEAD]
"""
from __future__ import annotations
import argparse, os, subprocess, sys


def clean_env() -> dict[str, str]:
    """git must read the repository we are standing in, not the caller's.

    Inside a hook GIT_DIR and GIT_WORK_TREE name the real repository and git
    ignores where you point it. test_no_test_can_touch_the_real_repo.py caught
    this guard itself once the branch was rebased onto the #1641 fix -- the
    lint's other findings had been masking it.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str) -> tuple[int, str]:
    r = subprocess.run(["git", *args], capture_output=True, text=True,
                       env=clean_env())
    return r.returncode, r.stdout


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="github/master")
    ap.add_argument("--head", default="HEAD")
    ap.add_argument("--exact-base", action="store_true",
                    help="compare against --base itself instead of the merge "
                         "base. A push event reports where the branch actually "
                         "WAS; deriving a merge base from it loses anything "
                         "added between the common ancestor and that tip, so a "
                         "force-push could drop a file and report nothing. A "
                         "pull request wants the merge base, because its base "
                         "branch may have moved on. (codex, #1635)")
    args = ap.parse_args(argv[1:])

    if args.exact_base:
        rc, base_sha = git("rev-parse", "--verify", f"{args.base}^{{commit}}")
    else:
        rc, base_sha = git("merge-base", args.base, args.head)
    base_sha = base_sha.strip()
    if rc != 0 or not base_sha:
        # Never a silent pass. Without a base there is no question to answer, and
        # answering "no deletions" would be a verdict about a comparison that did
        # not happen.
        print(f"[deletions] FATAL: no merge base between {args.base} and "
              f"{args.head}. Fetch the base branch first; refusing to report a "
              "clean result for a comparison that never ran.", file=sys.stderr)
        return 2

    # If the base resolves to the head, there is no range and every branch looks
    # clean. That is not a pass, it is a comparison that did not happen -- and it
    # is exactly what `--base origin/master` produced on a push to master, where
    # the ref already points at the revision being pushed. The caller was fixed;
    # this refuses too, because the next caller will be written by somebody who
    # has not read that fix. (codex, #1635)
    head_sha = git("rev-parse", args.head)[1].strip()
    if base_sha == head_sha:
        print(f"[deletions] FATAL: the base resolves to {args.head} itself, so the "
              "range is empty and no deletion could be found. Pass the revision "
              "this work started from, not the one it produced.", file=sys.stderr)
        return 2

    # -z, because core.quotePath defaults to on and git would otherwise C-quote
    # any path outside ASCII -- "benchmarks/runs/\316\262.md" for a path holding
    # a beta. The trailer carries the literal path, so the two spellings never
    # match and a correctly declared deletion is reported as both undeclared and
    # misplaced at once. This repo already tracks such paths. (codex, #1635)
    rc, out = git("diff", "--diff-filter=D", "--name-only", "-z",
                  f"{base_sha}..{args.head}")
    if rc != 0:
        # An ignored return code made empty stdout look like an empty set of
        # deletions: with a bad --head the diff failed and the guard reported OK
        # having compared nothing. A command that did not run is not a clean
        # answer. (codex, #1635)
        print(f"[deletions] FATAL: `git diff {base_sha}..{args.head}` failed "
              f"(exit {rc}). Refusing to report a clean branch for a comparison "
              "that did not happen.", file=sys.stderr)
        return 2
    deleted = {p for p in out.split("\0") if p.strip()}

    # The deletion transition that causes the FINAL absence, not "some commit
    # deleted it and some commit declared it".
    #
    # Two versions were wrong before this one. Set-wide over the range let a
    # trailer in commit B excuse a deletion in commit A. Per-commit fixed that
    # but still asked only "is there a commit that both deletes and declares" --
    # so A deletes P with a trailer, B restores P, C deletes P with none, and the
    # branch passed while claiming "each declared by the commit that performs
    # it". The declaration belonged to a deletion that had been undone.
    #
    # What has to carry the trailer is the LAST commit that removes the path,
    # because that is the one whose effect survives to the tip. (codex, #1635)
    rc, log = git("log", "--reverse", "--format=%H%x00%B%x1e",
                  f"{base_sha}..{args.head}")
    if rc != 0:
        print(f"[deletions] FATAL: `git log {base_sha}..{args.head}` failed "
              f"(exit {rc}). The range could not be read, so no commit was "
              "examined.", file=sys.stderr)
        return 2
    last_deleter: dict[str, str] = {}     # path -> sha of the last commit removing it
    trailers: dict[str, set[str]] = {}    # sha -> paths it declares
    deletes: dict[str, set[str]] = {}     # sha -> paths it removes
    for entry in log.split("\x1e"):
        if not entry.strip():
            continue
        sha, _, body = entry.partition("\x00")
        sha = sha.strip()
        if not sha:
            continue
        named = {line.split(":", 1)[1].strip()
                 for line in body.splitlines()
                 if line.lower().startswith("removes-deliberately:")
                 and line.split(":", 1)[1].strip()}
        # A merge deletes nothing of its own. Against its first parent it shows
        # every path the merged side removed, so on a `pull_request` run -- where
        # actions/checkout resolves the SYNTHETIC merge commit GitHub builds from
        # base and head -- that merge would become the last deleter of everything
        # in the PR, and its generated message carries no trailer. A correctly
        # declared deletion would fail. The same holds for an ordinary merge
        # commit landing on master. The deletion belongs to the commit on the
        # side branch that performed it, so merges are skipped when choosing the
        # last deleter. (codex, #1635)
        parents = git("rev-list", "--parents", "-n", "1", sha)[1].split()[1:]
        _, own = git("show", "--diff-filter=D", "--name-only", "--format=", "-z",
                     "--first-parent", "-m", sha)
        removed_here = {x for x in own.split("\0") if x.strip()}

        if len(parents) > 1:
            # A merge's OWN deletion is one the merge performed, not one it
            # inherited. Against its first parent a merge shows everything the
            # other side removed -- which on a `pull_request` run is the whole
            # PR, because actions/checkout resolves the synthetic merge GitHub
            # builds from base and head, and its generated message has no
            # trailer.
            #
            # The discriminator is the other parents. If the path is already
            # absent in ANY parent, the merge merely took that side. If it is
            # present in EVERY parent and absent in the result, the merge itself
            # removed it -- a conflict resolution that drops a file -- and it can
            # and must declare it. Discarding every merge deletion outright made
            # such a commit report as undeclared AND misplaced at once.
            # (codex, #1635)
            inherited = set()
            for path in removed_here:
                for par in parents[1:]:
                    if git("cat-file", "-e", f"{par}:{path}")[0] != 0:
                        inherited.add(path)
                        break
            removed_here = removed_here - inherited

        trailers[sha] = named
        deletes[sha] = removed_here
        for path in removed_here:
            last_deleter[path] = sha

    # With --exact-base on a force-push, a path can be absent from the new
    # history entirely: it exists at the old tip and no commit in base..head ever
    # touched it, so nothing can be its "last deleter". Requiring the trailer on
    # the deleting commit is then impossible to satisfy, and a trailer on the new
    # tip was reported as misplaced. A deliberate force-push removal could not be
    # declared at all. In this mode any commit in the range may carry it.
    # (codex, #1635)
    orphaned = {p for p in deleted if p not in last_deleter}
    if args.exact_base and orphaned:
        anywhere = set().union(*trailers.values()) if trailers else set()
        for path in sorted(orphaned):
            if path in anywhere:
                who = next(sha for sha, named in trailers.items() if path in named)
                last_deleter[path] = who
                deletes.setdefault(who, set()).add(path)

    declared: dict[str, str] = {}
    for path, sha in last_deleter.items():
        if path in trailers.get(sha, set()):
            declared[path] = sha[:8]

    # A trailer on a commit that removes no such path is misplaced. Kept separate
    # from the stale list: they need different sentences, and the previous
    # version put both in one list and then looked every entry up in `declared`,
    # which raised KeyError on the misplaced ones -- a traceback instead of the
    # remediation the message was written to give. (codex, #1635)
    misplaced: list[str] = []
    for sha, named in trailers.items():
        for path in sorted(named - deletes.get(sha, set())):
            misplaced.append(f"{path}  (declared in {sha[:8]}, which does not "
                             "delete it)")

    undeclared = sorted(deleted - set(declared))
    stale = sorted(set(declared) - deleted)

    if undeclared:
        print(f"[deletions] FAIL: {len(undeclared)} file(s) present on "
              f"{args.base} are deleted here and nothing says why:", file=sys.stderr)
        for p in undeclared:
            print(f"    {p}", file=sys.stderr)
        print("\n  Add a trailer to the commit that removes it:\n"
              "      Removes-deliberately: <path>\n"
              "  A deletion nobody declared is how a branch that predates the work\n"
              "  silently reverts it -- no conflict, no red, MERGEABLE true.",
              file=sys.stderr)
    if misplaced:
        print(f"[deletions] FAIL: {len(misplaced)} trailer(s) sit on a commit that "
              "does not perform the deletion:", file=sys.stderr)
        for m in misplaced:
            print(f"    {m}", file=sys.stderr)
        print("\n  Move the trailer to the commit that removes the file. If a later\n"
              "  commit removes it again, that commit needs the trailer -- the one\n"
              "  whose effect reaches the tip is the one being explained.",
              file=sys.stderr)
    if stale:
        print(f"[deletions] FAIL: {len(stale)} trailer(s) name a path this branch "
              "does not delete:", file=sys.stderr)
        for p in stale:
            print(f"    {p}  (declared in {declared[p]})", file=sys.stderr)
        print("\n  Put the trailer on the commit that performs the deletion, or\n"
              "  remove it. A trailer that sits on a different commit is not\n"
              "  travelling with the thing it explains -- and a rebase is exactly\n"
              "  where the two come apart.", file=sys.stderr)
    if undeclared or stale or misplaced:
        return 1

    if deleted:
        print(f"[deletions] OK: {len(deleted)} deletion(s), each declared by the "
              "commit that performs it.")
    else:
        print(f"[deletions] OK: this branch deletes nothing that {args.base} has.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
