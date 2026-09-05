#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The guard is exercised on repositories built for it, not on this one.

This repository has two archive tags and one of them is recorded, so running the
guard here proves it can print. What it cannot prove here is the refusal, which
is the half that matters: a tag over commits carrying a personal address must
stop the sweep. So the cases below build throwaway repositories with identities
chosen per commit, and the guard is run against them.

The addresses used are invented -- `someone@example.invalid` -- for the same
reason the guard prints no address: this file ships with the tree. `.invalid` is
reserved by RFC 2606 and resolves nowhere, so it cannot become a real one.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HIER = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HIER))
from fixture_env import sealed_env  # noqa: E402

WACHTER = HIER / "an_archive_tag_pins_no_personal_address.py"

ALIAS = "10383561+jasperdew@users.noreply.github.com"
PERSOONLIJK = "someone@example.invalid"

fouten: list[str] = []
geteld = 0


def verwacht(wat: str, ok: bool, detail: str = "") -> None:
    global geteld
    geteld += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fouten.append(wat)


def git(pad: pathlib.Path, *args: str):
    """Every call sealed: no GIT_*, no system config, an empty global one.

    Identity comes from the sandbox repository's LOCAL config, set per commit by
    `identiteit()` below, and not from GIT_AUTHOR_EMAIL -- `sealed_env` refuses
    those by design, because accepting GIT_* from a caller after stripping it
    from the environment is a doorman holding the back door open. Local config
    is the right place anyway: it lives inside the throwaway repository and
    disappears with it.
    """
    return subprocess.run(["git", "-C", str(pad), *args],
                          capture_output=True, text=True,
                          env=sealed_env(cwd=str(pad)))


def identiteit(pad: pathlib.Path, email: str) -> None:
    git(pad, "config", "user.name", "t")
    git(pad, "config", "user.email", email)


def bouw(pad: pathlib.Path, adressen: list[str]) -> None:
    """A master, and beside it the commits an archive tag would keep.

    Beside it, not on it: the guard measures `master..<tag>`, so commits made on
    master itself leave that range empty and every case passes for the wrong
    reason. The archived work has to be off the branch it is compared against,
    which is what a branch nobody merged actually looks like.
    """
    git(pad, "init", "-q", "-b", "master")
    identiteit(pad, ALIAS)
    (pad / "a.txt").write_text("base\n")
    git(pad, "add", "a.txt")
    git(pad, "commit", "-q", "-m", "base")
    git(pad, "checkout", "-q", "-b", "werk")
    for i, adres in enumerate(adressen):
        identiteit(pad, adres)
        (pad / f"b{i}.txt").write_text(f"{i}\n")
        git(pad, "add", f"b{i}.txt")
        git(pad, "commit", "-q", "-m", f"work {i}")
    identiteit(pad, ALIAS)
    git(pad, "tag", "keep/archived")


def draai(pad: pathlib.Path, *extra: str):
    # `master` is the last name the guard looks for, and the only one a
    # throwaway repository has -- so the base resolves without a remote.
    # --no-register because these are not this repository: every recorded row
    # names a tag that cannot exist here, and without the flag the expiry check
    # answers every case before the case is reached.
    return subprocess.run([sys.executable, str(WACHTER), "--no-register", *extra],
                          capture_output=True, text=True, cwd=str(pad),
                          env=sealed_env())


def main() -> int:
    if not WACHTER.is_file():
        print(f"SKIPPED (not a pass): {WACHTER} is missing, so nothing was checked",
              file=sys.stderr)
        return 3
    print("an archive tag pins no personal address")

    with tempfile.TemporaryDirectory() as d:
        # A tag over commits that all carry the alias is fine.
        schoon = pathlib.Path(d) / "schoon"
        schoon.mkdir()
        bouw(schoon, [ALIAS, ALIAS])
        r = draai(schoon)
        verwacht("a tag over alias-only commits passes", r.returncode == 0,
                 r.stdout + r.stderr)

        # THE REFUSAL. One commit is enough: the tag keeps all of them.
        vuil = pathlib.Path(d) / "vuil"
        vuil.mkdir()
        bouw(vuil, [ALIAS, PERSOONLIJK])
        r = draai(vuil)
        verwacht("one non-alias commit under the tag refuses", r.returncode == 1,
                 r.stdout + r.stderr)
        verwacht("and it says what a tag promises, not just that it failed",
                 "promise" in r.stderr or "promises" in r.stderr, r.stderr[-200:])
        verwacht("and it does not print the address it refused",
                 PERSOONLIJK not in r.stdout + r.stderr,
                 "this file and the guard both ship with the tree")

        # The committer alone is enough: a rebase writes that one, and a commit
        # whose author is an alias while its committer is not publishes just as
        # widely. Reading only %ae would have let every rebased branch through.
        alleen_committer = pathlib.Path(d) / "committer"
        alleen_committer.mkdir()
        bouw(alleen_committer, [ALIAS])
        identiteit(alleen_committer, PERSOONLIJK)
        git(alleen_committer, "commit", "-q", "--amend", "--no-edit",
            f"--author=t <{ALIAS}>")
        identiteit(alleen_committer, ALIAS)
        git(alleen_committer, "tag", "-f", "keep/archived")
        r = draai(alleen_committer)
        verwacht("a non-alias COMMITTER refuses too", r.returncode == 1,
                 (r.stdout + r.stderr)[-200:])

        # --candidate answers before the tag exists, which is the only moment
        # the answer is free: after `git push --tags` the ref is published.
        vooraf = pathlib.Path(d) / "vooraf"
        vooraf.mkdir()
        bouw(vooraf, [PERSOONLIJK])
        git(vooraf, "tag", "-d", "keep/archived")
        r = draai(vooraf)
        verwacht("with no tag and no candidate, nothing is measured and it says so",
                 r.returncode == 0 and "nothing was measured" in r.stdout,
                 (r.stdout + r.stderr)[-200:])
        r = draai(vooraf, "--candidate", "HEAD")
        verwacht("a candidate ref is judged before it becomes a tag",
                 r.returncode == 1, (r.stdout + r.stderr)[-200:])

        # A base that does not resolve prints no commits and exits 0, which
        # reads exactly like a clean tag. It has to refuse instead.
        geen_basis = pathlib.Path(d) / "geenbasis"
        geen_basis.mkdir()
        bouw(geen_basis, [PERSOONLIJK])
        git(geen_basis, "branch", "-m", "master", "hoofdlijn")
        r = draai(geen_basis)
        verwacht("no master to measure against is a refusal, not a pass",
                 r.returncode == 1 and "cannot be established" in r.stderr,
                 (r.stdout + r.stderr)[-200:])

        # The recorded row may shrink and may not outlive its subject. With the
        # register ON in a repository that has no such tag, that is exactly the
        # state it must refuse -- which is also why every other case here turns
        # the register off.
        weg = pathlib.Path(d) / "weg"
        weg.mkdir()
        bouw(weg, [ALIAS])
        r = subprocess.run([sys.executable, str(WACHTER)],
                           capture_output=True, text=True, cwd=str(weg),
                           env=sealed_env())
        verwacht("a recorded tag that no longer exists is refused, not ignored",
                 r.returncode == 1 and "no longer exists" in r.stderr,
                 (r.stdout + r.stderr)[-200:])

    # --- THE MUTATION --------------------------------------------------------
    # Read only the author and every rebased branch walks through. Without this,
    # the committer case above could pass because nothing is being compared.
    bron = WACHTER.read_text()
    gebroken = bron.replace("if not (is_allowed(auteur) and is_allowed(committer)):",
                            "if not is_allowed(auteur):", 1)
    verwacht("the mutation could be applied", gebroken != bron)
    mutant = HIER / "_archive_tag_mutant.py"
    try:
        mutant.write_text(gebroken)
        with tempfile.TemporaryDirectory() as d:
            pad = pathlib.Path(d) / "m"
            pad.mkdir()
            bouw(pad, [ALIAS])
            identiteit(pad, PERSOONLIJK)
            git(pad, "commit", "-q", "--amend", "--no-edit",
                f"--author=t <{ALIAS}>")
            identiteit(pad, ALIAS)
            git(pad, "tag", "-f", "keep/archived")
            r = subprocess.run([sys.executable, str(mutant), "--no-register"],
                               capture_output=True, text=True, cwd=str(pad),
                               env=sealed_env())
            verwacht("reading only the author lets a rebased commit through",
                     r.returncode == 0,
                     "the mutation did not change the answer, so the committer "
                     "half is not what makes this test red")
    finally:
        mutant.unlink(missing_ok=True)

    print(f"\n  {geteld} assertion(s) ran, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
