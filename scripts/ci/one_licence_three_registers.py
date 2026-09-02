#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""One crate, one licence, three registers -- and something that compares them (#304).

Three files describe the licence of every crate in this workspace:

  crates/<dir>/Cargo.toml                 what crates.io and every reader sees
  docs/release/canonical_licenses.toml    what the release gate insists on
  docs/licensing/boundary.toml            which side of the licence line it is on

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
is a fourth statement about the same crate and is compared too.

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

# Short names for the messages. A path is precise; a name is what a reader
# recognises at a glance in a list of twelve lines.
R_MANIFEST = "Cargo.toml"
R_CANONIEK = "canonical_licenses.toml"
R_GRENS = "boundary.toml"
R_BELEID = "LICENSE_POLICY.toml [own_packages]"

# The one expression our own crates carry on every channel since #257.
ONZE_LICENTIE = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

# side -> license_kind. `internal` is absent on purpose: an internal crate has no
# canonical row, and the guard checks that absence rather than a value.
SOORT_VAN_ZIJDE = {"ours": "agpl-or-commercial", "forked": "open-source-derivative"}

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


def _zegt(crate: str, a: str, a_zegt: str, b: str, b_zegt: str, waarom: str = "") -> str:
    """The message shape: crate, then the two sources and what each says."""
    staart = f". {waarom}" if waarom else ""
    return f"{crate}: {a} says {a_zegt}; {b} says {b_zegt}{staart}"


def vergelijk(dirnaam: str, m: Manifest | None, c: dict | None, g: dict | None,
              b: str | None, werkruimte_licentie: str) -> list[str]:
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
    return uit


def main() -> int:
    canon = lees_toml(CANONIEK)
    grens = lees_toml(GRENS)
    beleid = lees_toml(BELEID)
    werkruimte = lees_toml(WERKRUIMTE)
    if None in (canon, grens, beleid, werkruimte):
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

    for d in sorted(set(manifesten) | set(canoniek) | set(grensrijen)):
        problemen.extend(vergelijk(d, manifesten.get(d), canoniek.get(d),
                                   grensrijen.get(d), eigen.get(d), werkruimte_licentie))

    if problemen:
        print(f"[registers] FAIL: {len(problemen)} disagreement(s) between the "
              "licence registers:", file=sys.stderr)
        for p in problemen:
            print(f"  - {p}", file=sys.stderr)
        print(f"\n  {R_CANONIEK} is the truth for a published crate's expression and "
              f"{R_GRENS}\n  for its side; {R_MANIFEST} follows both. Fix the file that "
              "is not the truth for the\n  disputed fact. If the truth itself is wrong, "
              "that is an owner decision and goes in\n  its own commit "
              "(docs/release/PUBLISH_PROTOCOL.md).", file=sys.stderr)
        return 1

    print(f"[registers] OK: {len(manifesten)} crate(s) say the same thing in "
          f"{R_MANIFEST}, {R_CANONIEK} ({len(canoniek)} rows), {R_GRENS} "
          f"({len(grensrijen)} rows) and {R_BELEID} ({len(eigen)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
