#!/usr/bin/env python3
"""The checkout path is data being written into unit-file syntax.

Three rounds of review found three ways that goes wrong, each narrower than
the last: sed metacharacters, then the sed delimiter, then systemd specifiers.
The lesson is not any one of them -- it is that interpolating into a foreign
syntax needs that syntax's escaping, and shell-safety is not it.

Extracts the substitution the installer performs and checks it against paths
that are legal on disk and dangerous in a unit file.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
INSTALL = WORTEL / "infra/reaper/install.sh"
UNIT = WORTEL / "infra/reaper/pdfluent-reaper.service"

LASTIG = [
    ("/opt/xfa%foo", "%"),      # a systemd specifier
    ("/opt/a&b/xfa", "&"),      # sed replacement backreference
    ("/opt/a|b/xfa", "|"),      # the delimiter a sed version used
]


def vervang(pad: str) -> str:
    """Mirror the installer's substitution, read out of the installer itself."""
    tekst = INSTALL.read_text()
    if 'replace("%", "%%")' not in tekst:
        raise AssertionError("the installer no longer escapes systemd specifiers")
    if "sed " in tekst.split("SUBST")[0].split("CHECKOUT=")[-1]:
        raise AssertionError("the installer went back to sed for the substitution")
    return UNIT.read_text().replace("@CHECKOUT@", pad.replace("%", "%%"))


def main() -> int:
    stuk = []
    for pad, teken in LASTIG:
        try:
            uit = vervang(pad)
        except AssertionError as fout:
            print(f"FAIL: {fout}")
            return 1
        if "@CHECKOUT@" in uit:
            stuk.append(f"the placeholder survived a path containing `{teken}`")
        regel = next((r for r in uit.splitlines()
                      if r.startswith("WorkingDirectory=")), None)
        if regel is None:
            stuk.append("the unit lost its WorkingDirectory")
            continue
        waarde = regel.split("=", 1)[1]
        # A single % left in the value would be read as a specifier by systemd.
        if re.search(r"(?<!%)%(?!%)", waarde):
            stuk.append(f"an unescaped systemd specifier survived in `{waarde}`")
        if pad.replace("%", "%%") not in waarde:
            stuk.append(f"the path `{pad}` did not land intact in `{waarde}`")

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}")
        return 1
    print(f"[installer] OK: {len(LASTIG)} awkward path(s) land intact in the unit.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
