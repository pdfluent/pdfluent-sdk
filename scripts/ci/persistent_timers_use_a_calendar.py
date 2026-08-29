#!/usr/bin/env python3
"""`Persistent=true` only catches up missed runs on a calendar timer.

On a monotonic timer (OnBootSec/OnUnitActiveSec) systemd accepts the setting
and does nothing with it. The unit then reads as if it survives a suspend and
does not -- a config asserting a property it does not have, which is the same
class of failure as a check that stops looking.

It matters here because this timer replaced a GitHub schedule that fired twice
in a day (#279). Swapping one silent non-runner for another would have been
invisible.
"""
from __future__ import annotations

import configparser
import pathlib
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
MONOTOON = ("OnBootSec", "OnStartupSec", "OnUnitActiveSec", "OnUnitInactiveSec",
            "OnActiveSec")


def main() -> int:
    timers = sorted(WORTEL.glob("infra/**/*.timer"))
    stuk = []
    for pad in timers:
        lezer = configparser.ConfigParser(strict=False)
        lezer.optionxform = str
        try:
            lezer.read_string(pad.read_text())
        except configparser.Error as fout:
            stuk.append(f"{pad.name} is not readable as a unit file: {fout}")
            continue
        if not lezer.has_section("Timer"):
            stuk.append(f"{pad.name} has no [Timer] section, so it never fires")
            continue
        blok = lezer["Timer"]
        if blok.get("Persistent", "").strip().lower() not in ("true", "yes", "1", "on"):
            continue
        if not blok.get("OnCalendar", "").strip():
            aanwezig = [s for s in MONOTOON if blok.get(s)]
            stuk.append(
                f"{pad.name} sets Persistent= but fires on {', '.join(aanwezig) or 'nothing'}. "
                "systemd only replays missed runs for OnCalendar timers, so this unit "
                "claims to survive a suspend and does not."
            )
    # A unit that names a path nobody filled in fails every time the timer
    # fires, and a failing oneshot is quiet: the timer just keeps going.
    for unit in sorted(WORTEL.glob("infra/**/*.service")):
        tekst = unit.read_text()
        if "@CHECKOUT@" in tekst and not (WORTEL / "infra/reaper/install.sh").exists():
            stuk.append(f"{unit.name} has an unsubstituted placeholder and no installer "
                        "to fill it in")
        for regel in tekst.splitlines():
            if regel.startswith("WorkingDirectory=") and "@" not in regel:
                pad = regel.split("=", 1)[1].strip()
                stuk.append(f"{unit.name} hard-codes WorkingDirectory={pad}. The installer "
                            "must write the checkout it was run from, or the unit runs "
                            "somewhere else than the code being installed.")

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}", file=sys.stderr)
        return 1
    if not timers:
        print("SKIPPED (not a pass): no .timer units found under infra/.", file=sys.stderr)
        return 0
    print(f"[timers] OK: {len(timers)} timer(s); every Persistent= sits on a calendar.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
