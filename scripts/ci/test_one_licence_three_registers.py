#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Two-way proof for one_licence_three_registers.py (#304, #345).

Every case copies the real registers and the real manifests into a scratch
tree, changes ONE statement in ONE register, and demands that the guard go red
naming the crate and the two sources that now disagree. The unmutated copy must
pass, and a register that cannot be read must announce SKIPPED and exit 3 --
a comparison against nothing that returned 0 would be the exact failure the
three registers had before this guard existed.

The mutations are the ones the issue is about: a value changed in one register
only, a side flipped, a row removed, a crate added to the tree and nowhere else,
and the publish flag moved in one file but not the other.

Since #345 NOTICE is the fourth register, so the same treatment applies to it:
a crate in the wrong list, a licence expression that differs from the boundary
by the order of its operands, a published crate NOTICE forgets, one it names
that is never distributed, and a name no register knows. The first of those is
the failure the issue was opened for -- NOTICE booked `pdf-render` and
`pdf-font` as AGPL-or-commercial while the other three registers had them as
Apache-2.0 OR MIT forks -- and running this suite against the NOTICE text as it
stood before that fix goes red on thirteen disagreements.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/one_licence_three_registers.py"
CANONIEK = "docs/release/canonical_licenses.toml"
GRENS = "docs/licensing/boundary.toml"
BELEID = "docs/LICENSE_POLICY.toml"
KENNISGEVING = "NOTICE"
ONZE = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

def crates_in_de_werkruimte() -> int:
    """How many crates the guard will find, counted the way the guard counts.

    This assertion used to read `"48 crate(s)" in r.stdout`. It was the one
    hardcoded number in 132 assertions, it had to be edited every time a crate
    was added, and it failed looking like a licence problem when it was a
    counting problem. Walking `crates/*/Cargo.toml` for a `[package]` table is
    what `one_licence_three_registers.py` itself does, so the assertion still
    says "it compared all of them" without saying how many that is today.
    """
    return sum(
        1 for pad in sorted((REPO / "crates").glob("*/Cargo.toml"))
        if re.search(r"^\s*\[package\]", pad.read_text(encoding="utf-8"), re.M)
    )


fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def boom(root: pathlib.Path) -> None:
    """The real tree, reduced to what the guard reads."""
    (root / "scripts" / "ci").mkdir(parents=True)
    shutil.copy(REPO / GUARD, root / GUARD)
    for rel in (CANONIEK, GRENS, BELEID, KENNISGEVING, "Cargo.toml"):
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / rel, root / rel)
    for cargo in sorted((REPO / "crates").glob("*/Cargo.toml")):
        doel = root / "crates" / cargo.parent.name / "Cargo.toml"
        doel.parent.mkdir(parents=True)
        shutil.copy(cargo, doel)


def run_with(mutate: Callable[[pathlib.Path], None] | None) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td) / "repo"
        boom(root)
        if mutate:
            mutate(root)
        return subprocess.run([sys.executable, str(root / GUARD)],
                              capture_output=True, text=True)


def edit(rel: str, oud: str, nieuw: str, count: int = 1) -> Callable[[pathlib.Path], None]:
    """Replace `oud` in one file and refuse to proceed if it was not there: a
    mutation that did not apply makes the case pass vacuously."""
    def f(root: pathlib.Path) -> None:
        p = root / rel
        t = p.read_text()
        assert oud in t, f"mutation did not apply: {oud!r} not in {rel}"
        p.write_text(t.replace(oud, nieuw, count))
    return f


def zonder_rij(rel: str, sleutel: str, waarde: str) -> Callable[[pathlib.Path], None]:
    """Remove the whole `[[crate]]` table whose `<sleutel> = "<waarde>"`."""
    def f(root: pathlib.Path) -> None:
        p = root / rel
        t = p.read_text()
        blokken = re.split(r"(?=^\[\[crate\]\])", t, flags=re.M)
        rest = [b for b in blokken if not re.search(
            rf'^{re.escape(sleutel)}\s*=\s*"{re.escape(waarde)}"', b, re.M)]
        assert len(rest) == len(blokken) - 1, f"expected exactly one row with {sleutel}={waarde}"
        p.write_text("".join(rest))
    return f


def rood(what: str, r: subprocess.CompletedProcess, crate: str, *fragmenten: str) -> None:
    """Red, naming the crate and every fragment -- the two sources and both values."""
    expect(what, r.returncode == 1, f"exit={r.returncode} {r.stderr[-300:]}")
    expect(f"  names {crate}", crate in r.stderr, r.stderr[-300:])
    for frag in fragmenten:
        expect(f"  and says {frag!r}", frag in r.stderr, r.stderr[-300:])
    expect("  and does not crash", "Traceback" not in r.stderr)


print("one licence, three registers -- two-way")

r = run_with(None)
expect("the real tree passes unmutated", r.returncode == 0, r.stderr[-300:])
expect(f"  and says it compared all {crates_in_de_werkruimte()} crates",
       f"{crates_in_de_werkruimte()} crate(s)" in r.stdout, r.stdout)

# --- a value changed in ONE register --------------------------------------
OUD_ANNOT = 'published_name = "pdf-annot"\nworkspace_dir  = "pdf-annot"\nlicense_kind   = "agpl-or-commercial"\nlicense_decl   = "license"\nlicense_value  = "' + ONZE + '"'
rood("canonical value changed for pdf-annot",
     run_with(edit(CANONIEK, OUD_ANNOT, OUD_ANNOT.replace(ONZE, "AGPL-3.0-or-later OR LicenseRef-PDFluent-Commercial"))),
     "crates/pdf-annot", "canonical_licenses.toml", "Cargo.toml",
     "AGPL-3.0-or-later OR LicenseRef-PDFluent-Commercial", ONZE)

rood("manifest licence changed for pdf-syntax",
     run_with(edit("crates/pdf-syntax/Cargo.toml", 'license = "Apache-2.0 OR MIT"', 'license = "MIT"')),
     "crates/pdf-syntax", "canonical_licenses.toml", "Cargo.toml", "boundary.toml",
     "'Apache-2.0 OR MIT'", "'MIT'")

rood("boundary `declares` changed for pdfluent-lopdf",
     run_with(edit(GRENS, 'name = "pdfluent-lopdf"\ndir = "crates/lopdf"\nside = "forked"\ndeclares = "MIT"',
                   'name = "pdfluent-lopdf"\ndir = "crates/lopdf"\nside = "forked"\ndeclares = "Apache-2.0"')),
     "crates/lopdf", "boundary.toml", "declares = 'Apache-2.0'", "'MIT'")

rood("boundary `declares` removed from a fork",
     run_with(edit(GRENS, 'name = "pdfluent-lopdf"\ndir = "crates/lopdf"\nside = "forked"\ndeclares = "MIT"',
                   'name = "pdfluent-lopdf"\ndir = "crates/lopdf"\nside = "forked"')),
     "crates/lopdf", "records no `declares`")

# The policy's own entry for a cargo manifest is a fourth statement.
rood("policy own_packages disagrees with the xfa-wasm manifest",
     run_with(edit(BELEID, f'"crates/xfa-wasm/Cargo.toml" = "{ONZE}"',
                   '"crates/xfa-wasm/Cargo.toml" = "MIT"')),
     "crates/xfa-wasm", "LICENSE_POLICY.toml [own_packages]", "Cargo.toml", "'MIT'")

# The field NAME is part of the statement. LC9 replaced `license-file` with
# `license`; a canonical row that still says the old field, with the right
# value, is caught only by the direct canonical-against-manifest comparison --
# the side checks look at the value and would both be satisfied.
rood("canonical still says license-file for pdf-annot",
     run_with(edit(CANONIEK, OUD_ANNOT, OUD_ANNOT.replace('license_decl   = "license"',
                                                         'license_decl   = "license-file"'))),
     "crates/pdf-annot", "canonical_licenses.toml", "Cargo.toml", "license-file = ", "license = ")


def onuitgegeven_fork(root: pathlib.Path) -> None:
    """A fork that is publish = false has no canonical row, so its manifest can
    only be compared with what the boundary says it declares. Take pdf-syntax
    off crates.io consistently in both registers, then change its manifest."""
    edit("crates/pdf-syntax/Cargo.toml", 'license = "Apache-2.0 OR MIT"',
         'license = "MIT"\npublish = false')(root)
    edit(GRENS, 'name = "pdf-syntax"\ndir = "crates/pdf-syntax"\nside = "forked"',
         'name = "pdf-syntax"\ndir = "crates/pdf-syntax"\nside = "forked"\npublish_false = true')(root)
    zonder_rij(CANONIEK, "published_name", "pdf-syntax")(root)


rood("an unpublished fork whose manifest left its declared licence",
     run_with(onuitgegeven_fork), "crates/pdf-syntax", "boundary.toml", "Cargo.toml",
     "declares = 'Apache-2.0 OR MIT'", "license = 'MIT'")

# --- a side flipped --------------------------------------------------------
rood("pdf-annot flipped to forked on the boundary",
     run_with(edit(GRENS, 'name = "pdf-annot"\ndir = "crates/pdf-annot"\nside = "ours"',
                   'name = "pdf-annot"\ndir = "crates/pdf-annot"\nside = "forked"\ndeclares = "MIT"')),
     "crates/pdf-annot", "boundary.toml", "canonical_licenses.toml",
     "side = 'forked'", "license_kind = 'agpl-or-commercial'")

rood("pdf-syntax flipped to ours on the boundary",
     run_with(edit(GRENS, 'name = "pdf-syntax"\ndir = "crates/pdf-syntax"\nside = "forked"\ndeclares = "Apache-2.0 OR MIT"',
                   'name = "pdf-syntax"\ndir = "crates/pdf-syntax"\nside = "ours"')),
     "crates/pdf-syntax", "side = 'ours'", "license_kind = 'open-source-derivative'")

rood("an internal crate given a canonical row",
     run_with(lambda root: (root / CANONIEK).write_text(
         (root / CANONIEK).read_text() + '\n[[crate]]\npublished_name = "pdf-bench"\n'
         'workspace_dir  = "pdf-bench"\nlicense_kind   = "agpl-or-commercial"\n'
         'license_decl   = "license"\nlicense_value  = "' + ONZE + '"\nrequired_files = []\n')),
     "crates/pdf-bench", "side = 'internal'", "canonical_licenses.toml")

rood("the canonical kind changed while the side did not",
     run_with(edit(CANONIEK, OUD_ANNOT, OUD_ANNOT.replace("agpl-or-commercial", "commercial"))),
     "crates/pdf-annot", "boundary.toml", "canonical_licenses.toml",
     "license_kind = 'commercial'", "side = 'ours'")

# --- a crate missing from one register -------------------------------------
rood("a canonical row removed for a publishable crate",
     run_with(zonder_rij(CANONIEK, "published_name", "pdf-annot")),
     "crates/pdf-annot", "canonical_licenses.toml", "no row")

rood("a boundary row removed",
     run_with(zonder_rij(GRENS, "name", "pdf-annot")),
     "crates/pdf-annot", "boundary.toml", "on no side")


def nieuwe_crate(root: pathlib.Path) -> None:
    d = root / "crates" / "pdf-new-thing"
    d.mkdir()
    (d / "Cargo.toml").write_text('[package]\nname = "pdf-new-thing"\nversion = "0.1.0"\n'
                                  'license.workspace = true\n')


rood("a crate in the tree and in no register",
     run_with(nieuwe_crate), "crates/pdf-new-thing", "boundary.toml", "on no side")

rood("a boundary row for a directory that does not exist",
     run_with(lambda root: (root / GRENS).write_text(
         (root / GRENS).read_text() + '\n[[crate]]\nname = "pdf-ghost"\ndir = "crates/pdf-ghost"\nside = "ours"\n')),
     "crates/pdf-ghost", "no Cargo.toml")

# --- the publish flag moved in one file only -------------------------------
rood("publish = false dropped from the pdf-capi manifest",
     run_with(edit("crates/pdf-capi/Cargo.toml", "publish = false\n", "")),
     "crates/pdf-capi", "publishable", "publish_false = true", "no row")

rood("publish_false dropped from pdf-capi on the boundary",
     run_with(edit(GRENS, 'name = "pdf-capi"\ndir = "crates/pdf-capi"\nside = "ours"\npublish_false = true',
                   'name = "pdf-capi"\ndir = "crates/pdf-capi"\nside = "ours"')),
     "crates/pdf-capi", "publish = false", "no publish_false")

# --- names -----------------------------------------------------------------
rood("the boundary names the crate differently",
     run_with(edit(GRENS, 'name = "pdfluent-extract"\ndir = "crates/pdf-extract"',
                   'name = "pdf-extract"\ndir = "crates/pdf-extract"')),
     "crates/pdf-extract", "name = 'pdfluent-extract'", "name = 'pdf-extract'")

# --- an internal crate that stops inheriting ------------------------------
rood("an internal crate declaring its own licence",
     run_with(edit("crates/pdf-bench/Cargo.toml", "license.workspace = true", 'license = "MIT"')),
     "crates/pdf-bench", "side = 'internal'", "license = 'MIT'")

# --- ours, without a canonical row, still has to say the one expression ----
rood("an unpublished crate of ours declaring MIT",
     run_with(edit("crates/pdf-capi/Cargo.toml", f'license = "{ONZE}"', 'license = "MIT"')),
     "crates/pdf-capi", "side = 'ours'", "license = 'MIT'")

# --- NOTICE, the register the reader is handed (#345) ---------------------
# The regression the issue was opened for: a fork booked as one of ours.
OSS_RENDER = """  pdf-render           Apache-2.0 OR MIT
                       (substantially extended fork of hayro, co-authored by
                        Laurenz Stampfl)

"""


def render_terug_naar_commercieel(root: pathlib.Path) -> None:
    edit(KENNISGEVING, OSS_RENDER, "")(root)
    edit(KENNISGEVING, "  pdf-ocr              — OCR integration\n",
         "  pdf-ocr              — OCR integration\n"
         "  pdf-render           — page rendering (substantially extended fork)\n")(root)


rood("pdf-render moved back into the dual-licensed list",
     run_with(render_terug_naar_commercieel), "crates/pdf-render",
     "boundary.toml", "NOTICE", "side = 'forked' (so the open-source foundation list)",
     "the dual-licensed list")

rood("NOTICE states a fork's licence with the operands the other way round",
     run_with(edit(KENNISGEVING, "  pdf-syntax           Apache-2.0 OR MIT",
                   "  pdf-syntax           MIT OR Apache-2.0")),
     "crates/pdf-syntax", "NOTICE", "declares = 'Apache-2.0 OR MIT'",
     "'MIT OR Apache-2.0'")

rood("a published crate NOTICE does not name",
     run_with(edit(KENNISGEVING, "  xfa-license          — license enforcement runtime\n", "")),
     "crates/xfa-license", "NOTICE", "in neither list")

rood("an internal crate named in NOTICE",
     run_with(edit(KENNISGEVING, "  pdf-ocr              — OCR integration\n",
                   "  pdf-ocr              — OCR integration\n"
                   "  pdf-bench            — bench harness\n")),
     "crates/pdf-bench", "NOTICE", "side = 'internal'", "the dual-licensed list")

rood("a crate that is publish = false named in NOTICE",
     run_with(edit(KENNISGEVING, "  pdf-ocr              — OCR integration\n",
                   "  pdf-ocr              — OCR integration\n"
                   "  pdf-capi             — the C ABI\n")),
     "crates/pdf-capi", "NOTICE", "publish_false = true", "the dual-licensed list")

rood("NOTICE names a crate no register knows",
     run_with(edit(KENNISGEVING, "  pdf-ocr              — OCR integration\n",
                   "  pdf-ocr              — OCR integration\n"
                   "  pdf-ghostwriter      — nothing at all\n")),
     "pdf-ghostwriter", "NOTICE", "no crate of that name")


def kop_hernoemd(root: pathlib.Path) -> None:
    """A heading the parser no longer recognises collects nothing, and a
    register that was not read agrees with every other one."""
    edit(KENNISGEVING, "OPEN-SOURCE FOUNDATION", "PERMISSIVE FOUNDATION")(root)
    edit(KENNISGEVING, "DUAL-LICENSED COMPONENTS", "CRATES OF OUR OWN")(root)


r = run_with(kop_hernoemd)
expect("a NOTICE the parser cannot read is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode} {r.stderr[-300:]}")
expect("  and says so", "floor" in r.stderr and "NOTICE" in r.stderr, r.stderr[-300:])

# --- a register that cannot be read is not a pass --------------------------
for rel in (CANONIEK, GRENS, BELEID, KENNISGEVING, "Cargo.toml"):
    r = run_with(lambda root, rel=rel: (root / rel).unlink())
    expect(f"{rel} missing exits 3", r.returncode == 3, f"exit={r.returncode}")
    expect("  and announces SKIPPED (not a pass)", "SKIPPED (not a pass)" in r.stderr, r.stderr[-200:])

r = run_with(lambda root: (root / GRENS).write_text('[[crate]\nname = "broken'))
expect("an unparseable boundary exits 3", r.returncode == 3, f"exit={r.returncode}")
expect("  and announces SKIPPED (not a pass)", "SKIPPED (not a pass)" in r.stderr, r.stderr[-200:])
expect("  and does not crash", "Traceback" not in r.stderr)

r = run_with(lambda root: shutil.rmtree(root / "crates"))
expect("no crates/ at all exits 3", r.returncode == 3, f"exit={r.returncode}")


def klein(root: pathlib.Path) -> None:
    for d in sorted((root / "crates").iterdir())[5:]:
        shutil.rmtree(d)


r = run_with(klein)
expect("a tree under the floor is FATAL, not a pass", r.returncode == 2, f"exit={r.returncode}")
expect("  and says so", "floor" in r.stderr, r.stderr[-200:])

# --- the message tells the reader where the truth lives --------------------
r = run_with(edit(CANONIEK, OUD_ANNOT, OUD_ANNOT.replace(ONZE, "MIT")))
expect("a failure names the register that is the truth",
       "is the truth for a published crate's expression" in r.stderr, r.stderr[-400:])
expect("  and where a change to the truth goes", "PUBLISH_PROTOCOL" in r.stderr)

MINIMUM_CASES = 171  # FLOOR: set to what actually runs; a smaller suite passing is not this suite passing
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
if fails:
    for f in fails:
        print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}. Cases have gone "
          "missing.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
