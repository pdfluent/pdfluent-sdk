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
MINIMUM_CASES = 20  # FLOOR
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
