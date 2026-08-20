#!/usr/bin/env python3
"""Tests for the veraPDF verdict cache.

Run: python3 scripts/ci/test_verapdf_cached.py

No real veraPDF and no network. The stand-in records every invocation, which is
how the important test works at all: proving a cache HIT means proving the real
binary was not called, and only a counter can show that.

A wrong PASS is the worst thing this cache could produce, so most of these check
that it declines to answer rather than that it answers quickly.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
WRAPPER = HERE / "verapdf_cached.py"

FAKE = r'''#!/usr/bin/env python3
import sys, pathlib
calls = pathlib.Path(__import__("os").environ["FAKE_CALLS"])
calls.write_text(str(int(calls.read_text() or 0) + 1) if calls.exists() else "1")
if "--version" in sys.argv:
    print(__import__("os").environ.get("FAKE_VERSION", "veraPDF 1.28.2")); sys.exit(0)
target = [a for a in sys.argv[1:] if not a.startswith("-")]
print(f"PASS {target[-1] if target else '?'} 2b")
sys.exit(0)
'''

passed = failed = 0


def check(name: str, cond: bool, detail: str = "") -> None:
    global passed, failed
    if cond:
        passed += 1
        print(f"  ok   {name}")
    else:
        failed += 1
        print(f"  FAIL {name}  {detail}")


class Env:
    def __init__(self, tmp: Path, version: str = "veraPDF 1.28.2", enabled: bool = True):
        self.tmp = tmp
        self.fake = tmp / "fake_verapdf.py"
        self.fake.write_text(FAKE)
        self.fake.chmod(0o755)
        self.calls = tmp / "calls.txt"
        self.cache = tmp / "cache"
        self.version = version
        self.enabled = enabled

    def env(self) -> dict:
        e = dict(os.environ)
        e.update({
            "VERAPDF_REAL": str(self.fake),
            "VERAPDF_CACHE_DIR": str(self.cache),
            "FAKE_CALLS": str(self.calls),
            "FAKE_VERSION": self.version,
            "VERAPDF_CACHE": "on" if self.enabled else "off",
        })
        return e

    def run(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run([sys.executable, str(WRAPPER), *args],
                              capture_output=True, text=True, env=self.env())

    def call_count(self) -> int:
        return int(self.calls.read_text()) if self.calls.exists() else 0

    def reset_calls(self) -> None:
        if self.calls.exists():
            self.calls.unlink()


def main() -> int:
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        pdf = tmp / "doc.pdf"
        pdf.write_bytes(b"%PDF-1.7\nfake content A\n%%EOF\n")

        # --- a miss measures, and stores -------------------------------------
        e = Env(tmp)
        r1 = e.run("--format", "json", "--flavour", "0", str(pdf))
        check("miss calls the real binary", e.call_count() >= 1, f"calls={e.call_count()}")
        check("miss returns veraPDF's own output", "PASS" in r1.stdout, r1.stdout[:60])

        # --- a hit does NOT measure ------------------------------------------
        version_calls = e.call_count()
        e.reset_calls()
        r2 = e.run("--format", "json", "--flavour", "0", str(pdf))
        # --version is still consulted; the validation itself must not be.
        check("hit skips validation", e.call_count() < version_calls,
              f"calls={e.call_count()} vs {version_calls}")
        check("hit reproduces the verdict", r2.stdout == r1.stdout,
              f"{r1.stdout!r} != {r2.stdout!r}")

        # --- identical bytes elsewhere still hit, with the new path -----------
        other = tmp / "elsewhere" / "renamed.pdf"
        other.parent.mkdir()
        other.write_bytes(pdf.read_bytes())
        r3 = e.run("--format", "json", "--flavour", "0", str(other))
        check("identical bytes at another path hit", "PASS" in r3.stdout)
        check("the replayed output names the file it was given",
              str(other) in r3.stdout and str(pdf) not in r3.stdout, r3.stdout[:90])

        # --- different bytes must miss ---------------------------------------
        changed = tmp / "changed.pdf"
        changed.write_bytes(b"%PDF-1.7\nfake content B\n%%EOF\n")
        e.reset_calls()
        e.run("--format", "json", "--flavour", "0", str(changed))
        check("changed bytes are validated again", e.call_count() >= 2,
              f"calls={e.call_count()}")

        # --- a new veraPDF must not inherit old verdicts ----------------------
        e2 = Env(tmp, version="veraPDF 1.29.0")
        e2.cache = e.cache  # same store, newer validator
        e2.reset_calls()
        e2.run("--format", "json", "--flavour", "0", str(pdf))
        check("a validator upgrade invalidates the cache", e2.call_count() >= 2,
              f"calls={e2.call_count()}")

        # --- different flags are a different question -------------------------
        e.reset_calls()
        e.run("--format", "text", "--flavour", "0", str(pdf))
        check("changed flags are validated again", e.call_count() >= 2,
              f"calls={e.call_count()}")

        # --- the off switch really is off ------------------------------------
        e3 = Env(tmp, enabled=False)
        e3.cache = e.cache
        e3.reset_calls()
        e3.run("--format", "json", "--flavour", "0", str(pdf))
        e3.reset_calls()
        e3.run("--format", "json", "--flavour", "0", str(pdf))
        check("VERAPDF_CACHE=off always measures", e3.call_count() >= 1,
              f"calls={e3.call_count()}")

        # --- several files: forwarded, never guessed at -----------------------
        e.reset_calls()
        r = e.run("--format", "json", str(pdf), str(changed))
        check("multiple files are forwarded uncached", e.call_count() >= 1 and "PASS" in r.stdout)

        # --- a damaged entry must not become a verdict -------------------------
        entries = list(e.cache.rglob("*.json"))
        check("entries are actually written", bool(entries), f"{len(entries)} found")
        if entries:
            # Corrupt them all. Damaging entries[0] corrupted an arbitrary
            # document's verdict, so the lookup for this one still hit and the
            # test reported a failure that was its own.
            for entry in entries:
                entry.write_text("{ this is not json")
            e.reset_calls()
            r = e.run("--format", "json", "--flavour", "0", str(pdf))
            check("a corrupt entry falls back to measuring",
                  e.call_count() >= 2 and "PASS" in r.stdout, f"calls={e.call_count()}")

        # --- a validator that produced nothing is not a verdict ---------------
        silent = tmp / "silent.py"
        silent.write_text("#!/usr/bin/env python3\nimport sys,os,pathlib\n"
                          "p=pathlib.Path(os.environ['FAKE_CALLS'])\n"
                          "p.write_text(str(int(p.read_text() or 0)+1) if p.exists() else '1')\n"
                          "sys.exit(1)\n")
        silent.chmod(0o755)
        e4 = Env(tmp)
        e4.fake = silent
        e4.cache = tmp / "cache2"
        e4.reset_calls()
        e4.run("--format", "json", "--flavour", "0", str(pdf))
        e4.reset_calls()
        e4.run("--format", "json", "--flavour", "0", str(pdf))
        # Two calls: --version, then the validation itself. A cached empty result
        # would skip the second and leave the count at one.
        #
        # This assertion was `>= 1` first, which is true whether the guard works
        # or not -- removing the guard left the test green. Caught by deliberately
        # breaking the code, which is the only way that class of test ever shows
        # itself.
        check("an empty result is never cached", e4.call_count() >= 2,
              f"calls={e4.call_count()} — a broken validator must not turn into "
              "a thousand stored verdicts")

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
