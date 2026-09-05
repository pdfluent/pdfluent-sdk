#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The ways `the_order_form_agrees_with_the_licence.py` must go red (#220).

The guard's whole job is to find nothing: on a good day the licence and the form
agree, and it prints one line. That is indistinguishable from a guard that reads
neither file, so every case below MUTATES a throwaway copy of the real licence
and the real form and demands the verdict flip.

Each mutation is a way these two documents have actually drifted apart in
practice, or the way #220 found the licence files wrong in this repository:
an entity that does not exist under a signature line, a form drawn against a
superseded version, a grant on one side and not the other.

Exit codes:
    0  every case flipped as expected
    1  one did not
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
GUARD = "scripts/ci/the_order_form_agrees_with_the_licence.py"
FORM = "docs/licensing/order-form.md"
LICENCE = "LICENSE-COMMERCIAL"

# Everything the guard reads, plus everything the form points at: the reference
# check is one of the cases, so the fixture has to be a tree where those paths
# really exist.
BESTANDEN = [GUARD, FORM, LICENCE, "NOTICE", "docs/licensing/boundary.toml"]

# FLOOR: cases >= 12 -- this file is the specification of what the guard
# refuses, and a shortened list is a quietly narrowed guard.
FLOOR_CASES = 12

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'} {what}")
    if not ok:
        fails.append(f"{what}: {detail}")


def bouw_basis(td: pathlib.Path) -> pathlib.Path:
    root = td / "basis"
    for rel in BESTANDEN:
        doel = root / rel
        doel.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / rel, doel)
    return root


def geval(td: pathlib.Path, basis: pathlib.Path, naam: str,
          muteer) -> subprocess.CompletedProcess:
    root = td / re.sub(r"[^\w]+", "_", naam)
    shutil.copytree(basis, root)
    muteer(root)
    return subprocess.run([sys.executable, str(root / GUARD)], cwd=root,
                          capture_output=True, text=True)


def herschrijf(rel: str, oud: str, nieuw: str, count: int = 1):
    def f(root: pathlib.Path) -> None:
        p = root / rel
        t = p.read_text(encoding="utf-8")
        assert t.count(oud) == count, f"{rel}: {oud!r} occurs {t.count(oud)}x, not {count}x"
        p.write_text(t.replace(oud, nieuw), encoding="utf-8")
    return f


def rood(what: str, r: subprocess.CompletedProcess, *noemt: str) -> None:
    expect(f"{what} -> red", r.returncode == 1, f"exit={r.returncode} {r.stdout[-300:]}")
    for n in noemt:
        expect(f"{what} -> names {n!r}", n in r.stderr, r.stderr[-400:])


with tempfile.TemporaryDirectory(prefix="orderform-") as _td:
    td = pathlib.Path(_td)
    basis = bouw_basis(td)

    # --- 0. the copy of the real pair passes, so every red below is the mutation
    r = geval(td, basis, "unmutated", lambda root: None)
    expect("the unmutated fixture passes", r.returncode == 0, r.stderr[-400:] or r.stdout)

    # --- 1. the state this issue found: §10 pointing at a form that is not there
    r = geval(td, basis, "form absent", lambda root: (root / FORM).unlink())
    rood("the order form is missing", r, "10", "points at nothing")

    # --- 2. a grant added to the licence and not to the form. The commercial
    # half is sold on the form; a right that never appears there is one the
    # licensee is never asked about and therefore never buys.
    r = geval(td, basis, "grant e added",
              herschrijf(LICENCE, "\n\nPerpetual means",
                         "\n  e. Redistribution of the source under the licensee's own terms.\n"
                         "\nPerpetual means"))
    rood("a grant the form has no row for", r, "no scope row for", "(e)")

    # --- 3. the opposite, and the more dangerous direction: the form is the
    # document that prevails, so a row with no grant behind it grants it.
    r = geval(td, basis, "orphan row",
              herschrijf(FORM, "| d | Deployment in environments",
                         "| z | Resale of the source to third parties | ☐ |\n"
                         "| d | Deployment in environments"))
    rood("a form row no grant stands behind", r, "does not grant")

    # --- 4. the quiet one: the row keeps its letter and stops meaning the grant
    r = geval(td, basis, "row b rewritten",
              herschrijf(FORM, "| b | Operation as a network service, without the "
                               "AGPL section 13 obligation |",
                         "| b | Operation as a network service |"))
    rood("a row that stops carrying its grant", r, "does not carry the grant")

    # --- 5. #220 found exactly this in LICENSE-MIT: a signature under the name
    # of an entity that does not exist.
    r = geval(td, basis, "wrong entity",
              herschrijf(FORM, "Licensor: Innovation Trigger B.V., trading as PDFluent, Netherlands.",
                         "Licensor: PDFluent BV, Netherlands.", 1))
    rood("the form names an entity the licence does not", r, "binds a party")

    # --- 6. the licence is revised and the form still quotes the old one. v1.0
    # described licence keys and a 30-day expiry; neither exists.
    r = geval(td, basis, "version moved",
              herschrijf(LICENCE, "Version 2.0 — 31 August 2026",
                         "Version 3.0 — 1 October 2026"))
    rood("the licence version moves and the form does not", r, "version", "date")

    # --- 7/8. the rule between the two documents, on each side in turn
    r = geval(td, basis, "form drops precedence",
              herschrijf(FORM, "conflict, the order form prevails.",
                         "conflict, the documents are read together."))
    rood("the form no longer says it prevails", r, "prevails")

    r = geval(td, basis, "licence drops precedence",
              herschrijf(LICENCE, "Where they conflict, the order form prevails.",
                         "The documents are read together."))
    rood("the licence no longer says the form prevails", r, "10")

    # --- 9. a reference that stopped resolving. A form telling a buyer to read
    # a file that was renamed is worth what the file is worth.
    r = geval(td, basis, "dangling reference",
              lambda root: (root / "docs" / "licensing" / "boundary.toml").unlink())
    rood("the form points at a file that is gone", r, "boundary.toml")

    # --- 10. a truncated form agrees with the licence about everything it no
    # longer mentions -- the same lesson as the boundary map's floor.
    def snij(root: pathlib.Path) -> None:
        p = root / FORM
        t = p.read_text(encoding="utf-8")
        p.write_text(t[:t.index("## 4. Fee")], encoding="utf-8")
    r = geval(td, basis, "form truncated", snij)
    rood("a truncated form", r, "## 4. Fee")

    # --- 11. and a §2 that parses to nothing agrees with any form at all
    def leeg(root: pathlib.Path) -> None:
        p = root / LICENCE
        t = p.read_text(encoding="utf-8")
        begin = t.index("2. What you get")
        eind = t.index("3. What is not covered")
        p.write_text(t[:begin] + "2. What you get\n---------------\n\n" + t[eind:],
                     encoding="utf-8")
    r = geval(td, basis, "grants emptied", leeg)
    rood("a section 2 that parses to nothing", r, "floor")

    # --- 12. restored
    r = geval(td, basis, "restored", lambda root: None)
    expect("restored -> green", r.returncode == 0, r.stderr[-300:])

expect(f"at least {FLOOR_CASES} cases ran", ran >= FLOOR_CASES, f"ran={ran}")

if fails:
    print(f"\n[test_order_form] {len(fails)} of {ran} case(s) did not behave:",
          file=sys.stderr)
    for f in fails:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)
print(f"[test_order_form] {ran} case(s); the form and the licence cannot drift apart quietly")
