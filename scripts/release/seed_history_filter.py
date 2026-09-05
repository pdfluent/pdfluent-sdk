#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What a published HISTORY may carry, and the proof that it carries nothing else.

`docs/PUBLIC_TREE.toml` answers "does this file go public" for the tree.
`scripts/release/seed_public_repo.sh` publishes a *history*, and a history carries
every blob that was ever committed -- so until this file existed the two
mechanisms disagreed and the seeding one filtered nothing at all. That is the
irreversible half: a tree can be corrected with a commit, a published object
cannot be taken back from anyone who has its id (#260 measured exactly that, four
days after a rewrite that was supposed to end it).

ONE MANIFEST, NOT A SECOND LIST
===============================
`docs/licensing/history-rewrite-plan.md` says the seeding step "needs its own
`--invert-paths` list, and this is it", pointing at seven golden documents. That
sentence was written before `crates/xfa-golden-tests/` and `test-data/` were on
`[internal].paths`; measured today, all seven and the signed form of #215 are
already inside those two prefixes. A hand-kept copy of them here would be a
second list that drifts from the first, and the one that drifts is the one nobody
runs.

So the filter is derived, not written: every path that has EVER existed in the
source history, passed through `simulate_public_tree.wordt_gepubliceerd` -- the
same predicate the tree exporter and `geen_interne_zaken --boom` use. That is
also what closes the gap the plan was actually pointing at, which is not the
seven names but the shape: a file that lived at a different path before it was
moved, or was deleted before the manifest was written, is in no list of current
paths and is in the history all the same.

CONTENT, NOT ONLY NAMES
=======================
A path filter is a name filter, and #260's whole finding is that names are not
what stays fetchable. So the withdrawn blobs are also collected by object id --
every version of every excluded path -- minus any id that also occurs at a
published path in the source, because identical content at a published path is
not evidence of a leak (the empty blob would otherwise fail every run). Nothing
in that set may be reachable afterwards, whatever it is called.

WHAT IS VERIFIED, AND WHY THESE THREE
=====================================
    paths      no surviving object sits at a path the manifest calls internal
    objects    no withdrawn blob id is reachable under any name
    identity   every author and committer is an alias `commits_use_the_noreply_alias`
               accepts -- 6746 commits in this repository carry a personal address,
               and seeding would publish every one of them for the first time
    terms      `geen_interne_zaken` finds nothing in the surviving messages or the
               surviving file content

The terms scan is the reason this reads blobs and not only names: a customer name
inside a published file is exactly as public as one in a published file NAME, and
the tree exporter already learned that lesson (`geen_interne_zaken --boom` reads
content because reading names alone let `docs/ZZQBETA-notes.md` through).

Subcommands:
    plan <mirror> --paths <f> --withdrawn <f>
                        the paths that must not travel (NUL-separated) and the
                        object ids that must not travel, from one walk
    preflight <mirror>  everything the rewrite needs, asked before the rewrite
    trees <mirror>      ref, blob id and path for every file that MAY travel
    alias <mirror>      the noreply alias non-alias identities are rewritten to
    allowed <mirror>    every address in this history that may stay as it is
    verify <mirror> --withdrawn <file>
                        the four checks above over the REWRITTEN mirror

Exit codes:
    0  the mirror carries only what the manifest allows
    1  it does not, or a check could not be made and therefore did not pass
"""
from __future__ import annotations

import argparse
import importlib.util
import pathlib
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / "scripts" / "ci"

# A blob larger than this is not read for terms. A term is a word; a 4 MB blob is
# a PDF, a font or a lockfile, and reading every one of them over a full history
# turns a check into a reason to switch the check off. Named rather than inlined
# so the report can say what it did not read.
MAX_BLOB = 1 << 20


def _module(naam: str):
    """Import a guard by path, so there is one copy of each rule and not two."""
    pad = CI / f"{naam}.py"
    spec = importlib.util.spec_from_file_location(naam, pad)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _git(mirror: pathlib.Path, *args: str):
    return subprocess.run(["git", "-C", str(mirror), *args],
                          capture_output=True, check=True, text=True)


def manifest() -> dict:
    stp = _module("simulate_public_tree")
    return stp, tomllib.loads(stp.MANIFEST.read_text(encoding="utf-8"))


def _hash_bytes(mirror: pathlib.Path) -> int:
    soort = _git(mirror, "rev-parse", "--show-object-format").stdout.strip()
    return 32 if soort == "sha256" else 20


def alle_paden(mirror: pathlib.Path) -> dict[str, set[str]]:
    """Every path that ever existed, and the object ids that ever stood at it.

    WHY THIS WALKS TREES INSTEAD OF ASKING `git rev-list --objects`

    That command is the obvious answer and it is wrong here: it prints each
    OBJECT once, with the first path it happened to be reached through. Two paths
    holding identical content therefore yield one line, and which of the two
    survives is an accident of traversal order. Measured on a fixture: a file
    deleted from `README.md` and recreated under `test-data/` was reported only at
    the second path -- so had it gone the other way, an internal path would have
    been absent from the exclusion list and the file would have been published,
    with the script reporting success.

    A path filter built on "one path per object" is a path filter with holes in
    it, and the holes are invisible. So the trees are walked instead: every root
    tree of every commit, every subtree under it, memoised on (tree, prefix)
    because a directory's content repeats across thousands of commits. Measured on
    this repository: 34 057 trees, seconds.
    """
    breedte = _hash_bytes(mirror)
    roots = {r.strip() for r in _git(mirror, "log", "--all", "--format=%T").stdout.splitlines()
             if r.strip()}
    uit: dict[str, set[str]] = {}
    gezien: set[tuple[str, str]] = set()
    werk = [(t, "") for t in sorted(roots)]

    p = subprocess.Popen(["git", "-C", str(mirror), "cat-file", "--batch"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        while werk:
            oid, prefix = werk.pop()
            if (oid, prefix) in gezien:
                continue
            gezien.add((oid, prefix))
            p.stdin.write((oid + "\n").encode())
            p.stdin.flush()
            kop = p.stdout.readline().decode(errors="replace").split()
            if len(kop) != 3 or kop[1] != "tree":
                # A missing object is not something to walk past quietly, but a
                # tag or commit here would be a caller error, not data.
                continue
            ruw = p.stdout.read(int(kop[2]) + 1)[:int(kop[2])]
            i = 0
            while i < len(ruw):
                spatie = ruw.index(b" ", i)
                nul = ruw.index(b"\0", spatie)
                mode = ruw[i:spatie]
                naam = ruw[spatie + 1:nul].decode("utf-8", errors="surrogateescape")
                kind = ruw[nul + 1:nul + 1 + breedte].hex()
                i = nul + 1 + breedte
                pad = f"{prefix}{naam}"
                uit.setdefault(pad, set()).add(kind)
                if mode == b"40000":
                    werk.append((kind, pad + "/"))
    finally:
        p.stdin.close()
        p.stdout.close()
        p.wait()
    return uit


def gesplitst(mirror: pathlib.Path) -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """The historical paths, split into what may travel and what may not."""
    stp, m = manifest()
    paden = alle_paden(mirror)
    mag = {p: o for p, o in paden.items() if stp.wordt_gepubliceerd(p, m)}
    niet = {p: o for p, o in paden.items() if p not in mag}
    return mag, niet


def cmd_plan(a) -> int:
    """Both lists from ONE tree walk.

    Asked separately they were two walks of the same 34 000 trees for one answer,
    on a script whose slowest case is a test suite that runs it eight times.
    """
    mag, niet = gesplitst(a.mirror)
    a.paths.write_text("\0".join(sorted(niet)), encoding="utf-8")
    gepubliceerd: set[str] = set()
    for oids in mag.values():
        gepubliceerd |= oids
    ingetrokken: set[str] = set()
    for oids in niet.values():
        ingetrokken |= oids
    a.withdrawn.write_text(
        "".join(f"{o}\n" for o in sorted(ingetrokken - gepubliceerd)), encoding="utf-8")
    return 0


def cmd_preflight(a) -> int:
    """Everything the rewrite needs, asked BEFORE the rewrite.

    `geen_interne_zaken` refuses when the private term list is absent, and that
    refusal is right -- a scan that could not look did not pass. Discovering it
    after a filter-branch over 6746 commits is right and useless: the operator
    has waited out the whole rewrite to be told the check was never possible.
    """
    _module("geen_interne_zaken").alle_regels()
    print("[seed-preflight] OK: the private term list is readable")
    return 0


def cmd_trees(a) -> int:
    """Every ref, every path that MAY travel, and the blob standing at it.

    The original script's central claim is that only messages change and "not one
    byte of any file may differ". A path filter breaks the check that proved it --
    trees now differ on purpose -- so the claim is restated rather than dropped:
    of the files that stay, every one must still be the same blob. Run before the
    rewrite and after it, and diffed.
    """
    stp, m = manifest()
    refs = _git(a.mirror, "for-each-ref", "--format=%(refname)",
                "refs/heads/*", "refs/tags/*").stdout.split()
    for ref in refs:
        # `-z` and the DEFAULT format, not `--format`. `ls-tree` C-QUOTES any
        # path holding a non-ASCII byte -- leading double quote and all -- so the
        # string handed to `wordt_gepubliceerd` no longer starts with the
        # internal prefix and the file reads as publishable. `-z` is what turns
        # quoting off, and measured on git 2.52 it does that for the default
        # output and NOT for `--format`, which keeps quoting `%(path)` whatever
        # the terminator is. So the mode and type are parsed off rather than
        # asked for. It then appears in
        # the before-list, is correctly removed by the rewrite, and the
        # comparison reports that the rewrite "changed a file that was supposed
        # to stay".
        #
        # Measured by running the seeding over the real history: eight such rows
        # across five tags, every one an internal file and every one a false
        # accusation. Same defect `simulate_public_tree.tracked()` documents for
        # `git ls-files`, in a different command, one function away.
        r = subprocess.run(["git", "-C", str(a.mirror), "ls-tree", "-r", "-z",
                            f"{ref}^{{tree}}"], capture_output=True, text=True)
        if r.returncode != 0:
            continue
        for regel in r.stdout.split("\0"):
            if not regel:
                continue
            kop, _, pad = regel.partition("\t")
            oid = kop.split()[-1]
            if pad and stp.wordt_gepubliceerd(pad, m):
                print(f"{ref}\t{oid}\t{pad}")
    return 0


def cmd_withdrawn(a) -> int:
    """Object ids that must not survive, whatever they end up being called."""
    mag, niet = gesplitst(a.mirror)
    gepubliceerd: set[str] = set()
    for oids in mag.values():
        gepubliceerd |= oids
    ingetrokken: set[str] = set()
    for oids in niet.values():
        ingetrokken |= oids
    # Content that also stands at a published path is not withdrawn content. The
    # empty blob is the everyday case and would fail every run; a shared LICENSE
    # header is the next one.
    for oid in sorted(ingetrokken - gepubliceerd):
        print(oid)
    return 0


# --------------------------------------------------------------------- verify --

def _tekstblobs(mirror: pathlib.Path, oids: list[str]):
    """Yield (oid, text) for every blob small enough and textual enough to read.

    One `git cat-file --batch` for the whole history rather than a process per
    object: over a real history that is sixty thousand objects, and the version of
    this that spawned per blob took long enough that it would have been run once
    and then skipped.

    Written one request at a time rather than writing the whole list and then
    reading it back. The second shape deadlocks: sixty thousand ids is megabytes,
    the pipe buffer is not, and the writer blocks on a reader that has not started.
    """
    if not oids:
        return
    p = subprocess.Popen(["git", "-C", str(mirror), "cat-file", "--batch"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        for oid in oids:
            p.stdin.write((oid + "\n").encode())
            p.stdin.flush()
            kop = p.stdout.readline().decode(errors="replace").split()
            if len(kop) != 3:
                continue
            soort, grootte = kop[1], int(kop[2])
            ruw = p.stdout.read(grootte + 1)[:grootte]
            if soort != "blob" or grootte > MAX_BLOB:
                continue
            if b"\0" in ruw[:8192]:
                continue
            yield oid, ruw.decode("utf-8", errors="ignore")
    finally:
        p.stdin.close()
        p.stdout.close()
        p.wait()


def _fork_uitzondering(paden_van_oid: dict[str, set[str]]):
    """`commercieel` does not apply inside a forked crate, exactly as in the tree.

    `MRR` in hayro-jpeg2000 is a JPEG2000 coding pass, not a revenue term. The
    tree scan already carves that out; a history scan that did not would go red on
    somebody else's code and be switched off within the week. Only when EVERY
    path the blob ever had is inside a fork -- a blob that also lived in our own
    code is ours.
    """
    hs = _module("header_sweep")

    def in_fork(pad: str) -> bool:
        deel = pad.split("/")
        return (len(deel) > 1 and deel[0] == "crates"
                and hs.is_geforkt(hs.CRATES / deel[1]))

    def geforkt(oid: str) -> bool:
        paden = paden_van_oid.get(oid) or set()
        return bool(paden) and all(in_fork(p) for p in paden)

    return geforkt


def cmd_verify(a) -> int:
    mirror = a.mirror
    stp, m = manifest()
    gi = _module("geen_interne_zaken")
    identiteit = _module("commits_use_the_noreply_alias")

    fouten: list[str] = []

    # 1. Paths. Every object the rewritten history reaches, by the name it is
    #    reached through.
    paden = alle_paden(mirror)
    blijft_staan = sorted(p for p in paden if not stp.wordt_gepubliceerd(p, m))
    if blijft_staan:
        fouten.append(
            f"{len(blijft_staan)} path(s) the manifest calls internal are still in "
            f"the history, e.g. {', '.join(blijft_staan[:5])}")

    # 2. Content, under any name.
    if a.withdrawn and a.withdrawn.is_file():
        ingetrokken = {r.strip() for r in a.withdrawn.read_text().splitlines() if r.strip()}
        # Reachable objects, not the object database. `git push --mirror` sends
        # what the refs reach; an unreferenced blob that filter-branch left loose
        # in the local mirror is never published, and counting it here produced a
        # refusal on a fixture where nothing was wrong. What must be measured is
        # what leaves the machine.
        aanwezig = set()
        for oids in paden.values():
            aanwezig |= oids
        over = sorted(ingetrokken & aanwezig)
        if over:
            # The id is not printed. #260's finding is that an id IS the route to
            # the object, and a failing run's log is as public as the tree.
            fouten.append(f"{len(over)} withdrawn object(s) are still reachable, "
                          "under some name -- the ids are deliberately not printed")
    else:
        fouten.append("the withdrawn-object list was not readable, so content was "
                      "never checked; that is not a pass")

    # 3. Identity. `is_allowed` and not a second rule: the guard that refuses a
    #    personal address in a new commit is the guard that decides here.
    adressen = _git(mirror, "log", "--all", "--format=%ae%n%ce").stdout.split()
    niet_toegestaan = sorted({e for e in adressen if not identiteit.is_allowed(e)})
    if not adressen:
        fouten.append("the rewritten history holds no commits, so nothing was read")
    if niet_toegestaan:
        # Same reasoning as the guards this borrows from: the address is the
        # thing being kept out, so the count travels and the address does not.
        fouten.append(f"{len(niet_toegestaan)} identity/identities are not an alias "
                      "and would be published by the seeding")

    # 4. Internal terms, over the messages AND the surviving content.
    regels = gi.alle_regels()          # raises SystemExit when the list is absent
    berichten = _git(mirror, "log", "--all", "--format=%B").stdout
    treffers = gi.overtredingen(berichten)
    if treffers:
        naam, wat, ctx = treffers[0]
        wat, ctx = gi._toonbaar(naam, wat, ctx)
        fouten.append(f"{len(treffers)} internal term(s) in the commit messages, "
                      f"first [{naam}] {wat}")

    paden_van_oid: dict[str, set[str]] = {}
    for pad, oids in paden.items():
        for oid in oids:
            paden_van_oid.setdefault(oid, set()).add(pad)
    geforkt = _fork_uitzondering(paden_van_oid)

    inhoud: list[str] = []
    gelezen = 0
    for oid, tekst in _tekstblobs(mirror, sorted(paden_van_oid)):
        gelezen += 1
        # The same self-exemption the tree scan makes, and for the same written
        # reason: `geen_interne_zaken.py` spells `prijsstrategie` and `MRR` in its
        # own REGELS, that is ordinary trade vocabulary, and the terms that were
        # actually secret live outside the tree. Measured: without this the
        # seeding refuses on a file `geen_interne_zaken --boom` deliberately
        # allows -- one rule with two answers, which is the defect this file
        # avoids everywhere else by importing the predicate instead of restating
        # it. Only when EVERY path the blob ever had is that file.
        paden_hier = paden_van_oid.get(oid) or set()
        if paden_hier and paden_hier <= gi.EIGEN_BESTANDEN:
            continue
        fork = geforkt(oid)
        for naam, rx in regels:
            if fork and naam == "commercieel":
                continue
            mm = rx.search(tekst)
            if mm:
                waar = sorted(paden_van_oid[oid])[0]
                wat, _ = gi._toonbaar(naam, mm.group(0), "")
                waar, _ = gi._toonbaar(naam, waar, "")
                inhoud.append(f"[{naam}] {wat} -- {waar}")
    if inhoud:
        fouten.append(f"{len(inhoud)} internal term(s) in file content that would be "
                      f"published, first {inhoud[0]}")

    print(f"[seed-verify] {len(paden)} path(s), {gelezen} readable text blob(s), "
          f"{len(set(adressen))} distinct identity/identities")

    if fouten:
        print("[seed-verify] the rewritten history is not publishable:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[seed-verify] OK: no internal path, no withdrawn object, no personal "
          "address, no internal term")
    return 0


def cmd_alias(a) -> int:
    """The alias every non-alias identity is rewritten to.

    Derived from the source rather than written down here, and that is the point:
    an address invented in a script is an address nobody checked, while the most
    frequent noreply alias already in this history is one GitHub itself issued and
    that 858 published commits already carry. If there is none, this refuses --
    inventing `seed@invalid` would silently detach every commit from its author.
    """
    identiteit = _module("commits_use_the_noreply_alias")
    telling: dict[str, int] = {}
    for e in _git(a.mirror, "log", "--all", "--format=%ae%n%ce").stdout.split():
        if identiteit.NOREPLY.match(e):
            telling[e] = telling.get(e, 0) + 1
    if not telling:
        print("[seed-alias] no noreply alias occurs in this history, so there is "
              "nothing to rewrite the personal addresses TO. Pass --alias.",
              file=sys.stderr)
        return 1
    print(max(telling.items(), key=lambda kv: (kv[1], kv[0]))[0])
    return 0


def cmd_allowed(a) -> int:
    """Every address in this history that may stay as it is.

    The env-filter needs the answer as a fixed string it can test with `case`, so
    it is computed once here instead of per commit -- and by `is_allowed`, the
    predicate the guard that refuses a personal address in a new commit already
    uses. A second copy of that rule in shell would be a second thing to keep
    right.
    """
    identiteit = _module("commits_use_the_noreply_alias")
    for e in sorted({x for x in _git(a.mirror, "log", "--all",
                                     "--format=%ae%n%ce").stdout.split()}):
        if identiteit.is_allowed(e):
            print(e)
    return 0


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    for naam, fn in (("plan", cmd_plan), ("preflight", cmd_preflight),
                     ("trees", cmd_trees), ("alias", cmd_alias),
                     ("allowed", cmd_allowed), ("verify", cmd_verify)):
        s = sub.add_parser(naam)
        s.add_argument("mirror", type=pathlib.Path)
        s.set_defaults(fn=fn)
        if naam == "verify":
            s.add_argument("--withdrawn", type=pathlib.Path, default=None)
        if naam == "plan":
            s.add_argument("--paths", type=pathlib.Path, required=True)
            s.add_argument("--withdrawn", type=pathlib.Path, required=True)
    a = p.parse_args(argv[1:])
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
