#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the NOTICE guard must catch, and what it must leave alone.

Cases 3 and 4 are why it does not simply take every path-shaped word: NOTICE
deliberately names ANOTHER repository's layout, to say where something came
from. Demanding those exist would ask us to rebuild someone else's tree.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
GUARD = CI / "notice_names_what_exists.py"

BASE_NOTICE = """PDFluent — Notice on License Composition

The original notices are preserved in the THIRD_PARTY_LICENSES.txt file.
"""


def tree(tmp: pathlib.Path, root_text: str, copy_text: str | None = None) -> pathlib.Path:
    r = tmp / "repo"
    (r / "bindings/dotnet/src/PDFluent").mkdir(parents=True)
    (r / "THIRD_PARTY_LICENSES.txt").write_text("x\n", encoding="utf-8")
    (r / "NOTICE").write_text(root_text, encoding="utf-8")
    (r / "bindings/dotnet/src/PDFluent/NOTICE").write_text(
        root_text if copy_text is None else copy_text, encoding="utf-8")
    return r


def run_guard(r: pathlib.Path):
    omg = dict(os.environ, PDFLUENT_NOTICE_ROOT=str(r))
    return subprocess.run([sys.executable, str(GUARD)], capture_output=True,
                          text=True, env=omg)


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    ok = True

    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d), BASE_NOTICE)
        u = run_guard(r)
        ok &= case("a correct NOTICE is green", u.returncode == 0,
                      (u.stdout + u.stderr)[:200])

    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d), BASE_NOTICE, copy_text=BASE_NOTICE + "and one more line\n")
        u = run_guard(r)
        ok &= case("two copies that diverge is red",
                      u.returncode == 1 and "diverged" in u.stderr,
                      (u.stdout + u.stderr)[:200])

    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d), BASE_NOTICE + "\nSee bindings/dotnet/gone.txt for more.\n")
        u = run_guard(r)
        ok &= case("a path in our tree that does not exist is red",
                      u.returncode == 1 and "gone.txt" in u.stderr,
                      (u.stdout + u.stderr)[:200])

    # The UPSTREAM repository's path. `skills/` is a directory there and not
    # here, and that must not be a finding -- otherwise NOTICE can no longer say
    # where something came from.
    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d),
                 BASE_NOTICE + "\nTaken from skills/caveman/ at commit abc1234.\n")
        u = run_guard(r)
        ok &= case("a path from another repository is not a finding",
                      u.returncode == 0, (u.stdout + u.stderr)[:200])

    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d),
                 BASE_NOTICE + "\nSee https://github.com/someone/something/blob/main/gone.md\n")
        u = run_guard(r)
        ok &= case("a path inside a URL is not a path in our tree",
                      u.returncode == 0, (u.stdout + u.stderr)[:200])

    # The floor. Without this case the matching can break and everything above
    # stays green: zero paths would then read as "nothing wrong".
    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d), "PDFluent\n\nNo path here at all.\n")
        u = run_guard(r)
        ok &= case("zero recognised paths is SKIPPED, not green",
                      u.returncode == 1 and "SKIPPED (not a pass)" in u.stderr,
                      (u.stdout + u.stderr)[:200])

    with tempfile.TemporaryDirectory() as d:
        r = tree(pathlib.Path(d), BASE_NOTICE)
        (r / "bindings/dotnet/src/PDFluent/NOTICE").unlink()
        u = run_guard(r)
        ok &= case("a missing copy is SKIPPED, not green",
                      u.returncode == 1 and "SKIPPED (not a pass)" in u.stderr,
                      (u.stdout + u.stderr)[:200])

    print("test_notice_names_what_exists: " + ("OK" if ok else "GEFAALD"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
