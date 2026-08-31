#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
"""The licence boundary holds: each crate declares what its side says it does.

#220 asks for the boundary as data plus "een lint die faalt als een crate aan de
eigen kant van de grens een permissieve licentie declareert, of andersom".

THREE SIDES

  ours      dual-licensed AGPLv3 + commercial. Declares `license-file`, never a
            permissive SPDX expression.
  forked    a fork of somebody else's open source. Keeps its upstream permissive
            licence, exactly as NOTICE promises. Never `license-file`.
  internal  tooling that inherits the workspace `license = "MIT"`. Must be
            `publish = false`, because that MIT is only harmless while nothing
            carrying it reaches crates.io.

THE TRAP THIS EXISTS FOR

The workspace root declares MIT. Six crates inherit it and all six happen to be
publish = false, so nothing has ever been published as MIT. Nothing enforces
that. Add a crate, forget `publish = false`, and the proprietary SDK goes to
crates.io declaring MIT -- a grant that cannot be withdrawn from anyone who
fetched it. It fails no test and no build; it is simply true afterwards.

WHAT IS NOT CHECKED HERE, AND WHY

Whether crates.io agrees is checked by scripts/ci/license_registry_check.py,
which needs the network. This gate is offline and deliberately so: it has to run
in the commit hook, where a network call would make it skippable.
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
KAART = REPO / "docs" / "licensing" / "boundary.toml"

# The workspace default. A crate inheriting this must never be publishable.
WERKRUIMTE_LICENTIE = "MIT"

# A crate on our side of the line must not declare one of these.
PERMISSIEF = re.compile(r"\b(MIT|Apache-2\.0|BSD-[0-9]|ISC|Zlib|Unlicense|CC0)\b")

# FLOOR: a boundary with fewer crates than this is a boundary that stopped
# describing the workspace. Measured 31-08-2026 at 48; the floor sits under it
# so a crate may be removed without tripping this, but an empty or truncated
# map cannot pass as a clean one.
VLOER = 40


# --- the AGPL text itself ----------------------------------------------------

AGPL = REPO / "LICENSE-AGPL"

# Pinned to the FSF's own publication at https://www.gnu.org/licenses/agpl-3.0.txt,
# fetched 31-08-2026. Cross-checked word for word against SPDX's stored
# AGPL-3.0-only text: 5535 words in both, and the only three differences are
# `http` -> `https` in FSF/GNU URLs, which is the FSF's own migration and not a
# change to the licence.
#
# WHY A HASH AND NOT A STRUCTURAL CHECK
#
# An altered GPL is not the GPL. Edit one sentence and the result is
# incompatible with every other GPL work and loses the case law that gives the
# text its meaning -- and it would not look wrong, because it would still read
# like a licence. Counting section headings cannot catch a changed sentence
# inside section 7. A hash can, and nothing else here can.
#
# If this ever fails legitimately, the FSF published a new text. Then fetch it,
# diff it deliberately, and change this constant in the same commit -- do not
# make the constant follow the file.
AGPL_SHA256 = "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0"
AGPL_WOORDEN = 5535


def agpl_is_onaangeroerd() -> list[str]:
    """The AGPL text is byte-for-byte the one we pinned."""
    if not AGPL.is_file():
        return [f"{AGPL.name} is missing; the AGPL half of the dual licence has no text"]
    rauw = AGPL.read_bytes()
    echt = hashlib.sha256(rauw).hexdigest()
    if echt == AGPL_SHA256:
        return []
    woorden = len(rauw.decode("utf-8", "ignore").split())
    return [f"{AGPL.name} hashes to {echt[:16]}…, pinned is {AGPL_SHA256[:16]}… "
            f"({woorden} words, expected {AGPL_WOORDEN}). An edited GPL is not the "
            "GPL: it loses compatibility with every other GPL work and the case "
            "law that interprets it, and it still reads like a licence. If the FSF "
            "published a new text, diff it deliberately and move the pin in the "
            "same commit"]


def lees(pad: pathlib.Path) -> str:
    return pad.read_text(encoding="utf-8", errors="ignore")


def veld(tekst: str, naam: str) -> str | None:
    m = re.search(rf'^\s*{re.escape(naam)}\s*=\s*"([^"]+)"', tekst, re.M)
    return m.group(1) if m else None


def main() -> int:
    if not KAART.is_file():
        print(f"[boundary] FATAL: {KAART.relative_to(REPO)} is missing. Without it "
              "there is no boundary to check and this gate would pass on anything.",
              file=sys.stderr)
        return 1
    kaart = tomllib.loads(lees(KAART))
    rijen = kaart.get("crate") or []
    if len(rijen) < VLOER:
        print(f"[boundary] FATAL: {len(rijen)} crate(s) booked, floor is {VLOER}. "
              "A short map reads as a clean run and is not one.", file=sys.stderr)
        return 1

    problemen: list[str] = agpl_is_onaangeroerd()
    gezien: set[str] = set()

    for rij in rijen:
        naam, kant = rij["name"], rij["side"]
        gezien.add(rij["dir"])
        pad = REPO / rij["dir"] / "Cargo.toml"
        if not pad.is_file():
            problemen.append(f"{naam}: booked at {rij['dir']}, which has no Cargo.toml")
            continue
        t = lees(pad)
        erft = bool(re.search(r"^\s*license\.workspace\s*=\s*true", t, re.M))
        bestand = veld(t, "license-file")
        spdx = None if erft else veld(t, "license")
        publiceerbaar = not re.search(r"^\s*publish\s*=\s*false", t, re.M)

        if kant == "ours":
            if not bestand:
                problemen.append(
                    f"{naam} is on our side of the boundary and does not declare "
                    f"`license-file`. It declares {spdx or 'the workspace licence'}, "
                    "which offers the product under terms we cannot withdraw")
            if spdx and PERMISSIEF.search(spdx):
                problemen.append(
                    f"{naam} is ours and declares the permissive expression {spdx!r}")
        elif kant == "forked":
            if bestand:
                problemen.append(
                    f"{naam} is a fork of somebody else's work and declares "
                    "`license-file`, which puts a PDFluent licence on code that is "
                    "not ours to relicense. NOTICE promises the opposite")
            if spdx and not PERMISSIEF.search(spdx):
                problemen.append(
                    f"{naam} is booked as a fork but declares {spdx!r}, which is not "
                    "the permissive licence NOTICE promises for it")
            if rij.get("declares") and spdx and rij["declares"] != spdx:
                problemen.append(
                    f"{naam} declares {spdx!r}; the boundary records "
                    f"{rij['declares']!r}. Upstream's licence changed, or ours did")
        elif kant == "internal":
            if not erft:
                problemen.append(
                    f"{naam} is booked as internal but does not inherit the "
                    "workspace licence")
            if publiceerbaar:
                problemen.append(
                    f"{naam} inherits the workspace licence "
                    f"({WERKRUIMTE_LICENTIE}) and is publishable. That combination "
                    "ships the proprietary SDK to crates.io declaring "
                    f"{WERKRUIMTE_LICENTIE} -- a grant nobody can take back. Set "
                    "`publish = false`, or move it off the internal side")
        else:
            problemen.append(f"{naam}: side {kant!r} is not one of ours/forked/internal")

    # A crate in the tree and not on the map is the actual failure mode: the map
    # is complete on the day it is written and silently stops being so.
    for pad in sorted(REPO.glob("crates/*/Cargo.toml")):
        d = f"crates/{pad.parent.name}"
        if d in gezien:
            continue
        t = lees(pad)
        if not re.search(r"^\s*\[package\]", t, re.M):
            continue
        problemen.append(
            f"{d} exists and is on no side of the boundary. A new crate defaults "
            "to the workspace MIT, so an unbooked crate is one `publish = false` "
            "away from publishing the SDK permissively")

    if not problemen:
        print(f"[boundary] {len(rijen)} crate(s); every one declares what its side says")
        return 0
    print(f"[boundary] {len(problemen)} problem(s) on the licence boundary:",
          file=sys.stderr)
    for p in problemen:
        print(f"  - {p}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
