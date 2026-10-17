#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""NOTICE has to match the tree under it, and its own second copy.

Two things, both because NOTICE is a register, and a register that no longer
touches its subject is worse than none: it gets believed.

1. THE TWO COPIES AGREE. `bindings/dotnet/src/PDFluent/NOTICE` travels inside the
   NuGet package. What a .NET user reads about our licences has to be what the
   repository says; two versions means one of them is untrue and nobody can see
   which.

2. EVERY PATH IT NAMES EXISTS. A line about `.claude/skills/caveman` left behind
   after that directory is gone -- or written before it arrives -- describes
   something the reader does not have. Same shape as a guard still watching a
   deleted file, or a ledger that never shrinks.

WHAT COUNTS AS A PATH
=====================
Only something whose first segment is a real entry in the root. Without that
rule `skills/caveman/` counts -- the UPSTREAM repository's layout, named to say
where something came from -- and this file would demand we rebuild someone
else's tree.
"""
from __future__ import annotations
import os
import pathlib
import re
import sys

REPO = pathlib.Path(os.environ.get("PDFLUENT_NOTICE_ROOT")
                    or pathlib.Path(__file__).resolve().parents[2])
ROOT_NOTICE = REPO / "NOTICE"
COPY_NOTICE = REPO / "bindings/dotnet/src/PDFluent/NOTICE"

# EVERY COPY, NOT ONLY THE .NET ONE (#541). Three packaging tools cannot reach a
# file outside their own directory -- npm packs what is inside the package, and
# maturin refuses `..` in an include pattern -- so the .NET, Node and Python
# packages carry committed copies of the two registers, and a copy that has
# drifted is a licence text that says something the tree does not. Written by
# scripts/ci/regenerate_registers.sh; held to the original here.
KOPIEEN = (
    ("bindings/dotnet/src/PDFluent/NOTICE", "NOTICE"),
    ("bindings/dotnet/src/PDFluent/THIRD_PARTY_LICENSES.txt", "THIRD_PARTY_LICENSES.txt"),
    ("crates/pdf-node/NOTICE", "NOTICE"),
    ("crates/pdf-node/THIRD_PARTY_LICENSES.txt", "THIRD_PARTY_LICENSES.txt"),
    ("crates/pdf-python/NOTICE", "NOTICE"),
    ("crates/pdf-python/THIRD_PARTY_LICENSES.txt", "THIRD_PARTY_LICENSES.txt"),
    # The RAG loader packages ship our two licence texts in their sdists (#630).
    ("integrations/langchain-pdfluent/LICENSE", "LICENSE"),
    ("integrations/langchain-pdfluent/LICENSE-COMMERCIAL", "LICENSE-COMMERCIAL"),
    ("integrations/llama-index-readers-pdfluent/LICENSE", "LICENSE"),
    ("integrations/llama-index-readers-pdfluent/LICENSE-COMMERCIAL", "LICENSE-COMMERCIAL"),
    # And the MCP server package (#625).
    ("integrations/pdfluent-mcp/LICENSE", "LICENSE"),
    ("integrations/pdfluent-mcp/LICENSE-COMMERCIAL", "LICENSE-COMMERCIAL"),
    # And the two extract packages (#636).
    ("integrations/pdfluent-extract/LICENSE", "LICENSE"),
    ("integrations/pdfluent-extract/LICENSE-COMMERCIAL", "LICENSE-COMMERCIAL"),
    ("integrations/pdfluent-extract-js/LICENSE", "LICENSE"),
    ("integrations/pdfluent-extract-js/LICENSE-COMMERCIAL", "LICENSE-COMMERCIAL"),
)

# A path-shaped word: segments joined by `/`, or a bare file with an extension.
PATH_LIKE = re.compile(r"(?<![\w/.-])((?:\.?[\w.-]+/)+[\w.-]+/?|[\w-]+\.[A-Za-z][\w.]*)")


def paths(text: str, roots: set[str]) -> list[str]:
    found = []
    for line in text.splitlines():
        # A URL is not a path in this tree.
        without_url = re.sub(r"https?://\S+", " ", line)
        for m in PATH_LIKE.finditer(without_url):
            cand = m.group(1).rstrip("/.,;:")
            first = cand.split("/")[0]
            if first in roots:
                found.append(cand)
    return found


def lijsten_komen_overeen() -> list[str]:
    """Complaints about KOPIEEN above disagreeing with the regenerator's list.

    Two lists of the same six pairs, one in this guard and one in
    `scripts/ci/regenerate_registers.sh`, and a copy that only one of them knows
    about is either written and never checked or checked and never written.
    Same shape, same reason, as `touched_crates.py`'s `excludes_agree()`.
    """
    # Beside THIS file, not under the tree being judged: the two lists are code,
    # and a fixture tree that stands in for the repository (PDFLUENT_NOTICE_ROOT)
    # has neither of them.
    script = pathlib.Path(__file__).resolve().parent / "regenerate_registers.sh"
    if not script.is_file():
        return [f"{script.name} is missing, so its list of copies could not be read"]
    # The KOPIEEN block of that script and nothing else: the file holds a second
    # array of `file|command|command` rows for the generated registers, and
    # reading those as copies would report four disagreements that are not.
    tekst = script.read_text(errors="replace")
    blok = re.search(r"^KOPIEEN=\((.*?)^\)", tekst, re.S | re.M)
    if not blok:
        return ["regenerate_registers.sh has no KOPIEEN list, so nothing writes "
                "the packaged copies"]
    hier = {f"{kopie}|{origineel}" for kopie, origineel in KOPIEEN}
    daar = set(re.findall(r'"([^"|]+\|[^"|]+)"', blok.group(1)))
    ontbreekt = sorted(hier - daar)
    teveel = sorted(daar - hier)
    klachten = []
    if ontbreekt:
        klachten.append("regenerate_registers.sh does not write: " + ", ".join(ontbreekt))
    if teveel:
        klachten.append("regenerate_registers.sh writes copies this guard does not "
                        "check: " + ", ".join(teveel))
    return klachten


def main() -> int:
    if not ROOT_NOTICE.exists() or not COPY_NOTICE.exists():
        print("[notice] SKIPPED (not a pass): one of the two NOTICE files is "
              f"missing ({ROOT_NOTICE.exists()=}, {COPY_NOTICE.exists()=}), so nothing was "
              "compared.", file=sys.stderr)
        return 1

    a = ROOT_NOTICE.read_bytes()

    verschil = lijsten_komen_overeen()
    if verschil:
        print("[notice] the two lists of packaged copies disagree:\n", file=sys.stderr)
        for regel in verschil:
            print(f"  {regel}", file=sys.stderr)
        print("\n  One list writes the copies and the other checks them; a copy only "
              "one of them\n  knows about is written and never checked, or checked and "
              "never written.", file=sys.stderr)
        return 1

    afgedreven = []
    for kopie, origineel in KOPIEEN:
        pad, bron = REPO / kopie, REPO / origineel
        if not bron.is_file():
            print(f"[notice] SKIPPED (not a pass): {origineel} is missing, so its "
                  "copies could not be compared.", file=sys.stderr)
            return 1
        if not pad.is_file() or pad.read_bytes() != bron.read_bytes():
            afgedreven.append((kopie, origineel))
    if afgedreven:
        print("[notice] a packaged copy has diverged from the file it copies:\n",
              file=sys.stderr)
        for kopie, origineel in afgedreven:
            staat = "missing" if not (REPO / kopie).is_file() else "differs"
            print(f"  {kopie}  ({staat}; copies {origineel})", file=sys.stderr)
        print("\n  Those copies travel inside the packages -- the NuGet package, the\n"
              "  npm one and the wheel -- because npm packs only what is in the\n"
              "  package directory and maturin refuses `..`. A reader has one of them\n"
              "  in front of them and not the original, so two versions means one is\n"
              "  untrue and the reader cannot see which.\n\n"
              "      bash scripts/ci/regenerate_registers.sh\n",
              file=sys.stderr)
        return 1

    roots = {p.name for p in REPO.iterdir()}
    candidates = paths(a.decode("utf-8", "replace"), roots)
    missing = sorted({p for p in candidates if not (REPO / p).exists()})
    if missing:
        print(f"[notice] {len(missing)} path(s) named in NOTICE do not exist here:\n",
              file=sys.stderr)
        for p in missing:
            print(f"  {p}", file=sys.stderr)
        print("\n  A register that no longer touches its subject gets believed.\n"
              "  Remove the line, or land it once the path is there.",
              file=sys.stderr)
        return 1

    # A floor, because zero recognised paths is not "everything checks out" but
    # "nothing was looked at". The number is small -- NOTICE mostly names crates,
    # and a crate name is not a path -- which is exactly why it must not be able
    # to reach zero unnoticed: a broken extractor looks identical to a NOTICE that
    # is correct.
    if not candidates:
        print("[notice] SKIPPED (not a pass): no path recognised in NOTICE at "
              "all. Either the file was gutted or the matching is broken; either "
              "way nothing was checked.", file=sys.stderr)
        return 1

    print(f"[notice] OK: {len(KOPIEEN)} packaged copies identical to what they "
          f"copy, {len(set(candidates))} named path(s) exist.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
