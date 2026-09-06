#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""One crate, one licence, four registers -- and something that compares them (#304).

Four files describe the licence of every crate in this workspace:

  crates/<dir>/Cargo.toml                 what crates.io and every reader sees
  docs/release/canonical_licenses.toml    what the release gate insists on
  docs/licensing/boundary.toml            which side of the licence line it is on
  NOTICE                                  what the person holding a copy reads

Each was guarded against ONE neighbour. license_registry_check.py reads the
canonical register against the manifests; license_boundary.py reads the boundary
against the manifests and, since #295, part of the canonical register against the
boundary. Nothing put all three side by side, so `pdf-substitute-fonts` could say
`license_kind = "commercial"` in one file, `AGPL-3.0-or-later` in another and
`ours` in the third with every gate green, because no gate read more than two.

WHICH REGISTER IS THE TRUTH

  For the licence EXPRESSION of a published crate: canonical_licenses.toml.
  It is the file the release gate reads before `cargo publish`, its rows carry
  `forbidden_change = true`, and PUBLISH_PROTOCOL.md requires a separate,
  reasoned commit to change one. A manifest that disagrees with it is wrong;
  a register row that is wrong is an owner decision, taken in its own commit.

  For WHICH SIDE a crate is on, and whether it is published at all:
  boundary.toml. It is the only register that covers every crate, including
  the sixteen that never leave the workspace -- the canonical register has no
  row for those on purpose, and this guard is what makes "no row" mean
  "not published" rather than "forgotten".

  Cargo.toml is derived from both. It is what the world sees, so it is the
  file most likely to be edited in passing, and the least entitled to differ.

THE TRANSLATION TABLE, made executable

  boundary `side`   canonical `license_kind`     Cargo.toml `[package]`
  ours              agpl-or-commercial           license = <the #257 expression>
  forked            open-source-derivative       license = <the upstream licence
                                                 the boundary records as `declares`>
  internal          (no row: never published)    license.workspace = true,
                                                 publish = false

Where docs/LICENSE_POLICY.toml `[own_packages]` names a Cargo.toml, that entry
is one more statement about the same crate and is compared too.

NOTICE, THE REGISTER NOBODY WAS READING (#345)

NOTICE keeps two lists -- an open-source foundation and a dual-licensed
(AGPL-or-commercial) one -- and it is the register a reader actually receives:
it ships in the crate tarballs, in the NuGet package and, once the public seed
lands, on the public tree. It was guarded for its paths and for its second copy
(notice_names_what_exists.py) but never against the licences it states, which
is how it could book `pdf-render` and `pdf-font` as dual-licensed
AGPL-or-commercial components while all three other registers had them as
`forked`, `Apache-2.0 OR MIT`, relicensing prohibited. Three registers agreeing
and the fourth contradicting them is the exact failure this guard exists to
end, so it reads NOTICE too:

  boundary `side`   NOTICE list                 NOTICE licence column
  ours              dual-licensed components    (none: the list states no
                                                 expression)
  forked            open-source foundation      the upstream licence the
                                                 boundary records as `declares`
  internal, or      named in neither list       --
  publish = false

Every published crate is in exactly one of the two lists, and nothing that is
never distributed is in either: a reader is handed NOTICE and no other file, so
a crate missing from it carries no statement at all, and a crate listed that
they will never receive describes something they do not have.

That the two NOTICE copies are byte-identical is checked by
notice_names_what_exists.py and is not repeated here.

EVERY MESSAGE NAMES THE TWO SOURCES THAT DISAGREE. "Something is wrong with
pdf-annot" sends the reader back to comparing four files by hand, which is the
job this guard exists to take over.

Exit codes:
  0  every crate says the same thing in every register that mentions it
  1  a disagreement, or a crate that one register has and another lacks
  2  the tree is too small to judge (a floor tripped)
  3  a register could not be read (announced, never silent)
"""
from __future__ import annotations

import pathlib
import re
import sys
import tomllib
from dataclasses import dataclass

REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFESTEN = REPO / "crates"
WERKRUIMTE = REPO / "Cargo.toml"
CANONIEK = REPO / "docs" / "release" / "canonical_licenses.toml"
GRENS = REPO / "docs" / "licensing" / "boundary.toml"
BELEID = REPO / "docs" / "LICENSE_POLICY.toml"
KENNISGEVING = REPO / "NOTICE"

# Short names for the messages. A path is precise; a name is what a reader
# recognises at a glance in a list of twelve lines.
R_MANIFEST = "Cargo.toml"
R_CANONIEK = "canonical_licenses.toml"
R_GRENS = "boundary.toml"
R_BELEID = "LICENSE_POLICY.toml [own_packages]"
R_KENNISGEVING = "NOTICE"

# The one expression our own crates carry on every channel since #257.
ONZE_LICENTIE = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

# side -> license_kind. `internal` is absent on purpose: an internal crate has no
# canonical row, and the guard checks that absence rather than a value.
SOORT_VAN_ZIJDE = {"ours": "agpl-or-commercial", "forked": "open-source-derivative"}

# The two lists NOTICE keeps, named the way its own headings name them. The
# heading text is matched as a substring, so the parenthetical after it can be
# reworded without silently switching the parser off.
N_OPEN = "open-source foundation"
N_DUAAL = "dual-licensed"
NOTICE_KOPPEN = {"OPEN-SOURCE FOUNDATION": N_OPEN,
                 "DUAL-LICENSED COMPONENTS": N_DUAAL}

# side -> the NOTICE list a published crate belongs in. `internal` is absent for
# the same reason as above: it belongs in neither, and that absence is checked.
LIJST_VAN_ZIJDE = {"ours": N_DUAAL, "forked": N_OPEN}

# An entry line: the crate name in the left column, then its licence (in the
# open-source list) or its one-line description (in the dual-licensed one).
# Prose starts at column 1 and continuation lines are indented past the name
# column, so neither can be read as a crate row. One space is enough between the
# columns: `formcalc-interpreter` fills the name column exactly.
NOTICE_REGEL = re.compile(r"^ {2}([A-Za-z][\w-]*) +(\S.*?)\s*$")
NOTICE_STREEP = re.compile(r"^[\u2014-]\s*")

# FLOOR: NOTICE names every published crate, of which there were 32 on
# 06-09-2026. A parser that quietly stops matching reads as a register that
# agrees with everything, which is the failure mode this guard is about.
VLOER_NOTICE = 25

# FLOOR: fewer crates than this and the tree being read is not this workspace.
# Measured 02-09-2026 at 48. An empty crates/ would otherwise agree with an
# empty register and report a clean run over nothing.
VLOER_CRATES = 40


@dataclass
class Manifest:
    naam: str | None
    decl: str | None      # "license" | "license-file" | "workspace" | None
    waarde: str | None
    publiceerbaar: bool


def _pakketsectie(tekst: str) -> str:
    """Only `[package]`. `license = "MIT"` also appears in `[workspace.package]`
    at the root, and a comment quoting a licence must not read as one."""
    uit: list[str] = []
    binnen = False
    for regel in tekst.splitlines():
        s = regel.strip()
        if s.startswith("[") and s.endswith("]"):
            binnen = s == "[package]"
            continue
        if binnen:
            uit.append(regel)
    return "\n".join(uit)


def lees_manifest(pad: pathlib.Path) -> Manifest:
    sectie = _pakketsectie(pad.read_text(encoding="utf-8"))
    naam = re.search(r'^\s*name\s*=\s*"([^"]+)"', sectie, re.M)
    if re.search(r"^\s*license\.workspace\s*=\s*true", sectie, re.M):
        decl, waarde = "workspace", None
    elif (m := re.search(r'^\s*license\s*=\s*"([^"]+)"', sectie, re.M)):
        decl, waarde = "license", m.group(1)
    elif (m := re.search(r'^\s*license-file\s*=\s*"([^"]+)"', sectie, re.M)):
        decl, waarde = "license-file", m.group(1)
    else:
        decl, waarde = None, None
    publiceerbaar = not re.search(r"^\s*publish\s*=\s*false", sectie, re.M)
    return Manifest(naam.group(1) if naam else None, decl, waarde, publiceerbaar)


def lees_toml(pad: pathlib.Path) -> dict | None:
    """A register that cannot be read is announced, never skipped past: a
    comparison against nothing would report agreement with nothing."""
    if not pad.is_file():
        print(f"SKIPPED (not a pass): {pad.relative_to(REPO)} is missing, so the "
              "registers cannot be compared", file=sys.stderr)
        return None
    try:
        return tomllib.loads(pad.read_text(encoding="utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as exc:
        print(f"SKIPPED (not a pass): {pad.relative_to(REPO)} does not parse "
              f"({exc}), so the registers cannot be compared", file=sys.stderr)
        return None


def lees_notice(pad: pathlib.Path) -> dict[str, tuple[str, str]] | None:
    """NOTICE, as `crate name -> (which list, what the second column says)`.

    Sections are found by their banner heading rather than by line number, and a
    heading that is neither of the two lists (EVALUATION AND TRIAL, CONTACT)
    switches collecting off -- prose under it must not read as crate rows.
    """
    if not pad.is_file():
        print(f"SKIPPED (not a pass): {pad.name} is missing, so the registers "
              "cannot be compared", file=sys.stderr)
        return None
    try:
        tekst = pad.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        print(f"SKIPPED (not a pass): {pad.name} cannot be read ({exc}), so the "
              "registers cannot be compared", file=sys.stderr)
        return None

    def streep(regel: str) -> bool:
        kaal = regel.strip()
        return bool(kaal) and set(kaal) == {"="}

    regels = tekst.splitlines()
    uit: dict[str, tuple[str, str]] = {}
    lijst: str | None = None
    i = 0
    while i < len(regels):
        # A heading is the line framed by two rules of `=`. Reading it that way
        # rather than by line number means a section can be moved or reworded
        # without the parser silently collecting the wrong prose.
        if streep(regels[i]) and i + 2 < len(regels) and streep(regels[i + 2]):
            kop = regels[i + 1].strip()
            lijst = next((v for k, v in NOTICE_KOPPEN.items() if k in kop), None)
            i += 3
            continue
        m = NOTICE_REGEL.match(regels[i])
        if lijst is not None and m:
            uit[m.group(1)] = (lijst, NOTICE_STREEP.sub("", m.group(2)))
        i += 1
    return uit


def _zegt(crate: str, a: str, a_zegt: str, b: str, b_zegt: str, waarom: str = "") -> str:
    """The message shape: crate, then the two sources and what each says."""
    staart = f". {waarom}" if waarom else ""
    return f"{crate}: {a} says {a_zegt}; {b} says {b_zegt}{staart}"


def vergelijk(dirnaam: str, m: Manifest | None, c: dict | None, g: dict | None,
              b: str | None, n: tuple[str, str] | None,
              werkruimte_licentie: str) -> list[str]:
    """Every statement the registers make about one crate, side by side."""
    uit: list[str] = []
    crate = f"crates/{dirnaam}"

    # --- presence: a crate in one register and not in another --------------
    if m is None:
        for reg, rij in ((R_CANONIEK, c), (R_GRENS, g)):
            if rij is not None:
                uit.append(_zegt(crate, reg, "there is a crate here", R_MANIFEST,
                                 "there is no Cargo.toml",
                                 "A row for a crate that is gone is a row nobody "
                                 "will ever check against anything"))
        return uit
    if g is None:
        uit.append(_zegt(crate, R_MANIFEST, "a crate exists", R_GRENS,
                         "nothing -- it is on no side of the boundary",
                         "A crate the boundary does not know is one "
                         "`publish = false` away from shipping under the "
                         "workspace MIT"))
        return uit  # everything below compares against the side; there is none

    zijde = g.get("side")
    grens_publish_false = bool(g.get("publish_false", False))

    # --- names -------------------------------------------------------------
    if m.naam != g.get("name"):
        uit.append(_zegt(crate, R_MANIFEST, f"name = {m.naam!r}", R_GRENS,
                         f"name = {g.get('name')!r}"))
    if c is not None and m.naam != c.get("published_name"):
        uit.append(_zegt(crate, R_MANIFEST, f"name = {m.naam!r}", R_CANONIEK,
                         f"published_name = {c.get('published_name')!r}"))

    # --- published or not: the fact that decides whether a canonical row exists
    if m.publiceerbaar == grens_publish_false:
        uit.append(_zegt(crate, R_MANIFEST,
                         "publishable" if m.publiceerbaar else "publish = false",
                         R_GRENS,
                         "publish_false = true" if grens_publish_false else
                         "no publish_false, so published",
                         "Whether a crate leaves the workspace is the fact every "
                         "other row depends on, and the two registers disagree on it"))
    if m.publiceerbaar and c is None:
        uit.append(_zegt(crate, R_MANIFEST, "publishable", R_CANONIEK,
                         "nothing -- no row",
                         "Every publish-eligible crate has a row there, or the "
                         "release gate publishes it against no decision at all"))
    if not m.publiceerbaar and c is not None:
        uit.append(_zegt(crate, R_MANIFEST, "publish = false", R_CANONIEK,
                         "a row, which that register keeps only for "
                         "publish-eligible crates",
                         "Either the crate is published and the manifest is "
                         "wrong, or the row describes a channel that does not exist"))

    # --- side against kind -------------------------------------------------
    if zijde not in ("ours", "forked", "internal"):
        uit.append(f"{crate}: {R_GRENS} says side = {zijde!r}, which is not one "
                   "of ours/forked/internal, so nothing can be translated from it")
        return uit
    if c is not None:
        verwacht = SOORT_VAN_ZIJDE.get(zijde)
        if verwacht is None:
            uit.append(_zegt(crate, R_GRENS, "side = 'internal' (never published)",
                             R_CANONIEK, f"license_kind = {c.get('license_kind')!r}",
                             "An internal crate has no canonical row; one that "
                             "does is being published, or is on the wrong side"))
        elif c.get("license_kind") != verwacht:
            uit.append(_zegt(crate, R_GRENS, f"side = {zijde!r} (so {verwacht})",
                             R_CANONIEK, f"license_kind = {c.get('license_kind')!r}"))

    # --- the expression itself ---------------------------------------------
    manifest_zegt = (f"license.workspace = true ({werkruimte_licentie})"
                     if m.decl == "workspace" else
                     f"{m.decl} = {m.waarde!r}" if m.decl else "no licence field")

    if c is not None:
        if (m.decl, m.waarde) != (c.get("license_decl"), c.get("license_value")):
            uit.append(_zegt(crate, R_CANONIEK,
                             f"{c.get('license_decl')} = {c.get('license_value')!r}",
                             R_MANIFEST, manifest_zegt))

    if zijde == "ours":
        if c is not None and c.get("license_value") != ONZE_LICENTIE:
            uit.append(_zegt(crate, R_GRENS, f"side = 'ours' (so {ONZE_LICENTIE!r})",
                             R_CANONIEK, f"license_value = {c.get('license_value')!r}"))
        if (m.decl, m.waarde) != ("license", ONZE_LICENTIE):
            uit.append(_zegt(crate, R_GRENS, f"side = 'ours' (so {ONZE_LICENTIE!r})",
                             R_MANIFEST, manifest_zegt))
    elif zijde == "forked":
        verklaart = g.get("declares")
        if not verklaart:
            uit.append(f"{crate}: {R_GRENS} books it as forked and records no "
                       "`declares`, so the upstream licence it must keep cannot "
                       "be compared with anything")
        else:
            if (m.decl, m.waarde) != ("license", verklaart):
                uit.append(_zegt(crate, R_GRENS, f"declares = {verklaart!r}",
                                 R_MANIFEST, manifest_zegt))
            if c is not None and c.get("license_value") != verklaart:
                uit.append(_zegt(crate, R_GRENS, f"declares = {verklaart!r}",
                                 R_CANONIEK, f"license_value = {c.get('license_value')!r}"))
    else:  # internal
        if m.decl != "workspace":
            uit.append(_zegt(crate, R_GRENS, "side = 'internal' (inherits the "
                             f"workspace {werkruimte_licentie})", R_MANIFEST, manifest_zegt))

    # --- the policy's own_packages entry, where there is one ---------------
    if b is not None and b != m.waarde:
        uit.append(_zegt(crate, R_BELEID, repr(b), R_MANIFEST, manifest_zegt))

    # --- NOTICE, the register the reader is actually handed (#345) ---------
    uitgegeven = m.publiceerbaar and not grens_publish_false
    verwachte_lijst = LIJST_VAN_ZIJDE.get(zijde) if uitgegeven else None
    if verwachte_lijst is None:
        if n is not None:
            uit.append(_zegt(crate, R_GRENS,
                             "side = 'internal'" if zijde == "internal" else
                             "publish_false = true" if grens_publish_false else
                             "not published",
                             R_KENNISGEVING, f"the {n[0]} list",
                             "NOTICE describes what is distributed, and this "
                             "crate never leaves the workspace"))
    elif n is None:
        uit.append(_zegt(crate, R_GRENS, f"side = {zijde!r}, published",
                         R_KENNISGEVING, "nothing -- it is in neither list",
                         "NOTICE is the only licence overview a reader is "
                         "handed, so a published crate it does not name carries "
                         "no statement at all"))
    else:
        gevonden, tweede_kolom = n
        if gevonden != verwachte_lijst:
            uit.append(_zegt(crate, R_GRENS,
                             f"side = {zijde!r} (so the {verwachte_lijst} list)",
                             R_KENNISGEVING, f"the {gevonden} list"))
        elif zijde == "forked" and g.get("declares") and tweede_kolom != g["declares"]:
            uit.append(_zegt(crate, R_GRENS, f"declares = {g['declares']!r}",
                             R_KENNISGEVING, repr(tweede_kolom),
                             "The reader takes the licence from NOTICE; a "
                             "second expression is a second answer"))
    return uit


def main() -> int:
    canon = lees_toml(CANONIEK)
    grens = lees_toml(GRENS)
    beleid = lees_toml(BELEID)
    werkruimte = lees_toml(WERKRUIMTE)
    notice = lees_notice(KENNISGEVING)
    if None in (canon, grens, beleid, werkruimte) or notice is None:
        return 3
    if not MANIFESTEN.is_dir():
        print(f"SKIPPED (not a pass): {MANIFESTEN.relative_to(REPO)} is missing, so "
              "there are no manifests to compare", file=sys.stderr)
        return 3

    werkruimte_licentie = (werkruimte.get("workspace", {}).get("package", {})
                           .get("license", "?"))

    manifesten: dict[str, Manifest] = {}
    for pad in sorted(MANIFESTEN.glob("*/Cargo.toml")):
        if not re.search(r"^\s*\[package\]", pad.read_text(encoding="utf-8"), re.M):
            continue
        manifesten[pad.parent.name] = lees_manifest(pad)
    if len(manifesten) < VLOER_CRATES:
        print(f"[registers] FATAL: {len(manifesten)} crate(s) found, floor is "
              f"{VLOER_CRATES}. This is not the workspace; a small tree agreeing "
              "with itself proves nothing.", file=sys.stderr)
        return 2
    if len(notice) < VLOER_NOTICE:
        print(f"[registers] FATAL: {len(notice)} crate(s) read out of "
              f"{R_KENNISGEVING}, floor is {VLOER_NOTICE}. A register that was "
              "not parsed agrees with every other one.", file=sys.stderr)
        return 2

    problemen: list[str] = []

    # Keyed by directory, because the three registers use two different names
    # for the same crate (crates/pdf-extract publishes as pdfluent-extract). The
    # directory is the one key all three share.
    canoniek: dict[str, dict] = {}
    for rij in canon.get("crate", []):
        d = rij.get("workspace_dir")
        if d in canoniek:
            problemen.append(f"crates/{d}: {R_CANONIEK} has two rows for one "
                             "directory, so it disagrees with itself")
        canoniek[d] = rij
    grensrijen: dict[str, dict] = {}
    for rij in grens.get("crate", []):
        rel = str(rij.get("dir", ""))
        # Only the workspace crates under `crates/<name>/` are comparable here.
        # Since #295 the boundary also books first-party packages that live
        # elsewhere -- `tools/pdfluent-snippet-extract`, `fuzz`,
        # `scripts/quality/lopdf_probe`, `pdfluent-examples/rust` and the
        # per-fork `crates/<x>/fuzz` -- and none of those is a published crate
        # with a row in canonical_licenses.toml, which is what this guard
        # compares. `removeprefix("crates/")` folded them in anyway, so
        # `tools/pdfluent-snippet-extract` became
        # `crates/tools/pdfluent-snippet-extract` and was reported as a crate
        # that is gone -- for a row that is correct.
        #
        # Skipping them leaves them guarded: license_boundary.py checks every
        # one, which is the whole subject of #295.
        segmenten = rel.split("/")
        if len(segmenten) != 2 or segmenten[0] != "crates":
            continue
        d = segmenten[1]
        if d in grensrijen:
            problemen.append(f"crates/{d}: {R_GRENS} has two rows for one "
                             "directory, so it disagrees with itself")
        grensrijen[d] = rij
    eigen: dict[str, str] = {}
    for sleutel, waarde in beleid.get("own_packages", {}).items():
        m = re.fullmatch(r"crates/([^/]+)/Cargo\.toml", sleutel)
        if m and isinstance(waarde, str):
            eigen[m.group(1)] = waarde

    # NOTICE is keyed by the published name, the one key it shares with nothing
    # else in this file; the boundary is what translates it back to a directory.
    noticerijen: dict[str, tuple[str, str]] = {}
    onbekend: dict[str, tuple[str, str]] = dict(notice)
    for d in sorted(set(manifesten) | set(grensrijen)):
        rij = grensrijen.get(d)
        manifest = manifesten.get(d)
        naam = (rij or {}).get("name") or (manifest.naam if manifest else None)
        if naam is not None and naam in onbekend:
            noticerijen[d] = onbekend.pop(naam)
    for naam, (lijst, _) in sorted(onbekend.items()):
        problemen.append(_zegt(naam, R_KENNISGEVING, f"the {lijst} list",
                               R_GRENS, "no crate of that name",
                               "NOTICE names a crate the reader will look for "
                               "and not find"))

    for d in sorted(set(manifesten) | set(canoniek) | set(grensrijen)):
        problemen.extend(vergelijk(d, manifesten.get(d), canoniek.get(d),
                                   grensrijen.get(d), eigen.get(d),
                                   noticerijen.get(d), werkruimte_licentie))

    if problemen:
        print(f"[registers] FAIL: {len(problemen)} disagreement(s) between the "
              "licence registers:", file=sys.stderr)
        for p in problemen:
            print(f"  - {p}", file=sys.stderr)
        print(f"\n  {R_CANONIEK} is the truth for a published crate's expression and "
              f"{R_GRENS}\n  for its side; {R_MANIFEST} and {R_KENNISGEVING} "
              "follow both. Fix the file that is not the truth for the\n  "
              "disputed fact. If the truth itself is wrong, "
              "that is an owner decision and goes in\n  its own commit "
              "(docs/release/PUBLISH_PROTOCOL.md).", file=sys.stderr)
        return 1

    print(f"[registers] OK: {len(manifesten)} crate(s) say the same thing in "
          f"{R_MANIFEST}, {R_CANONIEK} ({len(canoniek)} rows), {R_GRENS} "
          f"({len(grensrijen)} rows), {R_BELEID} ({len(eigen)} entries) and "
          f"{R_KENNISGEVING} ({len(notice)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
