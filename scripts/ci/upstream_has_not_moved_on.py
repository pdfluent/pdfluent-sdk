#!/usr/bin/env python3
"""Our forks must not silently fall behind the upstream they came from.

A fork does not rot loudly. Upstream ships fixes, we keep building on the
snapshot we took, and nothing anywhere says the gap is widening. hayro moved
from 0.5 to 0.7 while we stood still: +2822 lines in the parser alone, 71 of
them mentioning overflow, bounds, panic or malformed. That is robustness work
we simply did not have.

Standing rule (Jasper, 29-08-2026): track upstream periodically for every
external dependency we fork or vendor. If upstream gets better, we get better.

Reads docs/UPSTREAM_FORKS.toml, asks crates.io where upstream is now, and
fails when the gap is wider than one minor release. Network failures announce
themselves rather than passing quietly.
"""
from __future__ import annotations

import json
import pathlib
import sys
import tomllib
import urllib.error
import urllib.request

WORTEL = pathlib.Path(__file__).resolve().parents[2]
LIJST = WORTEL / "docs/UPSTREAM_FORKS.toml"
# How far behind is still a choice rather than neglect. One minor release is
# a deliberate "not yet"; two is nobody looking.
MINORS_TOEGESTAAN = 1


def nieuwste(crate: str) -> str | None:
    req = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{crate}",
        headers={"User-Agent": "pdfluent-upstream-check"},
    )
    with urllib.request.urlopen(req, timeout=20) as antwoord:
        return json.load(antwoord)["crate"]["max_version"]


def delen(versie: str) -> tuple[int, int]:
    """Major and minor only -- for measuring how wide a gap is."""
    stukken = versie.split(".")
    return int(stukken[0]), int(stukken[1] if len(stukken) > 1 else 0)


def volledig(versie: str) -> tuple[int, ...]:
    """Every component, for deciding whether an accepted version still covers.

    Comparing on major.minor alone made an acceptance of 0.7.2 also cover 0.7.3
    and everything after it, so patch releases -- which is where fixes land --
    would have slipped past the check that exists to notice them.
    """
    uit = []
    for stuk in versie.split("."):
        cijfers = "".join(c for c in stuk if c.isdigit())
        uit.append(int(cijfers) if cijfers else 0)
    return tuple(uit)


def main() -> int:
    if not LIJST.exists():
        print(f"FAIL: {LIJST.relative_to(WORTEL)} is missing; nothing records where "
              "the forks came from, so nothing can tell whether they are behind.",
              file=sys.stderr)
        return 1

    forks = tomllib.load(LIJST.open("rb"))["fork"]
    achter, onbereikbaar = [], []
    for f in forks:
        try:
            boven = nieuwste(f["upstream"])
        except (urllib.error.URLError, OSError, KeyError) as fout:
            onbereikbaar.append((f["upstream"], str(fout)))
            continue
        if boven is None:
            onbereikbaar.append((f["upstream"], "no version in the response"))
            continue
        wij, zij = delen(f["gelijk_met"]), delen(boven)
        gat = (zij[0] - wij[0]) * 100 + (zij[1] - wij[1])
        aanvaard_nu = f.get("aanvaard_tot")
        vrij = gat <= MINORS_TOEGESTAAN or (
            aanvaard_nu is not None and volledig(boven) <= volledig(aanvaard_nu))
        merk = "  " if vrij else "!!"
        print(f"[upstream] {merk} {f['onze_crate']:14} {f['gelijk_met']:>7} "
              f"<- {f['upstream']:18} now {boven}")
        aanvaard = f.get("aanvaard_tot")
        if aanvaard and volledig(boven) <= volledig(aanvaard):
            # A gap somebody chose, with an issue against it. Still printed, so
            # it stays visible; not fatal, so it does not block other work.
            continue
        if gat > MINORS_TOEGESTAAN:
            achter.append((f["onze_crate"], f["upstream"], f["gelijk_met"], boven))

    if onbereikbaar:
        for crate, reden in onbereikbaar:
            print(f"SKIPPED (not a pass): could not reach crates.io for {crate}: {reden}",
                  file=sys.stderr)
        if len(onbereikbaar) == len(forks):
            print("[upstream] FAIL: not one fork could be checked, so this run says "
                  "nothing about how far behind we are.", file=sys.stderr)
            return 1
        if not achter:
            return 0

    if achter:
        print(file=sys.stderr)
        print(f"[upstream] FAIL: {len(achter)} fork(s) more than {MINORS_TOEGESTAAN} minor "
              "release behind:", file=sys.stderr)
        for onze, boven, wij, zij in achter:
            print(f"  {onze} sits on {boven} {wij}; upstream is at {zij}", file=sys.stderr)
        print("\nUpstream fixes are the cheapest improvements available: somebody else "
              "already found the bug and wrote the fix. Move up, or record a deliberate "
              "decision not to in docs/UPSTREAM_FORKS.toml.", file=sys.stderr)
        return 1

    print(f"[upstream] OK: {len(forks)} fork(s), none more than {MINORS_TOEGESTAAN} minor "
          "behind.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
