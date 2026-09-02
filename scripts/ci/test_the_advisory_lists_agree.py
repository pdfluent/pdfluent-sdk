#!/usr/bin/env python3
"""The advisory-list comparison, shown to bite (#294).

Written alongside the guard rather than after it, because a guard whose biting
is asserted in a commit message is the shape this repository spent 02-09-2026
removing -- twice in guards I wrote myself.

The cases build both files in a temporary tree, so a failure here is about the
comparison and not about today's acceptances.
"""
from __future__ import annotations
import pathlib, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "the_advisory_lists_agree.py"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def run(audit: list[str] | None, deny: list[str] | None) -> subprocess.CompletedProcess:
    """A tree with the two files, then the real guard pointed at it.

    `None` leaves the file out entirely, which is the "cannot compare" case.
    """
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td) / "repo"
        (root / "scripts" / "ci").mkdir(parents=True)
        (root / ".cargo").mkdir()
        shutil_copy = (root / "scripts" / "ci" / GUARD.name)
        shutil_copy.write_text(GUARD.read_text())
        if audit is not None:
            (root / ".cargo" / "audit.toml").write_text(
                "[advisories]\nignore = [" + ", ".join(f'"{x}"' for x in audit) + "]\n")
        if deny is not None:
            (root / "deny.toml").write_text(
                "[advisories]\nignore = [" + ", ".join(f'"{x}"' for x in deny) + "]\n")
        return subprocess.run([sys.executable, str(shutil_copy)],
                              capture_output=True, text=True)


print("the two advisory lists agree")

r = run(["RUSTSEC-2023-0071"], ["RUSTSEC-2023-0071"])
expect("identical lists pass", r.returncode == 0, f"exit={r.returncode} {r.stderr[:160]}")

r = run(["RUSTSEC-2023-0071"], [])
expect("accepted in audit.toml only FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and names the advisory", "RUSTSEC-2023-0071" in r.stderr, r.stderr[:200])
expect("  and says which tool will fail on it",
       "cargo-deny will fail" in r.stderr, r.stderr[:250])

r = run([], ["RUSTSEC-2026-0104"])
expect("accepted in deny.toml only FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and says the other tool will fail",
       "cargo-audit will fail" in r.stderr, r.stderr[:250])

r = run([], [])
expect("two empty lists agree", r.returncode == 0, f"exit={r.returncode}")

r = run(["RUSTSEC-2023-0071"], None)
expect("a missing file is FATAL, not agreement", r.returncode == 2, f"exit={r.returncode}")
expect("  and says so in those words", "not the same as agreeing" in r.stderr, r.stderr[:250])

# Order must not matter: these are sets, and a reordering is not a change.
r = run(["A", "B"], ["B", "A"])
expect("the same ids in a different order agree", r.returncode == 0, f"exit={r.returncode}")

MINIMUM_CASES = 10  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
