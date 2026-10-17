#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A failing register generator fails the run, loudly.

On 11-10 a sandbox refused the sccache wrapper, `capability_register.py` failed
inside `regenerate_registers.sh` through `cargo metadata`, and the run still
ended exit 0: no diffs, nothing to commit, a stale register found only later on
a merged tree. A generator that cannot run is not a file that is current, and a
generator that exits 0 without writing its file is not a file that was written.

So every generator step is judged twice: its exit status, and -- in write mode
-- whether its file was actually produced. Any failure ends the run non-zero
with `[registers] GENERATION FAILED: <generator>: <cause>` on stderr, carrying
the generator's own last lines. The success text and the stale-file message
stay exactly as they were; the last two cases pin them.

The generators here are stubs behind the script's test seam: writers that
write, checkers that pass, and failures that fail. Nothing runs `cargo`,
nothing touches the real registers, and every case is milliseconds.

Red without the fix -- cases 1, 2, 3, 4 and 6 fail against the old script,
which either exits 0 over the silent stub or reports without the line -- and
green with it.

Exit codes:
    0  every case holds
    1  a case failed
"""
from __future__ import annotations

import os
import re
import shlex
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

CI = Path(__file__).resolve().parent
SCRIPT = CI / "regenerate_registers.sh"

FAILED = "[registers] GENERATION FAILED:"
OK_TEXT = re.compile(
    r"\[registers\] OK: \d+ generated register\(s\) and \d+ copies are current"
)


def write_shim(path: Path, body: str) -> None:
    """An executable `#!/bin/sh` stub."""
    path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def run_script(args: list[str], rows: str) -> subprocess.CompletedProcess[str]:
    """The real script with stub rows and no copies: hermetic, no cargo."""
    env = dict(
        os.environ,
        REGENERATE_REGISTERS_ROWS=rows,
        REGENERATE_REGISTERS_COPIES="",
    )
    return subprocess.run(
        ["bash", str(SCRIPT), *args],
        capture_output=True, text=True, env=env, timeout=120,
    )


def row(out: Path, maker: str, checker: str) -> str:
    return f"{out}|{maker}|{checker}"


def case_a_failing_generator_fails_the_write() -> tuple[bool, str]:
    """Exit 1 and a missing tool both end the write non-zero, each named with
    GENERATION FAILED on stderr, the stub's own stderr carried through."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        fail = t / "fail.sh"
        write_shim(fail, 'echo "stub-generator: boom" >&2\nexit 1\n')
        missing = t / "not-there"  # never created: deterministically missing
        rows = "\n".join([
            row(t / "out_a.txt", str(fail), "true"),
            row(t / "out_b.txt", f"{missing} --write", "true"),
        ])
        r = run_script([], rows)
    ok = (
        r.returncode != 0
        and r.stderr.count(FAILED) == 2
        and "boom" in r.stderr
        and "(command not found)" in r.stderr
    )
    return (ok, f"exit={r.returncode} stderr={r.stderr.strip()[:400]!r}")


def case_a_silent_generator_is_a_failure() -> tuple[bool, str]:
    """The 11-10 shape: the generator exits 0 but produces nothing -- an
    untouched file, a missing file, an empty file. Each is a failure with its
    own cause, never a write receipt."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        silent = t / "silent.sh"
        write_shim(silent, "exit 0\n")
        untouched = t / "untouched.txt"
        untouched.write_text("stale bytes\n", encoding="utf-8")
        empty = t / "empty.txt"
        empty.write_text("", encoding="utf-8")
        rows = "\n".join([
            row(untouched, str(silent), "true"),
            row(t / "missing.txt", str(silent), "true"),
            row(empty, str(silent), "true"),
        ])
        r = run_script([], rows)
    ok = (
        r.returncode != 0
        and r.stderr.count(FAILED) == 3
        and "was not rewritten" in r.stderr
        and "was not written" in r.stderr
        and "is empty" in r.stderr
        and "  wrote " not in r.stdout
    )
    return (ok, f"exit={r.returncode} out={r.stdout.strip()[:200]!r} "
                f"err={r.stderr.strip()[:400]!r}")


def case_succeeding_stubs_keep_the_write_path() -> tuple[bool, str]:
    """Two stubs that write their files -- one rewriting, one creating: exit 0
    and the success text exactly as before, with no GENERATION FAILED."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        out_a = t / "out_a.txt"
        out_b = t / "out_b.txt"
        out_a.write_text("stale bytes\n", encoding="utf-8")
        gen_a = t / "gen_a.sh"
        gen_b = t / "gen_b.sh"
        write_shim(gen_a, f"printf 'stub register a\\n' > {shlex.quote(str(out_a))}\n")
        write_shim(gen_b, f"printf 'stub register b\\n' > {shlex.quote(str(out_b))}\n")
        rows = "\n".join([
            row(out_a, f"sh {gen_a}", "true"),
            row(out_b, f"sh {gen_b}", "true"),
        ])
        r = run_script([], rows)
        # Existence first: against the old script the seam is ignored, the
        # stubs never run, and out_b is not there at all. That is a case that
        # fails, not a runner that crashes.
        wrote_a = out_a.is_file() and out_a.read_text(encoding="utf-8") == "stub register a\n"
        wrote_b = out_b.is_file() and out_b.read_text(encoding="utf-8") == "stub register b\n"
    ok = (
        r.returncode == 0
        and f"wrote {out_a}" in r.stdout
        and f"wrote {out_b}" in r.stdout
        and "[registers] wrote 2 register(s) and 0 copies. "
            "Commit what changed." in r.stdout
        and FAILED not in r.stderr
        and FAILED not in r.stdout
        and wrote_a
        and wrote_b
    )
    return (ok, f"exit={r.returncode} out={r.stdout.strip()[:400]!r} "
                f"err={r.stderr.strip()[:200]!r}")


def case_a_failing_check_fails_loudly() -> tuple[bool, str]:
    """A checker that dies like `cargo metadata` did -- exit 2 -- and a missing
    tool: non-zero with GENERATION FAILED, never OK or current."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        fatal = t / "fatal.sh"
        write_shim(fatal, 'echo "stub-checker: cargo metadata failed: '
                          'sccache wrapper refused" >&2\nexit 2\n')
        missing = t / "not-there"  # never created: deterministically missing
        rows = "\n".join([
            row(t / "out_a.txt", "true", f"{fatal} --check"),
            row(t / "out_b.txt", "true", f"{missing} --check"),
        ])
        r = run_script(["--check"], rows)
    ok = (
        r.returncode != 0
        and r.stderr.count(FAILED) == 2
        and "sccache wrapper refused" in r.stderr
        and "(command not found)" in r.stderr
        and "are current" not in r.stdout
        and "[registers] OK:" not in r.stdout
    )
    return (ok, f"exit={r.returncode} out={r.stdout.strip()[:200]!r} "
                f"err={r.stderr.strip()[:400]!r}")


def case_a_passing_check_keeps_the_ok_text() -> tuple[bool, str]:
    """A checker that passes: exit 0 and the OK text, with no GENERATION
    FAILED. The counts stay a regex: rows come and go, the sentence stays."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        passed = t / "pass.sh"
        write_shim(passed, "exit 0\n")
        rows = row(t / "out.txt", "true", str(passed))
        r = run_script(["--check"], rows)
    ok = (
        r.returncode == 0
        and OK_TEXT.search(r.stdout) is not None
        and FAILED not in r.stderr
        and FAILED not in r.stdout
    )
    return (ok, f"exit={r.returncode} out={r.stdout.strip()[:300]!r} "
                f"err={r.stderr.strip()[:200]!r}")


def case_a_stale_check_keeps_the_stale_message() -> tuple[bool, str]:
    """A checker reporting stale -- exit 1 -- is not a generation failure: exit
    1 with the ALL-of-them message and the fixing command, and no GENERATION
    FAILED anywhere."""
    with tempfile.TemporaryDirectory(prefix="regen-loud-") as tmp:
        t = Path(tmp)
        stale = t / "stale.sh"
        write_shim(stale, 'echo "[stub] FAIL: the register no longer matches"\nexit 1\n')
        rows = row(t / "out.txt", "true", f"{stale} --check")
        r = run_script(["--check"], rows)
    ok = (
        r.returncode == 1
        and "ALL of them" in r.stdout
        and "bash scripts/ci/regenerate_registers.sh" in r.stdout
        and FAILED not in r.stderr
        and FAILED not in r.stdout
    )
    return (ok, f"exit={r.returncode} out={r.stdout.strip()[:400]!r} "
                f"err={r.stderr.strip()[:200]!r}")


CASES = [
    case_a_failing_generator_fails_the_write,
    case_a_silent_generator_is_a_failure,
    case_succeeding_stubs_keep_the_write_path,
    case_a_failing_check_fails_loudly,
    case_a_passing_check_keeps_the_ok_text,
    case_a_stale_check_keeps_the_stale_message,
]

# A runner that runs nothing reports green. Six cases, counted, so one lost to
# a rename is noticed rather than approved.
EXPECTED_CASES = 6


def main() -> int:
    if len(CASES) != EXPECTED_CASES:
        print(f"[registers-loud] FAIL: {len(CASES)} cases, "
              f"expected {EXPECTED_CASES}", file=sys.stderr)
        return 1
    failed = []
    for case in CASES:
        ok, detail = case()
        print(f"  {'ok  ' if ok else 'FAIL'}  {case.__name__}")
        if not ok:
            failed.append((case.__name__, detail))
    if failed:
        print("\n[registers-loud] a failing generator is not failing loudly:\n",
              file=sys.stderr)
        for name, detail in failed:
            print(f"  - {name}\n      got: {detail}", file=sys.stderr)
        return 1
    print(f"[registers-loud] {len(CASES)} cases, all good")
    return 0


if __name__ == "__main__":
    sys.exit(main())
