#!/usr/bin/env python3
"""A hit must not publish the term it was looking for (#1660).

`geen_interne_zaken.py` screens commit messages and the tree against a list of
customer and partner names that is deliberately NOT in the repository -- a
forbidden-terms list that publishes its own terms leaks exactly what it exists
to stop. The list arrives as a secret in CI.

On a hit, the guard printed the matched term and its surrounding context to
stderr. A failing run's log is as public as the tree, so the guard published a
partner name at the moment it caught one: the one run where the term is
certainly a real one. `::add-mask::` does not cover it, because the rule matches
case-insensitively while masking is exact.

Both directions are asserted here, because either alone is the wrong fix:
under CI the term must NOT appear, and locally it MUST, since that is what makes
the message usable on the one machine whose log belongs to nobody else.
"""
from __future__ import annotations
import importlib.util, os, pathlib, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402
GUARD = REPO / "scripts" / "ci" / "geen_interne_zaken.py"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


spec = importlib.util.spec_from_file_location("giz", GUARD)
giz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(giz)


def giz_sha(t: str) -> str:
    import hashlib
    return hashlib.sha256(t.lower().encode()).hexdigest()

TERM = "Voorbeeldpartner"
CONTEXT = f"...werk voor {TERM} afgerond..."


_WEGWERP: list = []


def laad_prive(*termen: str) -> None:
    """Load a private list, the way a real run does.

    The redaction keys on the compiled private PATTERN, so a test that calls
    `_toonbaar` without loading one is testing the absence of a list rather
    than the redaction. The real flow always loads it before a finding can
    exist.
    """
    # Held so it is removed when the process ends. `mkdtemp()` does not clean
    # up, and this helper is called four times a run -- on a persistent runner
    # that is four directories per push, which is the leak I measured in the
    # other suites this morning and would have reintroduced here.
    td = tempfile.TemporaryDirectory()
    _WEGWERP.append(td)
    lijst = pathlib.Path(td.name) / "termen.txt"
    lijst.write_text("\n".join(termen) + "\n")
    giz.PRIVATE_PAD = str(lijst)
    assert giz.private_regel() is not None, "fixture list did not load"


def toonbaar(ci: bool, naam: str = "partner", wat: str = TERM, context: str = CONTEXT):
    oud = os.environ.get("CI")
    if ci:
        os.environ["CI"] = "true"
    else:
        os.environ.pop("CI", None)
    try:
        return giz._toonbaar(naam, wat, context)
    finally:
        if oud is None:
            os.environ.pop("CI", None)
        else:
            os.environ["CI"] = oud


print("a hit must not publish the term")
laad_prive(TERM)

wat, ctx = toonbaar(ci=True)
expect("under CI the term is not printed",
       TERM.lower() not in wat.lower() and TERM.lower() not in ctx.lower(),
       f"{wat!r} {ctx!r}")
expect("  nor is its context", CONTEXT.lower() not in ctx.lower(), ctx)
expect("  and it says a private term matched", "private term" in wat, wat)

# NO fingerprint. A digest of the term with its exact length beside it is
# reversible with a word list, so a redaction that publishes one is a slower
# way of publishing the secret.
expect("  and reveals neither the length nor a digest of it",
       str(len(TERM)) not in wat
       and giz_sha(TERM)[:8] not in wat.lower(), wat)
expect("  so two different terms are indistinguishable in the log",
       toonbaar(ci=True, wat="AndereNaam", context="x met AndereNaam erin")[0] == wat
       or "private term" in toonbaar(ci=True, wat="AndereNaam")[0])

wat, ctx = toonbaar(ci=False)
expect("locally the term IS printed", wat == TERM and ctx == CONTEXT, f"{wat!r}")

# THE KEYCHAIN PATH. `keychain_overtredingen()` matches a label against every
# rule including the private one, then reports it as `keychain-label` -- so a
# redaction keyed on the rule NAME printed the term in full down that path.
wat, ctx = toonbaar(ci=True, naam="keychain-label",
                    wat=f"security find-generic-password -s {TERM}",
                    context=f"...-s {TERM}...")
expect("a private term reported under ANOTHER rule name is still redacted",
       TERM.lower() not in wat.lower() and TERM.lower() not in ctx.lower(), f"{wat!r}")

# THE LOCATION COLUMN. `{path}:{line}` was printed outside the redaction, so a
# finding in a file whose NAME holds a private term published it in full --
# the match and the context withheld while the location beside them spelled it
# out. Three columns, one rule.
laad_prive("zzqgamma")
wat, _ = toonbaar(ci=True, naam="keychain-label",
                  wat="docs/ZZQGAMMA-contract.md:12", context="")
expect("a private term in the LOCATION is redacted too",
       "zzqgamma" not in wat.lower(), wat)

# And the other half: the scan has to look at the name at all. A file called
# after a customer publishes that customer in every clone however clean its
# contents are, and only the contents were read.
rx = giz.private_regel()[1]
expect("the private rule matches a path, not only a line of text",
       bool(rx.search("docs/ZZQGAMMA-contract.md")), "paths are not scanned")
laad_prive(TERM)

# CASE-INSENSITIVITY, asserted rather than assumed. Removing `re.I` from
# `private_regel()` left all nine earlier assertions green while the guard
# stopped seeing `zzqAlpha` written as `ZZQALPHA` -- and that property is the
# entire argument for redacting these hits at all: `::add-mask::` is exact, so
# a differently-cased term reaches the log unmasked. A suite that cannot see
# the property its own subject depends on is testing something else.
laad_prive("zzqalpha")
rx = giz.private_regel()[1]
expect("the private rule matches a differently-cased term",
       bool(rx.search("werk voor ZZQALPHA gedaan")), "re.I is missing")
expect("  and the exact spelling too", bool(rx.search("werk voor zzqalpha gedaan")))
expect("  and does not match an unrelated word", not rx.search("zzqbeta"))
laad_prive(TERM)

# Only the private rule is redacted. The built-in rules match words that are in
# the repository already, and hiding those would make the guard unusable for the
# thing it is mostly used for.
os.environ["CI"] = "true"
try:
    wat, ctx = giz._toonbaar("keychain", "security find-generic-password", "x")
finally:
    os.environ.pop("CI", None)
expect("a non-partner rule is not redacted", wat == "security find-generic-password", wat)

# And end to end, on a range that REALLY trips the rule. The term is taken from
# the range's own messages, so the hit is guaranteed rather than hoped for: a
# case that reports "no hit, skipped" and counts as a pass is the shape this
# repository keeps finding, and writing one into the test for a redaction would
# be a poor place to start.
bereik = "HEAD~3..HEAD"
# `env=sealed_env()`: inside a commit-msg or pre-push hook, GIT_DIR and
# GIT_WORK_TREE point at the REAL repository, and git then ignores the
# directory it was pointed at. A test that reads commit messages would be
# reading someone else's. The gitenv guard caught this one before it ran
# anywhere -- which is the guard working, on the test for another guard.
boodschappen = subprocess.run(["git", "log", "--format=%B", bereik], cwd=REPO,
                              capture_output=True, text=True, check=True,
                              env=sealed_env()).stdout
kandidaten = [w for w in ("lockfile", "regenerate", "binding", "workspace")
              if w in boodschappen.lower()]
assert kandidaten, f"no usable term in {bereik}; the fixture cannot guarantee a hit"
TREFFER = kandidaten[0]

with tempfile.TemporaryDirectory() as td:
    lijst = pathlib.Path(td) / "termen.txt"
    lijst.write_text(TREFFER + "\n")
    env = dict(os.environ, CI="true", PDFLUENT_INTERNE_TERMEN=str(lijst))
    r = subprocess.run([sys.executable, str(GUARD), "--bereik", bereik],
                       cwd=REPO, capture_output=True, text=True, env=env)
    expect(f"end to end, {TREFFER!r} really is a hit",
           r.returncode == 1 and "[partner]" in r.stderr,
           f"exit={r.returncode}: {r.stderr[:200]}")
    # `.lower()` on both sides and stdout included: the guard prints
    # `m.group(0)`, which is the text AS WRITTEN, so an assertion in one casing
    # against one stream can pass while the term is published in another. An
    # absence check that is itself case-sensitive proves nothing about a
    # case-insensitive rule.
    uitvoer = (r.stdout + r.stderr).lower()
    expect("  and the term does not appear in stdout or stderr, in any casing",
           TREFFER.lower() not in uitvoer, uitvoer[:250])
    expect("  while the run still fails, so nobody has to read the log to know",
           r.returncode == 1, f"exit={r.returncode}")

MINIMUM_CASES = 16  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
