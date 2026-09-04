#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Every tracked document is registered, and every registration names a file (#215).

`corpus_herkomst.py` demands a provenance line for each tracked document and
generates docs/CORPUS_HERKOMST.md from it. It asks `git ls-files "*.pdf"`.

THAT GLOB IS THE HOLE. A document is not only a PDF. On 30-08-2026 two Word
files sat in the repository root -- an internal briefing marked "Intern
document" and the `~$` lock file Word leaves behind -- and both landed in the
tree that docs/PUBLIC_TREE.toml would publish. Neither is a PDF, so the
provenance register never saw them; neither is under an `[internal]` path, so
nothing else did either. A register that covers one file extension reads
exactly like a register that covers everything.

THE SECOND DIRECTION
A register that only grows has stopped measuring. `corpus_herkomst.DERDEN`
still carried `crates/pdf-java/src/test/resources/sample.pdf` months after that
file was deleted -- a provenance line for nothing, indistinguishable from a
provenance line for something. Both directions fail here: an unregistered
document, and a registration whose file is gone.

WHAT COUNTS AS REGISTERED
  PDF          `corpus_herkomst.herkomst()` returns a provenance line.
  other        it is kept out of the published tree by docs/PUBLIC_TREE.toml,
               or listed under `[documents]` there with a reason. There is no
               provenance register for non-PDF documents, and inventing one for
               the two files that exist would be a table nobody maintains.

Exit codes:
    0  every document is registered and every registration names a file
    1  it is not, or the walk found fewer documents than the floor
    3  cannot check: docs/PUBLIC_TREE.toml is missing, unreadable or not the
       manifest (announced as SKIPPED, never silent)
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))

import corpus_herkomst  # noqa: E402

PUBLIC_TREE = REPO / "docs" / "PUBLIC_TREE.toml"

# FLOOR: tracked document files >= 150 -- measured 164 on 30-08-2026 (162 PDF,
# 2 Word). A walk that returns a handful means the git call broke, and a guard
# that inspects nothing reports the same green as one that inspected the lot.
MIN_DOCUMENTEN = 150

# What counts as a document: something a person authored and that can therefore
# carry someone else's copyright or someone's personal data. Source files and
# images are out -- images are covered by their own crate's provenance and are
# render output, not authored documents.
DOCUMENT_SUFFIXEN = (
    ".pdf", ".doc", ".docx", ".dot", ".dotx",
    ".xls", ".xlsx", ".ppt", ".pptx",
    ".odt", ".ods", ".odp", ".rtf",
    ".xdp", ".eml", ".msg", ".pages", ".key", ".numbers",
)


def _git_omgeving() -> dict[str, str]:
    """git without the caller's GIT_* variables.

    Inside a hook GIT_DIR and GIT_WORK_TREE are absolute and point at the real
    repository; a subprocess that inherits them ignores the directory it was
    pointed at.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def getrackt() -> list[str]:
    uit = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z"],
        capture_output=True, text=True, check=True, env=_git_omgeving(),
    )
    # -z: git C-quotes any path with a non-ASCII byte, and the quoted form is
    # not a path -- the extension test misses it and the file is never opened.
    # A document nobody could open counted as a document nobody had to check.
    return [r for r in uit.stdout.split("\0") if r]


# The three directories where test material lives. Inside them a suffix list is
# not enough: #215 asks the gate to fail on any new binary test file without a
# registration, and a document does not become safe by being called `.bin`.
#
# Measured when this was added: those directories held 163 PDFs and zero other
# binary files, so nothing existing had to be registered to turn this on. The
# rule guards the next one rather than papering over the last one.
TESTMATERIAAL = ("fixtures/", "test-data/", "corpus/")


def _is_binair(pad: str) -> bool:
    """A NUL byte in the first 4 KB -- the same test git uses to call a file binary.

    Not a suffix list: the point is the extension nobody thought of.
    """
    try:
        with open(REPO / pad, "rb") as fh:
            return b"\0" in fh.read(4096)
    except OSError:
        return False


def is_document(pad: str) -> bool:
    naam = pad.rsplit("/", 1)[-1]
    # Word's owner file: two bytes of metadata and the name of whoever had the
    # document open. It is never a fixture and never wanted.
    if naam.startswith("~$"):
        return True
    if pad.lower().endswith(DOCUMENT_SUFFIXEN):
        return True
    # Anything binary under the test-material directories, whatever it is called.
    return pad.startswith(TESTMATERIAAL) and _is_binair(pad)


def haalt_de_publieke_boom(pad: str, m: dict) -> bool:
    """The same rule scripts/ci/simulate_public_tree.py assembles by."""
    paden = tuple(m["internal"]["paths"])
    return not (pad.startswith(paden) or pad in set(m["internal"]["files"]))


def lees_manifest() -> dict | None:
    """The manifest, or None after saying why there is none to read.

    Missing, unreadable and malformed get the same verdict: without the
    manifest this guard cannot tell what gets published, so it has checked
    nothing. That is a skip and it says so. A traceback would exit non-zero
    too, but it reads as a crash, and a crash gets retried rather than read.
    """
    try:
        tekst = PUBLIC_TREE.read_text(encoding="utf-8")
    except OSError as e:
        reden = f"cannot be read ({e.strerror or e})"
    else:
        try:
            manifest = tomllib.loads(tekst)
        except tomllib.TOMLDecodeError as e:
            reden = f"is not valid TOML ({e})"
        else:
            intern = manifest.get("internal")
            if isinstance(intern, dict) and isinstance(intern.get("paths"), list) \
                    and isinstance(intern.get("files"), list):
                return manifest
            reden = "has no [internal] table with `paths` and `files`, so it is not the manifest"
    print(f"SKIPPED (not a pass): docs/PUBLIC_TREE.toml {reden}; without it this "
          "guard cannot tell what gets published and has checked nothing.",
          file=sys.stderr)
    return None


def main() -> int:
    manifest = lees_manifest()
    if manifest is None:
        return 3
    toegestaan: dict[str, str] = dict(manifest.get("documents", {}))

    alles = getrackt()
    aanwezig = set(alles)
    documenten = sorted(p for p in alles if is_document(p))

    if len(documenten) < MIN_DOCUMENTEN:
        print(f"document-register: {len(documenten)} document(s) found, fewer than "
              f"{MIN_DOCUMENTEN}. The git call is broken, not the repository empty.",
              file=sys.stderr)
        return 1

    ongeregistreerd: list[str] = []
    for pad in documenten:
        if pad.lower().endswith(".pdf"):
            if corpus_herkomst.herkomst(pad) is None:
                ongeregistreerd.append(f"{pad}: no provenance in corpus_herkomst.py")
            continue
        if pad in toegestaan:
            continue
        if haalt_de_publieke_boom(pad, manifest):
            ongeregistreerd.append(
                f"{pad}: not a PDF, so no provenance register covers it, and "
                f"docs/PUBLIC_TREE.toml lets it reach the published tree")

    # The other direction. A registration that names no file is a line nobody
    # can check, and it reads exactly like one that can be.
    verweesd: list[str] = []
    for pad in sorted(corpus_herkomst.DERDEN) + sorted(corpus_herkomst.EIGEN_LOS):
        if pad not in aanwezig:
            verweesd.append(f"corpus_herkomst.py registers {pad}, which is not tracked")
    for pad in sorted(toegestaan):
        if pad not in aanwezig:
            verweesd.append(f"PUBLIC_TREE.toml [documents] lists {pad}, which is not tracked")
    for pad in sorted(manifest["internal"]["files"]):
        if pad not in aanwezig:
            verweesd.append(f"PUBLIC_TREE.toml [internal].files lists {pad}, which is not tracked")

    if ongeregistreerd or verweesd:
        if ongeregistreerd:
            print(f"document-register: {len(ongeregistreerd)} document(s) with no "
                  f"registration.", file=sys.stderr)
            for r in ongeregistreerd:
                print(f"  {r}", file=sys.stderr)
        if verweesd:
            print(f"document-register: {len(verweesd)} registration(s) naming a file "
                  f"that is not there.", file=sys.stderr)
            for r in verweesd:
                print(f"  {r}", file=sys.stderr)
        print("\nA PDF needs a line in corpus_herkomst.py. Any other document needs\n"
              "an [internal] entry in docs/PUBLIC_TREE.toml so it stays in-house, or\n"
              "a [documents] entry there saying why it may ship.", file=sys.stderr)
        return 1

    publiek = [p for p in documenten if haalt_de_publieke_boom(p, manifest)]
    print(f"OK: {len(documenten)} tracked documents, {len(publiek)} of them in the "
          f"published tree, all registered; {len(toegestaan)} non-PDF exception(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
