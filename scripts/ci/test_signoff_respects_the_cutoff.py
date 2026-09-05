#!/usr/bin/env python3
"""The sign-off gate judges what arrives after the cutoff, and nothing before it.

`licence_signoff.py` said in its own comment that "the gate applies to what
arrives from here on" and then walked the whole `github/master..HEAD` range with
no date filter. On a long-lived branch that meant demanding a certification from
every commit the branch ever carried: measured on #1543, **329 of 338**, none of
them written after the decision and none of them that push's to certify.

A promise a comment makes and the code does not keep is worse than no promise --
it reads as a bound, so nobody looks.

Built on a throwaway repository rather than on this one: a fixture that needs the
real history cannot show the boundary from both sides, and a test that writes
commits must never be pointed at the repository it is defending.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HIER = pathlib.Path(__file__).resolve().parent
GUARD = HIER / "licence_signoff.py"

sys.path.insert(0, str(HIER))
from fixture_env import sealed_env  # noqa: E402

fouten: list[str] = []
gedaan = 0


def expect(wat: str, ok: bool, detail: str = "") -> None:
    global gedaan
    gedaan += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fouten.append(wat)


# Dropping GIT_* keeps a hook's variables OUT of the scratch repository; it does
# nothing about writes going the other way. `git config user.name` outside a
# repository lands in the developer's global file, and that is how a fixture
# identity ended up in a real worktree. sealed_env() also sets
# GIT_CONFIG_NOSYSTEM and points GIT_CONFIG_GLOBAL at an empty file, so a write
# that misses the sandbox has nowhere real to land.
#
# It refuses GIT_* from the caller, which is why the commit dates below are set
# with `--date` rather than GIT_AUTHOR_DATE: the guard reads `%at`, and `--date`
# is exactly the author date. Handing the variable back through the helper would
# be asking it to open the door it exists to close.


def cutoff() -> int:
    sys.path.insert(0, str(HIER))
    from every_commit_since_the_cutoff_is_signed import CUT_AT
    return CUT_AT


def bouw(tmp: pathlib.Path, cut: int) -> None:
    """A repository with one commit before the cutoff and one after, neither signed."""
    def git(*a: str) -> subprocess.CompletedProcess:
        return subprocess.run(["git", *a], cwd=tmp, capture_output=True, text=True,
                              env=sealed_env(cwd=tmp), check=False)

    git("init", "-q", "-b", "master")
    git("config", "user.name", "Fixture")
    git("config", "user.email", "fixture@example.invalid")
    # A root commit first, so a range holding ONLY the before-cutoff commit
    # exists: `base~1..base` needs base to have a parent.
    (tmp / "root.txt").write_text("root\n")
    git("add", "root.txt")
    git("commit", "-q", "-m", "root", f"--date=@{cut - 172800} +0000")
    (tmp / "a.txt").write_text("before\n")
    git("add", "a.txt")
    git("commit", "-q", "-m", "before the cutoff, unsigned", f"--date=@{cut - 86400} +0000")
    git("branch", "-f", "base")
    (tmp / "b.txt").write_text("after\n")
    git("add", "b.txt")
    git("commit", "-q", "-m", "after the cutoff, unsigned", f"--date=@{cut + 86400} +0000")


def git(tmp: pathlib.Path, *a: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *a], cwd=tmp, capture_output=True, text=True,
                          env=sealed_env(cwd=tmp), check=False)


def draai(tmp: pathlib.Path, bereik: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(GUARD), bereik],
                          cwd=tmp, capture_output=True, text=True, env=sealed_env(cwd=tmp))


def main() -> int:
    if not GUARD.is_file():
        print(f"SKIPPED (not a pass): {GUARD} is missing, so nothing was checked",
              file=sys.stderr)
        return 3

    cut = cutoff()
    print("the sign-off gate respects the cutoff")

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        bouw(tmp, cut)

        # The commit AFTER the cutoff is unsigned: it must be refused, by name.
        r = draai(tmp, "base..HEAD")
        expect("an unsigned commit written after the cutoff is refused",
               r.returncode == 1 and "Signed-off-by" in (r.stdout + r.stderr),
               (r.stdout + r.stderr)[-200:])

        # The commit BEFORE it is unsigned too, and must NOT be demanded. This is
        # the half the old code got wrong.
        r2 = draai(tmp, "base~1..base")
        expect("an unsigned commit written before the cutoff is left alone",
               r2.returncode == 0 and "not required" in r2.stdout,
               (r2.stdout + r2.stderr)[-200:])

        # And an empty result after filtering is a real answer, not a silent pass
        # over a range nobody read: the range floor still applies to the range.
        expect("  and says so rather than passing quietly",
               "adds no commit written after the cutoff" in r2.stdout,
               r2.stdout[-160:])

        # THE CO-AUTHOR, which is the half #223 names and nothing was reading.
        #
        # This commit is signed off by its author and is therefore green under
        # every version of this gate before today -- while the second person who
        # wrote it certified nothing. A co-author is recorded in this trailer and
        # nowhere else: not in %ae, not in %ce.
        (tmp / "c.txt").write_text("co\n")
        git(tmp, "add", "c.txt")
        git(tmp, "commit", "-q", "-m",
            "after the cutoff, signed by one of its two authors\n\n"
            "Co-authored-by: Someone Else <else@example.invalid>\n"
            "Signed-off-by: Fixture <fixture@example.invalid>\n",
            f"--date=@{cut + 90000} +0000")
        r3 = draai(tmp, "HEAD~1..HEAD")
        expect("a co-author who signed off on nothing is refused",
               r3.returncode == 1, (r3.stdout + r3.stderr)[-300:])
        expect("  and the refusal names that co-author",
               "else@example.invalid" in (r3.stdout + r3.stderr),
               (r3.stdout + r3.stderr)[-300:])

        # Their own line is what certifies their half. With it, green.
        git(tmp, "commit", "-q", "--amend", "-m",
            "after the cutoff, signed by both of its authors\n\n"
            "Co-authored-by: Someone Else <else@example.invalid>\n"
            "Signed-off-by: Fixture <fixture@example.invalid>\n"
            "Signed-off-by: Someone Else <else@example.invalid>\n",
            f"--date=@{cut + 90000} +0000")
        r4 = draai(tmp, "HEAD~1..HEAD")
        expect("  and passes once the co-author signs their own half",
               r4.returncode == 0, (r4.stdout + r4.stderr)[-300:])

    bron = GUARD.read_text()
    expect("the cutoff is imported, not copied",
           "from every_commit_since_the_cutoff_is_signed import CUT_AT" in bron,
           "a second copy of the number is a second answer to one question")
    expect("merges are skipped",
           "--no-merges" in bron,
           "a merge carries a generated message and can never hold a sign-off")
    expect("the AUTHOR date decides, not the committer date",
           "%at" in bron and "committer date" in bron,
           "the committer date moves on rebase and would drag old commits across")

    print(f"\n  {gedaan} assertion(s) ran, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
