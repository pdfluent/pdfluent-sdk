#!/usr/bin/env python3
"""A drop-in veraPDF that remembers verdicts it has already reached.

WHY THIS EXISTS

The holdout-1000 gate converts a thousand documents and validates every result
with veraPDF, which is Java and slow. On four cores that is hours. But a typical
round of fixes changes a handful of documents -- round 4's own notes say eight --
and the other 990 convert to exactly the same bytes as last time. Validating them
again asks a question whose answer cannot have changed.

WHAT IS AND IS NOT CACHEABLE

Not the source documents. veraPDF here validates OUR CONVERTED OUTPUT, and that
output is the thing under test, so keying a cache on the input document would
freeze the very answer we are measuring.

What works is keying on the bytes actually handed to veraPDF. Measured on
2026-08-20: converting the same document twice, two seconds apart, produces a
byte-identical file. So an unchanged document yields an identical hash, and last
run's verdict still applies. A changed one misses the cache and gets validated, as
it must.

The veraPDF version is part of the key. Without it, upgrading the validator would
silently replay verdicts from the old one, and the gate would be measuring a
program that is no longer installed.

USAGE

Point the test runner at this instead of the real binary:

    xfa-test-runner ... --verapdf-path scripts/ci/verapdf_cached.py

It forwards every argument, so it behaves like veraPDF for anything it does not
recognise. Set VERAPDF_REAL to the actual binary (default /usr/local/bin/verapdf)
and VERAPDF_CACHE_DIR to where verdicts live (default
/mnt/storagebox/pdfa-verdicts, which survives between runs).

VERAPDF_CACHE=off disables it entirely, which is how the two paths get compared.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

REAL = os.environ.get("VERAPDF_REAL", "/usr/local/bin/verapdf")
CACHE_DIR = Path(os.environ.get("VERAPDF_CACHE_DIR", "/mnt/storagebox/pdfa-verdicts"))
ENABLED = os.environ.get("VERAPDF_CACHE", "on").lower() not in ("off", "0", "false")

# The stored JSON has the validated file's path replaced by this, because that
# path is a temporary file that differs on every run. Replaying a stale path would
# put a filename in the output that never existed.
PATH_SENTINEL = "@@VERAPDF_CACHED_PATH@@"


def real_version() -> str:
    """veraPDF's own version string, part of every cache key."""
    try:
        out = subprocess.run([REAL, "--version"], capture_output=True, text=True, timeout=60)
        return (out.stdout + out.stderr).strip().splitlines()[0] if (out.stdout or out.stderr) else "unknown"
    except Exception:  # noqa: BLE001
        return "unknown"


def target_file(argv: list[str]) -> str | None:
    """The single file being validated, or None if this is not that shape.

    Conservative on purpose: anything with zero or several file arguments is
    forwarded uncached. A cache that guesses at argument parsing is a cache that
    returns the wrong verdict, and a wrong PASS is the worst output this whole
    system can produce.
    """
    files = [a for a in argv if not a.startswith("-") and Path(a).is_file()]
    if len(files) != 1:
        return None
    # Flags that take a value would swallow the next token; if the file we found
    # sits directly after such a flag it is that flag's argument, not the target.
    idx = argv.index(files[0])
    if idx > 0 and argv[idx - 1] in ("--format", "--flavour", "--profile", "--policyfile"):
        return None
    return files[0]


def cache_key(path: str, argv: list[str], version: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    # The flags change what veraPDF reports, so they belong in the key. The file
    # path itself deliberately does not: identical bytes deserve one entry.
    flags = [a for a in argv if a != path]
    h.update(b"\x00ARGS\x00" + "\x00".join(flags).encode())
    h.update(b"\x00VER\x00" + version.encode())
    return h.hexdigest()


def passthrough(argv: list[str]) -> int:
    proc = subprocess.run([REAL, *argv], capture_output=True)
    sys.stdout.buffer.write(proc.stdout)
    sys.stderr.buffer.write(proc.stderr)
    return proc.returncode


def main() -> int:
    argv = sys.argv[1:]

    if not ENABLED:
        return passthrough(argv)

    path = target_file(argv)
    if path is None:
        return passthrough(argv)

    version = real_version()
    try:
        key = cache_key(path, argv, version)
    except OSError:
        return passthrough(argv)

    entry = CACHE_DIR / key[:2] / f"{key}.json"

    if entry.is_file():
        try:
            stored = json.loads(entry.read_text())
            sys.stdout.write(stored["stdout"].replace(PATH_SENTINEL, path))
            sys.stderr.write(stored["stderr"].replace(PATH_SENTINEL, path))
            return int(stored["returncode"])
        except Exception:  # noqa: BLE001
            # A damaged entry must not become a verdict. Drop it and measure.
            try:
                entry.unlink()
            except OSError:
                pass

    proc = subprocess.run([REAL, *argv], capture_output=True, text=True)

    # Only remember a run that produced something. veraPDF failing to start is
    # not a verdict about the document, and caching it would make one machine's
    # broken Java installation look like a thousand invalid PDFs.
    if proc.stdout.strip():
        try:
            entry.parent.mkdir(parents=True, exist_ok=True)
            tmp = entry.with_suffix(".part")
            tmp.write_text(json.dumps({
                "returncode": proc.returncode,
                "stdout": proc.stdout.replace(path, PATH_SENTINEL),
                "stderr": proc.stderr.replace(path, PATH_SENTINEL),
                "verapdf_version": version,
            }))
            tmp.replace(entry)
        except OSError:
            pass  # an unwritable cache slows things down; it must not break them

    sys.stdout.write(proc.stdout)
    sys.stderr.write(proc.stderr)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
