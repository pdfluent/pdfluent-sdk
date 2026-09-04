#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""A cross-compilation target must land on the toolchain cargo will use.

`dtolnay/rust-toolchain@stable` installs stable and adds the requested target
to stable. cargo then reads rust-toolchain.toml, switches to the pinned channel,
and finds no std for that target there:

    error[E0463]: can't find crate for `core`

The message names neither the toolchain nor the target, so it reads as a code
problem. Five workflows had this shape and WASM Surface Guard ran red on it.

The Hetzner snapshot hit the same thing from the other side -- `rustup target
add wasm32-unknown-unknown` against "stable" while the image pinned 1.94.0 --
which is why the workflow that boots it checks the pin explicitly.

WHAT THIS CHECKS

Any dtolnay/rust-toolchain step that requests a non-host target must also name
a toolchain, and that toolchain must equal the channel in rust-toolchain.toml.
A step with no `targets:` is left alone: it compiles for the host, where the
pinned channel brings its own std.

Exit codes:
    0  every cross-target step names the pinned channel
    1  one does not, or the scan found nothing to look at
"""

from __future__ import annotations

# FLOOR: toolchain steps inspected >= 8 -- there are far more across the
# workflows, and a scan that reads a couple has lost its glob.
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
FLOWS = REPO / ".github" / "workflows"
PIN = REPO / "rust-toolchain.toml"
FLOOR = 8

USES = re.compile(r"^(?P<inspring>\s*)(?:- )?uses:\s*dtolnay/rust-toolchain@(?P<kanaal>[\w.]+)\s*$")


def stappen(tekst: str):
    """Every dtolnay/rust-toolchain step, with the whole step read.

    Not a single regex over `uses:` followed immediately by `with:`. A step may
    put `if:`, `id:` or `name:` between the two, and a pattern that expects them
    adjacent then finds no `with:` block, misses the `targets:` inside it, and
    passes the step. Codex caught that on #1539 -- a false negative in the guard
    itself, which is the failure this whole family of checks exists to prevent.

    The step runs from its `uses:` line to the next line at or left of its own
    indentation that starts a new list item.
    """
    regels = tekst.splitlines()
    for i, regel in enumerate(regels):
        m = USES.match(regel)
        if not m:
            continue
        diep = len(m.group("inspring"))
        blok = []
        for volgende in regels[i + 1:]:
            if not volgende.strip():
                blok.append(volgende)
                continue
            insp = len(volgende) - len(volgende.lstrip())
            if volgende.lstrip().startswith("- ") and insp <= diep:
                break
            if insp <= diep and not volgende.lstrip().startswith("#"):
                break
            blok.append(volgende)
        yield i + 1, m.group("kanaal"), "\n".join(blok)


def gepind() -> str | None:
    if not PIN.is_file():
        return None
    m = re.search(r'^channel\s*=\s*"([^"]+)"', PIN.read_text(), re.M)
    return m.group(1) if m else None


def main() -> int:
    kanaal = gepind()
    if not kanaal:
        print("[wasm_target] FATAL: rust-toolchain.toml has no channel; without it there is "
              "nothing to compare against", file=sys.stderr)
        return 1
    if not FLOWS.is_dir():
        print(f"[wasm_target] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    bekeken, fout = 0, []
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        tekst = pad.read_text()
        for regel, kanaal_van_actie, blok in stappen(tekst):
            bekeken += 1
            doel = re.search(r"targets:\s*(\S+)", blok)
            if not doel:
                continue  # host build; the pinned channel brings its own std
            tc = re.search(r"toolchain:\s*'?([\w.${}\w\s.-]+?)'?\s*$", blok, re.M)
            if not tc:
                fout.append((pad.name, regel, doel.group(1),
                             f"no toolchain named; the action installs "
                             f"{kanaal_van_actie} and cargo will use {kanaal}"))
            elif "${{" in tc.group(1):
                # Derived from the checked-out ref rather than written out.
                # expensive-validation.yml runs against an arbitrary branch or
                # tag whose rust-toolchain.toml may pin something else, so a
                # literal here would be wrong one step removed. Codex raised
                # this on #1539.
                continue
            elif tc.group(1) != kanaal:
                fout.append((pad.name, regel, doel.group(1),
                             f"names {tc.group(1)}, rust-toolchain.toml pins {kanaal}"))

    if bekeken < FLOOR:  # FLOOR
        print(f"[wasm_target] FATAL: {bekeken} toolchain step(s) read, floor is {FLOOR}. "
              f"The glob lost its files, and an empty scan reports a clean tree.",
              file=sys.stderr)
        return 1

    print(f"[wasm_target] {bekeken} toolchain step(s) inspected, pin is {kanaal}")
    if not fout:
        print("[wasm_target] every cross-target step names the pinned channel")
        return 0

    print()
    print(f"[wasm_target] {len(fout)} step(s) add a target to the wrong toolchain:")
    for bestand, regel, doel, waarom in fout:
        print(f"  {bestand}:{regel}  targets: {doel}")
        print(f"    {waarom}")
    print()
    print("[wasm_target] cargo reads rust-toolchain.toml and switches channel, so a target")
    print("[wasm_target] installed on stable is not there when it compiles. The error it")
    print("[wasm_target] produces -- can't find crate for `core` -- names neither the")
    print("[wasm_target] toolchain nor the target, and reads as a code problem.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
