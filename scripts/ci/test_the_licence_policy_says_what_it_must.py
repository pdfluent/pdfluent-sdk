#!/usr/bin/env python3
"""Two-way proof for the_licence_policy_says_what_it_must.py (#300).

A guard that only ever passes is indistinguishable from no guard. Each case
here MUTATES a copy of the real policy and demands the verdict flip. The point
is not that the guard is green today -- it is that it goes red when the thing it
protects is removed, which is the property the licence gates did not have and
the reason #300 exists.
"""
from __future__ import annotations
import pathlib, re, shutil, subprocess, sys, tempfile, tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/the_licence_policy_says_what_it_must.py"
POLICY_REL = "docs/LICENSE_POLICY.toml"

fails: list[str] = []
ran = 0


sys.path.insert(0, str(REPO / "scripts" / "ci"))
import the_licence_policy_says_what_it_must as guard  # noqa: E402


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def set_list(text: str, key: str, values: list[str]) -> str:
    """Replace one multi-line array in the REAL policy text.

    The test mutates the real file rather than re-emitting the policy from a
    parsed copy: a hand-rolled TOML writer drops what it does not know about,
    and then the test measures the writer instead of the guard. That mistake
    cost the first run of this suite -- every case failed with FATAL because the
    re-emitted file no longer parsed, which looked exactly like a broken guard.
    """
    items = ", ".join(f'"{v}"' for v in values)
    pat = re.compile(rf"^{re.escape(key)} = \[.*?\]", re.S | re.M)
    out, n = pat.subn(f"{key} = [{items}]", text, count=1)
    assert n == 1, f"{key} not found in the policy text"
    return out


def read_list(text: str, key: str) -> list[str]:
    return tomllib.loads(text)["licenses"][key]


def run_with(mutate) -> subprocess.CompletedProcess:
    """Run the real guard against a mutated copy of the real tree.

    `mutate` takes the policy TEXT and returns new text; None means unchanged.
    """
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td) / "repo"
        (root / "scripts" / "ci").mkdir(parents=True)
        (root / "docs").mkdir(parents=True)
        for name in (pathlib.Path(GUARD).name, "license_gate.py"):
            shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
        text = (REPO / POLICY_REL).read_text()
        text = mutate(text) if mutate else text
        (root / POLICY_REL).write_text(text)
        return subprocess.run([sys.executable, str(root / GUARD)],
                              capture_output=True, text=True)


def without(key: str, *drop: str):
    def f(text: str) -> str:
        return set_list(text, key, [x for x in read_list(text, key) if x not in drop])
    return f


print("the licence policy says what it must — two-way")

r = run_with(None)
expect("the real policy passes unmutated", r.returncode == 0, r.stderr[-200:])

# The exact mutation #300 was opened about: measured on master, removing these
# left all three licence gates green.
r = run_with(without("forbidden", "AGPL-3.0-only", "AGPL-3.0-or-later"))
expect("removing both AGPL-3.0 spellings FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and names both", "AGPL-3.0-only" in r.stderr and "AGPL-3.0-or-later" in r.stderr)
expect("  and gives the reason, not just the name", "flip" in r.stderr.lower())

for one in ("GPL-2.0-only", "GPL-3.0-only", "SSPL-1.0"):
    r = run_with(without("forbidden", one))
    expect(f"removing {one} FAILS", r.returncode == 1, f"exit={r.returncode}")

r = run_with(without("allowed", "MIT"))
expect("removing MIT from allowed FAILS", r.returncode == 1, f"exit={r.returncode}")

r = run_with(lambda t: set_list(t, "forbidden", []))
expect("emptying the forbidden list FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and says it was emptied, not curated", "emptied" in r.stderr)

# The permitting list is an attack surface too: `allowed` is tested before
# `weak_copyleft`, so a weak licence there is acceptable even where the weak set
# is empty. (codex, #1656)
r = run_with(lambda t: set_list(t, "allowed", ["MPL-2.0"] + read_list(t, "allowed")))
expect("a weak-copyleft licence in allowed FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and says the per-surface set is skipped",
       "deliberately empty" in r.stderr, r.stderr[-200:])

r = run_with(lambda t: set_list(t, "forbidden", ["MPL-2.0"] + read_list(t, "forbidden")))
expect("a weak-copyleft licence in forbidden FAILS", r.returncode == 1,
       f"exit={r.returncode}")

r = run_with(lambda t: set_list(t, "forbidden", ["MIT"] + read_list(t, "forbidden")))
expect("a licence in BOTH lists FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and says which way the evaluator reads it", "forbidden first" in r.stderr)

# A list the code does not honour is a comment. Prove the guard notices by
# breaking the evaluator rather than the list.
def blind_evaluator(root: pathlib.Path) -> None:
    g = root / "scripts" / "ci" / "license_gate.py"
    # The mutation has to make the evaluator ACCEPT a forbidden licence. The
    # first version of this case blinded the check with `if False:` -- and the
    # verdict stayed correct, because a forbidden licence is not in `allowed`
    # either, so it fell through to "on no list" and was still refused. It
    # passed for the wrong reason and proved nothing about the list being
    # honoured. A mutation that does not change the answer is not a test.
    s = g.read_text().replace('if x in verboden:\n            return False, f"{x} is forbidden"',
                              'if x in verboden:\n            return True, ""', 1)
    assert s != g.read_text(), "the mutation did not apply; the case would pass vacuously"
    g.write_text(s)


with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)
    for name in (pathlib.Path(GUARD).name, "license_gate.py"):
        shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
    shutil.copy(REPO / POLICY_REL, root / POLICY_REL)
    blind_evaluator(root)
    r = subprocess.run([sys.executable, str(root / GUARD)],
                       capture_output=True, text=True)
expect("an evaluator that stops honouring the list FAILS",
       r.returncode == 1, f"exit={r.returncode}")

# The other way an evaluator stops honouring it: DELETE the forbidden branch.
# Every forbidden licence then falls through to "not classified" -- also False,
# so a check that only asked "is it refused" saw agreement. False is the absence
# of a verdict, not a verdict. (codex, #1656)
with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)
    for name in (pathlib.Path(GUARD).name, "license_gate.py"):
        shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
    shutil.copy(REPO / POLICY_REL, root / POLICY_REL)
    g = root / "scripts" / "ci" / "license_gate.py"
    before = g.read_text()
    g.write_text(before.replace(
        '        if x in verboden:\n            return False, f"{x} is forbidden"\n', "", 1))
    assert g.read_text() != before, "the mutation did not apply"
    r = subprocess.run([sys.executable, str(root / GUARD)], capture_output=True, text=True)
expect("deleting the forbidden branch FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and says it is refused for the wrong reason",
       "wrong reason" in r.stderr, r.stderr[-200:])
expect("  and does not crash", "Traceback" not in r.stderr)

# Every spelling of the family, moved one at a time. The guard used to name
# six of the ten, so `GPL-2.0-or-later` and `GPL-3.0-or-later` could be moved
# to `allowed` with every licence gate green (measured, #1656). A per-name
# check would have needed somebody to think of each name -- which is the thing
# that failed -- so the case list is generated from the same family the guard
# generates its requirement from, and a name added to one appears in the other.
def verplaats(naam: str):
    def f(text: str) -> str:
        rest = [x for x in read_list(text, "forbidden") if x != naam]
        text = set_list(text, "forbidden", rest)
        return set_list(text, "allowed", read_list(text, "allowed") + [naam])
    return f


for naam in sorted(guard.MOET_GECLASSIFICEERD):
    r = run_with(verplaats(naam))
    expect(f"{naam} moved to allowed is refused",
           r.returncode == 1 and naam in r.stderr, f"exit={r.returncode}")

# The weak set is the other door into `allowed`, and it is the one that reads
# as harmless: LGPL is classified-and-refused everywhere by sitting in
# weak_copyleft with no surface naming it, so promoting it to `allowed` skips
# the per-surface decision entirely rather than overriding it.
r = run_with(lambda t: set_list(t, "allowed", read_list(t, "allowed") + ["LGPL-3.0-or-later"]))
expect("LGPL promoted to allowed is refused",
       r.returncode == 1 and "LGPL-3.0-or-later" in r.stderr, f"exit={r.returncode}")

# An exception is a decision, not a word. Nothing may reach `allowed` on the
# strength of "WITH" appearing in it while UITZONDERINGEN is empty.
r = run_with(lambda t: set_list(t, "allowed",
                                read_list(t, "allowed") + ["GPL-3.0-only WITH Autoconf-exception-3.0"]))
expect("an unnamed WITH-exception does not admit strong copyleft",
       r.returncode == 1, f"exit={r.returncode}")

# The second door. `allowed` was the only list checked for the family, so
# `weak_copyleft` -- and any surface's own weak set -- let it back in with all
# five licence guards green. license_gate builds the cargo weak set as the
# UNION of every surface, so naming it on `editor` alone reaches all 786 cargo
# packages. Measured before the fix; these are the three doors it opened.
def naar_zwak(naam: str):
    def f(text: str) -> str:
        rest = [x for x in read_list(text, "forbidden") if x != naam]
        text = set_list(text, "forbidden", rest)
        return set_list(text, "weak_copyleft", read_list(text, "weak_copyleft") + [naam])
    return f


for naam in ("AGPL-3.0", "GPL-2.0-or-later"):
    r = run_with(naar_zwak(naam))
    expect(f"{naam} moved to weak_copyleft is refused",
           r.returncode == 1 and naam in r.stderr, f"exit={r.returncode}")

r = run_with(lambda t: t.replace('editor = { weak_copyleft = ["MPL-2.0"]',
                                 'editor = { weak_copyleft = ["AGPL-3.0", "MPL-2.0"]', 1))
expect("a surface's own weak set is checked too",
       r.returncode == 1 and "surfaces.editor" in r.stderr, r.stderr[-200:])

# The anchor. A licence field is an EXPRESSION, and `^` saw neither operand of
# a disjunction nor a family name inside a LicenseRef.
for naam in ("MIT OR GPL-3.0-only", "LicenseRef-AGPL-3.0"):
    r = run_with(lambda t, n=naam: set_list(t, "allowed", read_list(t, "allowed") + [n]))
    expect(f"{naam!r} in allowed is refused", r.returncode == 1, f"exit={r.returncode}")

# And the lookbehind that keeps LGPL out of it: `LGPL-2.1-only` contains
# `GPL-2`, and calling file-level copyleft strong would contradict the
# deliberate decision to classify it per surface.
expect("LGPL is not read as strong copyleft",
       not guard.STERK_COPYLEFT.search("LGPL-2.1-only")
       and bool(guard.BESTANDS_COPYLEFT.search("LGPL-2.1-only")))
expect("an exception-bearing permissive licence is not read as strong",
       not guard.STERK_COPYLEFT.search("Apache-2.0 WITH LLVM-exception"))

# The floor now counts what the generated family does NOT: delete the
# hand-curated entries and it must fail, which the old whole-list floor of 10
# could not do while twenty family entries held it up.
r = run_with(lambda t: set_list(t, "forbidden",
                                [x for x in read_list(t, "forbidden")
                                 if x in guard.MOET_GECLASSIFICEERD]))
expect("deleting every non-family forbidden entry fails", r.returncode == 1,
       f"exit={r.returncode}")

# A guard that cannot read its input must not report a clean policy.
with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)
    for name in (pathlib.Path(GUARD).name, "license_gate.py"):
        shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
    r = subprocess.run([sys.executable, str(root / GUARD)],
                       capture_output=True, text=True)
expect("a missing policy is FATAL, not a pass", r.returncode == 2, f"exit={r.returncode}")

# Set to what actually runs, and actually compared. Declared-but-never-read is
# how the same floor failed in #1641: len(fails) counts only failures, so a
# deleted case left the suite green while it shrank, and a floor below the real
# count tolerated the shrinkage it existed to catch.
MINIMUM_CASES = 50  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
if fails:
    for f in fails:
        print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}. Cases have "
          "gone missing; a smaller suite passing is not the same as this suite "
          "passing.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
