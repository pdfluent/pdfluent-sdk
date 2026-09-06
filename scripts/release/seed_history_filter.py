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
    workspace  no version of the root `Cargo.toml` names a member whose directory
               the manifest keeps in-house -- a published workspace that names a
               crate it does not carry does not parse, let alone build

The terms scan is the reason this reads blobs and not only names: a customer name
inside a published file is exactly as public as one in a published file NAME, and
the tree exporter already learned that lesson (`geen_interne_zaken --boom` reads
content because reading names alone let `docs/ZZQBETA-notes.md` through).

A TERM IS NOT A PATH, AND NEITHER IS A MESSAGE
=============================================
The path filter cannot reach the last class. Measured on `github/master`
05-09-2026: 63 internal terms in file content and 24 in commit messages SURVIVED
it, because the files carrying them are files that have to go out -- an old
`pom.xml` naming the private group URL, a design note naming a partner. There is
no path to exclude; the string itself has to go, over the range that carries it.

So there is a second list, and it is a list of REPLACEMENTS rather than of
paths: `literal==>replacement`, one per line, read from outside the tree for the
same reason the term list is (`geen_interne_zaken.py`: a denylist that ships its
own terms publishes exactly what it forbids). Default
`~/.config/pdfluent/seed-replacements.txt`, overridable with
`PDFLUENT_SEED_VERVANGINGEN`.

Two invariants make the list a repair rather than a way to silence the check,
and both are enforced here rather than trusted:

    a literal must itself be an internal term   -- so the list cannot be used to
                                                   rewrite arbitrary published
                                                   content on the way out
    a replacement must contain no internal term -- so a substitution cannot smuggle
                                                   one term in under another

WITHOUT THE LIST NOTHING CHANGES. No file is read differently, no message is
touched, and the seeding refuses on the terms exactly as it did before. An
absent list is the old behaviour, not a silent pass -- the refusal is what asks
for eyes, and the list is what a person writes after using them.

A WORKSPACE MEMBER THAT IS NOT THERE IS NOT A BUILDABLE REPOSITORY
==================================================================
`crates/xfa-golden-tests` is on `[internal].paths`, and it is also a line in the
root `Cargo.toml` `members` list. Drop the directory and leave the line and the
published repository does not build -- it does not even parse:

    error: failed to load manifest for workspace member `crates/xfa-golden-tests`
    referenced by workspace at `Cargo.toml`

`simulate_public_tree.assemble` has always known this and edits the members list
on the way out. The seeding did not, because its central promise is that a file
that stays is byte for byte what it was -- so it published the manifest unchanged
and #222's own acceptance criterion ("somebody who clones the repository can
build the SDK") failed on the first command a reader would type. Measured
06-09-2026 on the seeded mirror: that one line was the ONLY thing between the
published tree and a clean-clone build; with it removed the workspace builds and
the smoke tests pass in an empty environment.

The clean-clone job did not catch it because it builds the tree
`simulate_public_tree` assembles -- the tree WITH the edit -- and the seeding
publishes the tree without it. Two mechanisms, one question, different answers:
the shape this file exists to remove.

So the edit is made here too, from the same manifest key
(`[internal].workspace_members`) and not from a second list, over every version
of the root manifest in the history. A blob is only touched when EVERY path it
ever had is the root `Cargo.toml`, the same test the fork exemption makes, and a
version that predates the crate is left alone. `verify` then asserts the property
rather than the edit: no root manifest the published history reaches may name a
member the manifest calls internal.

Subcommands:
    plan <mirror> --paths <f> --withdrawn <f>
                        the paths that must not travel (NUL-separated) and the
                        object ids that must not travel, from one walk
    preflight <mirror>  everything the rewrite needs, asked before the rewrite
    replacements <mirror> --map <f> --sed <f>
                        for every surviving blob that carries an internal term,
                        the rewritten blob (written into the mirror) as an
                        `<old> <new>` map, plus the message redaction as a sed
                        script. Refuses when a term survives its own replacement.
    trees <mirror> [--map <f>]
                        ref, blob id and path for every file that MAY travel,
                        with --map naming the blob the rewrite is EXPECTED to
                        leave there
    alias <mirror>      the noreply alias non-alias identities are rewritten to
    allowed <mirror>    every address in this history that may stay as it is
    verify <mirror> --withdrawn <file>
                        the five checks above over the REWRITTEN mirror

Exit codes:
    0  the mirror carries only what the manifest allows
    1  it does not, or a check could not be made and therefore did not pass
"""
from __future__ import annotations

import argparse
import functools
import importlib.util
import os
import pathlib
import re
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


@functools.lru_cache(maxsize=None)
def _module(naam: str):
    """Import a guard by path, so there is one copy of each rule and not two.

    Cached: the borrowed helpers are called per blob and per member, and
    re-executing a module on every call turned a table lookup back into the
    hours this file exists to avoid.
    """
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
    # A malformed replacement list must be reported now, not after a rewrite
    # over thousands of commits -- the same reason the term list is read here.
    paren = vervangingen()
    if paren:
        print(f"[seed-preflight] OK: {len(paren)} reviewed replacement(s), each one a "
              "term a rule calls internal")
    else:
        print("[seed-preflight] no replacement list; the seeding will REFUSE on any "
              "internal term it finds rather than redact it")
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
    # With --map, the blob printed for a path is the one the rewrite is EXPECTED
    # to leave there. Without it the check would read a redaction as "the rewrite
    # changed a file that was supposed to stay" -- true, and the wrong verdict:
    # the content rewrite changes surviving files on purpose, and what has to be
    # proved is that it changes exactly those and no others.
    kaart: dict[str, str] = {}
    if getattr(a, "map", None) and a.map.is_file():
        for regel in a.map.read_text(encoding="utf-8").split("\n"):
            deel = regel.split()
            if len(deel) == 2:
                kaart[deel[0]] = deel[1]
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
                print(f"{ref}\t{kaart.get(oid, oid)}\t{pad}")
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


# --------------------------------------------------------------- replacements --
#
# The list lives outside the tree, next to the term list and for its reason: half
# of what it has to name ARE the terms, and a file in the published tree that
# spells them publishes exactly what the seeding removes.
VERVANGINGEN_PAD = os.environ.get(
    "PDFLUENT_SEED_VERVANGINGEN",
    os.path.expanduser("~/.config/pdfluent/seed-replacements.txt"),
)

SCHEIDING = "==>"


def vervangingen() -> list[tuple[str, str, bool]]:
    """The reviewed replacement list, checked against the rules it serves.

    Each entry is (literal, replacement, case-insensitive), and the third field
    is READ OFF THE RULE that recognises the literal rather than chosen here. The
    partner rule is `re.I` because a name is spelled however the writer felt like
    it; the commercial rule is deliberately case-SENSITIVE because `arr` is an
    ordinary variable name and `ARR` is a revenue term. A blanket
    case-insensitive replacement would have rewritten every `arr` in the history.

    Returns [] when the file is absent, and that is deliberately not an error:
    an absent list means the seeding behaves exactly as it did before this
    existed -- it refuses on the terms. A list is what somebody writes after
    looking at what the refusal reported; it is not a prerequisite for looking.
    """
    try:
        with open(VERVANGINGEN_PAD, encoding="utf-8") as f:
            regels = [r.rstrip("\n") for r in f]
    except OSError:
        return []

    gi = _module("geen_interne_zaken")
    rules = gi.alle_regels()
    paren: list[tuple[str, str]] = []
    for n, regel in enumerate(regels, 1):
        if not regel.strip() or regel.lstrip().startswith("#"):
            continue
        if SCHEIDING not in regel:
            raise SystemExit(
                f"[seed-replace] {VERVANGINGEN_PAD}:{n} carries no `{SCHEIDING}`. "
                "Each line is `literal==>replacement`; a line that is neither that "
                "nor a comment is a line whose intention cannot be read, and "
                "guessing it would rewrite published content.")
        links, _, rechts = regel.partition(SCHEIDING)
        if not links:
            raise SystemExit(f"[seed-replace] {VERVANGINGEN_PAD}:{n} replaces the empty "
                             "string, which matches everywhere.")
        # A literal that is not itself an internal term would make this list a
        # general rewriting facility over published content. It is not one: what
        # it may change is exactly what the guards refuse to publish.
        raakt = [(naam, rx) for naam, rx in rules if rx.search(links)]
        if not raakt:
            raise SystemExit(
                f"[seed-replace] {VERVANGINGEN_PAD}:{n} names something no rule in "
                "geen_interne_zaken calls internal. This list may only replace terms "
                "that would otherwise refuse the seeding; anything else is an edit to "
                "published history under the name of a redaction.")
        # And the other direction: a replacement that carries a term would move
        # the problem rather than fix it, and the run afterwards would still be red.
        for naam, rx in rules:
            if rx.search(rechts):
                raise SystemExit(
                    f"[seed-replace] {VERVANGINGEN_PAD}:{n} replaces one internal term "
                    f"with something the [{naam}] rule also calls internal.")
        negeer = all(rx.flags & re.IGNORECASE for _, rx in raakt)
        paren.append((links, rechts, negeer))
    # Longest first: a term that contains a shorter one must be replaced as the
    # long form, or the short replacement leaves the tail of the long one behind.
    paren.sort(key=lambda pr: len(pr[0]), reverse=True)
    return paren


def toepassen(tekst: str, paren: list[tuple[str, str]]) -> str:
    """Case-insensitively, because the rules it serves are.

    `str.replace` was the first version and it left two blobs behind on the real
    history: an operator runbook and a publish plan that spell a partner name
    with different capitalisation from the term list. The partner rule is
    `re.I` on purpose -- "the terms are names and a commit message spells them
    however it feels like" -- so a case-sensitive replacement is a list that is
    incomplete by construction, and the incompleteness shows up as a refusal on
    a file nobody would think to look at.
    """
    for links, rechts, negeer in paren:
        tekst = re.sub(re.escape(links), rechts.replace("\\", "\\\\"), tekst,
                       flags=re.IGNORECASE if negeer else 0)
    return tekst


def _sed_script(paren: list[tuple[str, str]]) -> str:
    """The same replacements as a sed script, for the message filter.

    A python process per commit is 4072 of them on this history, and the message
    filter already runs inside a shell that forks per commit. sed is the cheap
    half; the escaping is the part worth getting right, so it is generated here
    rather than written by hand: `|` as the delimiter with every literal `|`,
    `\\` and regex metacharacter escaped, and `&` escaped on the right where it
    would otherwise mean "the whole match".
    """
    uit = []
    for links, rechts, negeer in paren:
        # Case-insensitively, matching `toepassen`, and spelled as character
        # classes rather than with sed's `I` flag: that flag is a GNU and a
        # BSD extension with different spellings, and this script runs on both
        # a developer's macOS and the Linux runner. `[Aa]` is portable to
        # every sed there is.
        l = ""
        for teken in links:
            if teken.isalpha() and negeer:
                l += f"[{teken.upper()}{teken.lower()}]"
            elif teken in ".^$*+?()[]{}|\\/":
                l += "\\" + teken
            else:
                l += teken
        r = rechts.replace("\\", "\\\\").replace("&", "\\&").replace("|", "\\|")
        uit.append(f"s|{l}|{r}|g")
    return "\n".join(uit) + ("\n" if uit else "")


def cmd_replacements(a) -> int:
    """Rewrite every surviving blob that carries a term, and report what is left.

    The new blobs are written into the mirror here, before filter-branch starts,
    so the rewrite itself only has to swap an object id in the index -- which is
    a table lookup rather than a checkout, and is the difference between minutes
    and hours over a history this size.

    Nothing is printed that a term could be read out of: the count travels, the
    string does not, exactly as in `geen_interne_zaken._toonbaar`.
    """
    paren = vervangingen()
    gi = _module("geen_interne_zaken")
    rules = gi.alle_regels()

    mag, _ = gesplitst(a.mirror)
    paden_van_oid: dict[str, set[str]] = {}
    for pad, oids in mag.items():
        for oid in oids:
            paden_van_oid.setdefault(oid, set()).add(pad)
    geforkt = _fork_uitzondering(paden_van_oid)

    _, m = manifest()
    leden = list(m["internal"]["workspace_members"])

    regels_uit: list[str] = []
    onopgelost: list[str] = []
    gelezen = manifesten = 0
    for oid, tekst in _tekstblobs(a.mirror, sorted(paden_van_oid)):
        gelezen += 1
        paden_hier = paden_van_oid.get(oid) or set()
        nieuw = tekst

        # 1. The reviewed replacements.
        if not (paden_hier and paden_hier <= gi.EIGEN_BESTANDEN):
            fork = geforkt(oid)
            van_toepassing = [(naam, rx) for naam, rx in rules
                              if not (fork and naam == "commercieel")]
            if any(rx.search(tekst) for _, rx in van_toepassing):
                nieuw = toepassen(tekst, paren)
                rest = [naam for naam, rx in van_toepassing if rx.search(nieuw)]
                if rest:
                    waar, _ = gi._toonbaar(rest[0], sorted(paden_hier)[0], "")
                    onopgelost.append(f"[{rest[0]}] -- {waar}")
                    continue

        # 2. The workspace members whose directories do not travel. Applied to
        #    the REPLACED text and not to the original: a manifest can need both,
        #    and two independent rewrites of one blob would keep whichever was
        #    written last.
        if paden_hier and paden_hier <= WORTELMANIFEST:
            zonder = zonder_interne_leden(nieuw, leden)
            if zonder != nieuw:
                manifesten += 1
                nieuw = zonder

        if nieuw == tekst:
            continue
        r = subprocess.run(["git", "-C", str(a.mirror), "hash-object", "-w",
                            "--stdin"], input=nieuw.encode("utf-8"),
                           capture_output=True, check=True)
        regels_uit.append(f"{oid} {r.stdout.decode().strip()}")

    a.map.write_text("".join(r + "\n" for r in regels_uit), encoding="utf-8")
    a.sed.write_text(_sed_script(paren), encoding="utf-8")
    a.sed.chmod(0o600)

    print(f"[seed-replace] {len(paren)} reviewed replacement(s), {gelezen} readable "
          f"blob(s), {len(regels_uit)} rewritten, {manifesten} of them a root "
          f"manifest naming an internal workspace member")
    if onopgelost:
        print(f"[seed-replace] {len(onopgelost)} blob(s) still carry an internal term "
              "after the list was applied:", file=sys.stderr)
        for r in onopgelost[:20]:
            print(f"  {r}", file=sys.stderr)
        print("  Add the term to the replacement list, or take the string out of the "
              "file. The seeding does not guess at a redaction.", file=sys.stderr)
        return 1
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


# The root manifest, by the one name it has. A blob is only edited when EVERY
# path it ever had is this one -- a blob that also lived somewhere else is not
# only a workspace manifest, and editing it there would be editing a file nobody
# asked about.
WORTELMANIFEST = {"Cargo.toml"}


def _ledenregel(lid: str) -> re.Pattern:
    """The members line for one crate, from the tree exporter and not restated.

    The whole reason this edit is here at all is that the exporter made it and
    the seeding did not; a second copy of the expression would be the same defect
    one function later. `simulate_public_tree` owns it, this borrows it, and a
    change to either is a change to both.
    """
    stp, _ = manifest()
    return stp.ledenregel(lid)


def zonder_interne_leden(tekst: str, leden: list[str]) -> str:
    """The root manifest with every internal workspace member taken out.

    No refusal when a member is absent, unlike `simulate_public_tree.assemble`:
    this runs over the WHOLE history, and a manifest from before the crate
    existed does not name it. What must hold is the property, and `verify` is
    where that is asserted.
    """
    stp, _ = manifest()
    return stp.zonder_interne_leden(tekst, leden)


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

    # 5. Buildability, which is the one property here a reader meets first. Every
    #    version of the root manifest the published history reaches, checked for
    #    a member whose directory the manifest keeps in-house. Asserted rather
    #    than assumed: the edit that removes them is in another function, and a
    #    check that reads the edit instead of the result proves nothing.
    leden = list(m["internal"]["workspace_members"])
    manifest_oids = sorted(paden.get("Cargo.toml") or set())
    kapot: list[str] = []
    gelezen_manifesten = 0
    for oid, tekst in _tekstblobs(mirror, manifest_oids):
        gelezen_manifesten += 1
        namen = [lid for lid in leden if _ledenregel(lid).search(tekst)]
        if namen:
            kapot.append(namen[0])
    # A history with no root manifest is not a workspace and has nothing to
    # break -- a repository that is not a cargo workspace is a legitimate source.
    # A history that HAS one and could not read a single version of it is the
    # other thing, and reporting green over that is what these checks refuse
    # everywhere else. (`trees` is what catches a rewrite that dropped the file:
    # a publishable path that stopped holding its blob fails there, not here.)
    if manifest_oids and not gelezen_manifesten:
        fouten.append(f"the published history reaches {len(manifest_oids)} root "
                      "Cargo.toml object(s) and not one could be read, so the "
                      "workspace was never checked; that is not a pass")
    if kapot:
        fouten.append(f"{len(kapot)} version(s) of the root manifest name a "
                      f"workspace member whose directory does not travel, e.g. "
                      f"{kapot[0]} -- a clone of this would not build")

    print(f"[seed-verify] {len(paden)} path(s), {gelezen} readable text blob(s), "
          f"{len(set(adressen))} distinct identity/identities, "
          f"{len(manifest_oids)} version(s) of the root manifest")

    if fouten:
        print("[seed-verify] the rewritten history is not publishable:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[seed-verify] OK: no internal path, no withdrawn object, no personal "
          "address, no internal term, no workspace member that does not travel")
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
                     ("allowed", cmd_allowed), ("verify", cmd_verify),
                     ("replacements", cmd_replacements)):
        s = sub.add_parser(naam)
        s.add_argument("mirror", type=pathlib.Path)
        s.set_defaults(fn=fn)
        if naam == "verify":
            s.add_argument("--withdrawn", type=pathlib.Path, default=None)
        if naam == "trees":
            s.add_argument("--map", type=pathlib.Path, default=None)
        if naam == "replacements":
            s.add_argument("--map", type=pathlib.Path, required=True)
            s.add_argument("--sed", type=pathlib.Path, required=True)
        if naam == "plan":
            s.add_argument("--paths", type=pathlib.Path, required=True)
            s.add_argument("--withdrawn", type=pathlib.Path, required=True)
    a = p.parse_args(argv[1:])
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
