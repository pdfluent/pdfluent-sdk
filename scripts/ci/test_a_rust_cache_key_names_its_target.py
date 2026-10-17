#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Drives a_rust_cache_key_names_its_target.py. (#636)

The guard has one rule -- a rust-cache step in a target-matrix job keys per
target -- and four ways to misread a workflow while staying green:

  it only reads one matrix spelling       -- `include:` legs or `target: [..]`,
                                             and the other shape keeps sharing
  it accepts any key:                      -- a static `key: build-cache` still
                                             restores the same target/ per leg
  it accepts a reference to the wrong name -- `matrix.target` on a job whose
                                             matrix varies `triple` renders the
                                             same string on every leg
  it judges jobs with no target matrix     -- and then it is red about the
                                             twenty-five cache steps that
                                             correctly keep the default key

Each of those is a case. The last case is the repository itself: the two
steps the incident was about key per target, and the rest keep the default.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "a_rust_cache_key_names_its_target.py"

failures: list[str] = []
done = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global done
    done += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        failures.append(what)


def job(name: str, matrix: str, cache_with: str) -> str:
    out = [f"  {name}:", "    runs-on: ubuntu-latest"]
    if matrix:
        out.append("    strategy:")
        out.append("      fail-fast: false")
        out.append("      matrix:")
        out.extend(f"      {line}" if line else line for line in matrix.splitlines())
    out.append("    steps:")
    out.append("      - uses: Swatinem/rust-cache@v2")
    if cache_with:
        out.append("        with:")
        out.extend(f"          {line}" if line else line for line in cache_with.splitlines())
    return "\n".join(out) + "\n"


INCLUDE_TARGET = "  include:\n    - target: x86_64-unknown-linux-gnu\n    - target: aarch64-unknown-linux-musl\n"
INCLUDE_TRIPLE = "  include:\n    - triple: x86_64-unknown-linux-gnu\n    - triple: aarch64-unknown-linux-musl\n"
INCLUDE_OS = "  include:\n    - os: ubuntu-latest\n    - os: macos-latest\n"
LIST_TARGET = "  target: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-musl]\n"

KEY_TARGET = "key: build-${{ matrix.target }}"
KEY_TRIPLE = "key: natives-${{ matrix.triple }}"
KEY_STATIC = "key: build-cache"
KEY_BRACKET = "key: build-${{ matrix['target'] }}"


def pad(jobs: int = 52, caches: int = 12) -> str:
    """Filler jobs so the fixture passes the guard's floors honestly.

    The guard refuses a scan that found almost nothing (fewer than 50 jobs
    or 10 cache steps); the padding is ordinary jobs, most without a cache
    step and none with a target matrix, so it counts towards the floors and
    towards no verdict.
    """
    body = ["name: pad", "on: push", "jobs:"]
    for n in range(jobs):
        body.append(f"  filler-{n}:")
        body.append("    runs-on: ubuntu-latest")
        body.append("    steps:")
        if n < caches:
            body.append("      - uses: Swatinem/rust-cache@v2")
        else:
            body.append("      - run: echo filler")
    return "\n".join(body) + "\n"


def run(files: dict[str, str]) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as temporary:
        fake = pathlib.Path(temporary)
        (fake / ".github" / "workflows").mkdir(parents=True)
        (fake / ".github" / "workflows" / "pad.yml").write_text(pad())
        for name, content in files.items():
            (fake / ".github" / "workflows" / f"{name}.yml").write_text(content)
        return subprocess.run([sys.executable, str(GUARD), "--repo", str(fake)],
                              capture_output=True, text=True, timeout=120)


def workflow(*jobs: str) -> str:
    return "name: fixture\non: push\njobs:\n" + "".join(jobs)


print("a target-matrix job that keys per target passes")
good = run({"one": workflow(job("build", INCLUDE_TARGET, KEY_TARGET))})
expect("include-form matrix with matrix.target passes", good.returncode == 0, good.stdout[-300:])
triple = run({"one": workflow(job("natives", INCLUDE_TRIPLE, KEY_TRIPLE))})
expect("a triple matrix keys per matrix.triple", triple.returncode == 0, triple.stdout[-300:])
listed = run({"one": workflow(job("build", LIST_TARGET, KEY_TARGET))})
expect("the list-form matrix counts as a target matrix", listed.returncode == 0, listed.stdout[-300:])
bracket = run({"one": workflow(job("build", INCLUDE_TARGET, KEY_BRACKET))})
expect("the matrix['target'] spelling counts", bracket.returncode == 0, bracket.stdout[-300:])

print("and one that does not is refused")
missing = run({"one": workflow(job("build", INCLUDE_TARGET, ""))})
expect("no key: is refused", missing.returncode == 1, missing.stdout[-300:])
expect("and the job is named", "`build`" in missing.stdout, missing.stdout[-300:])
static = run({"one": workflow(job("build", INCLUDE_TARGET, KEY_STATIC))})
expect("a static key is refused", static.returncode == 1, static.stdout[-300:])
wrong = run({"one": workflow(job("natives", INCLUDE_TRIPLE, KEY_TARGET))})
expect("matrix.target on a triple matrix is refused", wrong.returncode == 1, wrong.stdout[-300:])
listed_bad = run({"one": workflow(job("build", LIST_TARGET, ""))})
expect("the list form without a key is refused", listed_bad.returncode == 1, listed_bad.stdout[-300:])

print("jobs with no target matrix keep the default key")
plain = run({"one": workflow(job("smoke", "", ""))})
expect("no matrix at all passes", plain.returncode == 0, plain.stdout[-300:])
other = run({"one": workflow(job("test", INCLUDE_OS, ""))})
expect("an os matrix passes", other.returncode == 0, other.stdout[-300:])

print("a scan that finds nothing approves nothing")
with tempfile.TemporaryDirectory() as temporary:
    fake = pathlib.Path(temporary)
    (fake / ".github" / "workflows").mkdir(parents=True)
    (fake / ".github" / "workflows" / "one.yml").write_text(
        workflow(job("build", INCLUDE_TARGET, KEY_TARGET)))
    thin = subprocess.run([sys.executable, str(GUARD), "--repo", str(fake)],
                          capture_output=True, text=True, timeout=60)
expect("one job is too few to be the real set", thin.returncode == 2, thin.stdout[-200:])
expect("and it says so", "approves everything" in thin.stdout, thin.stdout[-200:])
none = subprocess.run([sys.executable, str(GUARD), "--repo", "/nonexistent-tree"],
                      capture_output=True, text=True)
expect("no workflow directory is a refusal", none.returncode == 2, none.stdout[-200:])
broken = run({"one": "name: broken\non: push\njobs: [unclosed\n"})
expect("an unparseable workflow is a refusal", broken.returncode == 1, broken.stdout[-300:])

print("and this repository obeys the rule")
real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True, timeout=120)
expect("every target-matrix job here keys per target", real.returncode == 0, real.stdout[-800:])

print(f"test_a_rust_cache_key_names_its_target: {done} case(s), {len(failures)} failure(s)")
sys.exit(1 if failures else 0)
