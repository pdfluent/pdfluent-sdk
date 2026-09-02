#!/usr/bin/env python3
"""The hollow-job guard, shown to bite (#292).

`every_job_does_something.py` was written because `measurement-guard` spent two
days as one `actions/checkout` and nothing else, reporting "The published
measurements still measure what they say" as a green tick. The guard caught
that in a scratch run -- and nothing in the pipeline said so, which is the
shape it exists to remove, one level up.

The cases are built as workflow files in a temporary directory rather than by
editing the real ones, so a failure here is about the guard and not about
today's ci.yml.
"""
from __future__ import annotations
import pathlib, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "every_job_does_something.py"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def run(inhoud: dict[str, str]) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as td:
        wf = pathlib.Path(td) / "workflows"
        wf.mkdir()
        for naam, tekst in inhoud.items():
            (wf / naam).write_text(tekst)
        return subprocess.run([sys.executable, str(GUARD), "--workflows", str(wf)],
                              capture_output=True, text=True)


HOL = """
on: [push]
jobs:
  hollow-guard:
    name: The measurements still measure what they say
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
"""

DOET_IETS = """
on: [push]
jobs:
  real-guard:
    name: It runs something
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: python3 scripts/ci/something.py
"""

HERBRUIK = """
on: [push]
jobs:
  delegating:
    uses: ./.github/workflows/other.yml
"""

ANDERE_ACTIE = """
on: [push]
jobs:
  uploader:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/upload-artifact@v4
        with: {name: x, path: y}
"""

print("a job that runs nothing can only be green")

r = run({"hol.yml": HOL})
expect("a job with only checkout and setup FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and it names the job", "hollow-guard" in r.stderr, r.stderr[:200])
expect("  and quotes the promise in its title",
       "still measure what they say" in r.stderr, r.stderr[:250])

r = run({"echt.yml": DOET_IETS})
expect("a job with a run: step passes", r.returncode == 0, f"exit={r.returncode} {r.stderr[:160]}")

r = run({"herbruik.yml": HERBRUIK})
expect("a job that only calls a reusable workflow passes", r.returncode == 0,
       f"exit={r.returncode} {r.stderr[:160]}")

r = run({"upload.yml": ANDERE_ACTIE})
expect("an action that is not checkout or setup counts as doing something",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[:160]}")

# A scan that reads nothing must not report agreement -- the floor this
# repository keeps rediscovering.
r = run({})
expect("an empty workflow directory is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode}")

# Both together, so one hollow job among healthy ones is still found.
r = run({"hol.yml": HOL, "echt.yml": DOET_IETS})
expect("one hollow job among healthy ones is still found", r.returncode == 1,
       f"exit={r.returncode}")

MINIMUM_CASES = 8  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
