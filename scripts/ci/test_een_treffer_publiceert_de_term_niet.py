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

TERM = "Voorbeeldpartner"
CONTEXT = f"...werk voor {TERM} afgerond..."


def toonbaar(ci: bool):
    oud = os.environ.get("CI")
    if ci:
        os.environ["CI"] = "true"
    else:
        os.environ.pop("CI", None)
    try:
        return giz._toonbaar("partner", TERM, CONTEXT)
    finally:
        if oud is None:
            os.environ.pop("CI", None)
        else:
            os.environ["CI"] = oud


print("a hit must not publish the term")

wat, ctx = toonbaar(ci=True)
expect("under CI the term is not printed", TERM not in wat and TERM not in ctx,
       f"{wat!r} {ctx!r}")
expect("  nor is its context", CONTEXT not in ctx, ctx)
expect("  and what is printed identifies it", "partner term" in wat and str(len(TERM)) in wat, wat)
expect("  by a digest, so two different terms differ",
       giz._toonbaar("partner", "AndereNaam", CONTEXT)[0] != wat)

wat, ctx = toonbaar(ci=False)
expect("locally the term IS printed", wat == TERM and ctx == CONTEXT, f"{wat!r}")

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
    expect("  and the term does not appear in the output",
           TREFFER not in r.stderr, r.stderr[:250])
    expect("  while the run still fails, so nobody has to read the log to know",
           r.returncode == 1, f"exit={r.returncode}")

MINIMUM_CASES = 9  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
