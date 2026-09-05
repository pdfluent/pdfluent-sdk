#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The corpus stays in-house, and so do the names of the documents in it.

THE DECISION (2026-08-27)

Twenty-five of the test documents came from somewhere else. Six carry an
explicit licence or fall under a statute that settles it. The rest do not, and
"published for the public to fill in" is not a licence to redistribute a file in
a commercially used source repository.

Regenerating eighteen documents as synthetic fixtures would have cost the XFA
golden suite fourteen of its fifteen real forms, and synthetic XFA does not
reproduce what real government forms do. The decision was to keep the corpus
internal instead: less transparency, no loss of test quality.

**The names travel too.** A file list is a description of the corpus even when
the corpus is absent, so the paths below and the filenames inside them do not
appear in the public tree either.

WHAT THIS CHECKS

Run with --tree <dir> it fails if a published tree contains one of the internal
paths, or mentions one of the internal document names in any text file.

Run without arguments it checks the same thing against the files this repository
tracks today, minus the internal paths -- which is what the public SDK tree will
be assembled from.

Exit codes:
    0  nothing internal is exposed
    1  an internal path or document name is in the published tree
"""

from __future__ import annotations

# FLOOR: internal paths >= 4 and alternatives in the names pattern >= 8 -- both
# are read from disk, and a guard checking an empty list reports success over
# nothing. Only the paths half was enforced until 30-08-2026; `names_in_pattern`
# sat in docs/PUBLIC_TREE.toml and nothing ever read it, so shortening the regex
# to `\b()\b` would have silenced the whole name check while the run stayed green.
#
# The declared value was 15 against a pattern that actually carries 11
# alternatives (measured 30-08-2026), so enforcing it as written would have
# failed on a correct tree. That is the other way a floor goes wrong, and it is
# why the number has to be measured rather than remembered. Set to 8.
import os
import re
import subprocess
import tomllib
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# Read from docs/PUBLIC_TREE.toml, not repeated here.
#
# One manifest, two readers: this guard and the LC10 exporter that will assemble
# the published tree. Two lists would drift, and the one that drifts is the one
# nobody runs.
MANIFEST = REPO / "docs" / "PUBLIC_TREE.toml"


def manifest() -> dict:
    if not MANIFEST.is_file():
        print(f"[internal] FATAL: {MANIFEST.name} is missing. Without it this guard has "
              f"no list, and a guard with no list reports success over nothing.",
              file=sys.stderr)
        sys.exit(1)
    return tomllib.loads(MANIFEST.read_text())


# What NOT to read, rather than what to read. An allowlist of text extensions
# has to be complete to be a guard, and it was not: `.xml` was missing while
# `crates/pdf-java/pom.xml` sits in this very branch, and `.svg`, `.csv` and
# `.tsv` were missing too -- a file listing naming the corpus is exactly a
# `.tsv`. Each miss was silent, because a skipped file cannot report anything.
# Inverting it means a new text format is covered on the day it appears rather
# than on the day somebody remembers it. (T1 review, #1650)
BINAIRE_SUFFIXEN = {".pdf", ".png", ".jpg", ".jpeg", ".gif", ".ico", ".webp",
                    ".woff", ".woff2", ".ttf", ".otf", ".eot",
                    ".zip", ".gz", ".xz", ".bz2", ".tar", ".jar", ".class",
                    ".so", ".dylib", ".dll", ".a", ".o", ".wasm", ".bin",
                    ".mp4", ".mov", ".mp3", ".wav", ".pyc", ".pack", ".idx"}

# Above this a file is not prose and reading it costs more than it can find.
MAX_BYTES = 8 * 1024 * 1024

# Byte-order marks, longest first: UTF-32's little-endian mark starts with
# UTF-16's, so testing UTF-16 first would decode a UTF-32 file as garbage.
BOMS = ((b"\x00\x00\xfe\xff", "utf-32-be"), (b"\xff\xfe\x00\x00", "utf-32-le"),
        (b"\xfe\xff", "utf-16-be"), (b"\xff\xfe", "utf-16-le"),
        (b"\xef\xbb\xbf", "utf-8-sig"))


def lees_tekst(pad) -> str | None:
    """The file's text, whatever it is encoded in, or None if it is not text.

    `read_text(errors="replace")` decodes as UTF-8 and nothing else, so a UTF-16
    file became a string with a replacement character between every letter and
    matched no pattern at all -- silently, because "replace" cannot raise. A
    name written in UTF-16 is plainly readable in any editor that honours the
    BOM, and this guard reported the tree clean. (T1 review, #1650)
    """
    try:
        data = pad.read_bytes()
    except OSError:
        return None
    if len(data) > MAX_BYTES:
        return None
    for bom, enc in BOMS:
        if data.startswith(bom):
            return data.decode(enc, errors="replace")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        # Not UTF-8 and no mark. Latin-1 cannot fail and leaves every ASCII byte
        # where it was, which is all an internal document name is made of.
        return data.decode("latin-1", errors="replace")



def _git_omgeving() -> dict:
    """git without the caller's GIT_* variables.

    Inside a git hook GIT_DIR and GIT_WORK_TREE point at the real repository,
    and git then ignores the directory you point it at. On 25-08-2026 a test set
    `core.bare = true` on the real repository that way and everything stopped.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def tracked() -> list[str]:
    """Tracked paths, NUL-separated.

    `git ls-files` without `-z` C-quotes any path holding a byte outside ASCII:
    a name ending in `-Dβγ.md` comes back as `"...-D\\316\\262\\316\\263.md"`,
    quotes and all. Two tracked files are in that state today.

    Which is not cosmetic here. The quoted form starts with `"`, so it does not
    match any prefix in [internal].paths -- an internal file with a non-ASCII
    name is simply not recognised as internal. It then survives into the list
    this guard scans, where `read_text` fails on the literal name, the OSError
    is swallowed, and the file is never read for internal document names. The
    count printed at the end includes it. A file that is counted and not read is
    the silent skip this repository keeps rediscovering, one layer down.

    `-z` turns that off: literal bytes, NUL between them.
    """
    out = subprocess.run(["git", "ls-files", "-z"], capture_output=True, text=True,
                         cwd=REPO, env=_git_omgeving())
    return [f for f in out.stdout.split("\0") if f]


def main() -> int:
    m = manifest()
    paden = tuple(m["internal"]["paths"])
    bestanden = tuple(m["internal"]["files"])
    patroon = re.compile(m["names"]["pattern"])
    vloer_paden = m["floors"]["paths"]
    vloer_namen = m["floors"]["names_in_pattern"]

    if len(paden) < vloer_paden:  # FLOOR
        print(f"[internal] FATAL: {len(paden)} internal paths declared, below the floor "
              f"of {vloer_paden}. Someone shortened the list, and a shorter list "
              f"publishes more.", file=sys.stderr)
        return 1

    # Alternatives in the names regex. A pattern that matches nothing finds
    # nothing, and finding nothing is what this guard prints on success.
    namen_in_patroon = m["names"]["pattern"].count("|") + 1
    if namen_in_patroon < vloer_namen:  # FLOOR
        print(f"[internal] FATAL: the names pattern carries {namen_in_patroon} "
              f"alternative(s), below the floor of {vloer_namen}. A shorter pattern "
              "matches fewer documents, and matching nothing is indistinguishable "
              "from a clean tree.", file=sys.stderr)
        return 1

    def intern(f: str) -> bool:
        return f.startswith(paden) or f in bestanden

    args = sys.argv[1:]
    if args and args[0] == "--tree":
        if len(args) < 2:
            print("[internal] --tree needs a directory", file=sys.stderr)
            return 1
        root = Path(args[1])
        lijst = [str(p.relative_to(root)) for p in root.rglob("*") if p.is_file()]
        lees = lambda f: root / f  # noqa: E731
        wat = f"published tree {root}"
    else:
        lijst = [f for f in tracked() if not intern(f)]
        lees = lambda f: REPO / f  # noqa: E731
        wat = "the tree that would be published from this repository"

    blootgesteld = [f for f in lijst if intern(f)]
    namen: list[tuple[str, str]] = []
    for f in lijst:
        if Path(f).suffix.lower() in BINAIRE_SUFFIXEN:
            continue
        tekst = lees_tekst(lees(f))
        if tekst is None:
            continue
        hit = patroon.search(tekst)
        if hit:
            namen.append((f, hit.group(1)))

    print(f"[internal] checked {len(lijst)} file(s) in {wat}")

    if not blootgesteld and not namen:
        print("[internal] no internal path and no internal document name is exposed")
        return 0

    if blootgesteld:
        print(f"[internal] {len(blootgesteld)} internal path(s) present:")
        for f in blootgesteld[:20]:
            print(f"  {f}")
    if namen:
        print(f"[internal] {len(namen)} file(s) name an internal document:")
        for f, n in namen[:20]:
            print(f"  {f}  ->  {n}")
    print()
    print("[internal] These documents came from elsewhere and nothing establishes a right")
    print("[internal] to redistribute them. A file list describes the corpus even when the")
    print("[internal] corpus is absent, so the names stay in as well. Add the file to")
    print("[internal] `files` in docs/PUBLIC_TREE.toml, or take the name out of it.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
