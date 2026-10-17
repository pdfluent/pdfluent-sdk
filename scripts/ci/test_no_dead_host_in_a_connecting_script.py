#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The dead-host guard refuses a literal address and lets a version pin pass."""
import pathlib
import subprocess
import sys
import tempfile

# Built from parts: the guard scans this file too, and it connects (ssh below).
ADDR = ".".join(["10", "1", "2", "3"])
GUARD = pathlib.Path(__file__).resolve().parent / "no_dead_host_in_a_connecting_script.py"


def run(body: str) -> int:
    with tempfile.TemporaryDirectory() as t:
        root = pathlib.Path(t)
        (root / "scripts").mkdir()
        (root / "scripts" / "go.sh").write_text("#!/bin/sh\n" + body + "ssh build-host true\n")
        return subprocess.run([sys.executable, str(GUARD), "--root", str(root)],
                              capture_output=True, text=True).returncode


def main() -> int:
    failures = []
    if run(f'HOST={ADDR}\n') == 0:
        failures.append("a literal address in a connecting script must fail")
    if run('OPENCV_VERSION="4.14.0.94"\n') != 0:
        failures.append("a NAME_VERSION pin must pass")
    if run('pip install opencv==4.14.0.94\n') != 0:
        failures.append("a pkg==x.y.z.w pin must pass")
    if run(f'OPENCV_VERSION="4.14.0.94"; HOST={ADDR}\n') == 0:
        failures.append("a pin on the line must not excuse an address beside it")
    for f in failures:
        print("FAIL", f)
    if failures:
        return 1
    print("dead-host guard: an address fails, a version pin passes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
