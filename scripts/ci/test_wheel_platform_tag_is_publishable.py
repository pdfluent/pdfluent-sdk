#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The wheel-tag rule refuses what the index refuses, and only that (#358).

The first case is the wheel that was actually built on 10-09-2026 and would
have been rejected at the upload after passing `twine check`; the second is the
one that replaced it. The rest are the ways a name-matching version of this
rule was wrong: a substring of the file name that is not a tag, a tag set where
one of two platforms is unacceptable, and a directory with nothing in it.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "release"))
import wheel_platform_tag_is_publishable as rule  # noqa: E402

MINIMUM_CASES = 14  # FLOOR


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    # The two Linux wheels of 10-09-2026, in the order they were built.
    refused = "pdfluent-1.0.0-cp38-abi3-linux_x86_64.whl"
    case("the wheel a fresh Ubuntu builds is refused",
         rule.unacceptable(refused) == ["linux_x86_64"], str(rule.unacceptable(refused)))

    accepted = "pdfluent-1.0.0-cp38-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
    case("the zig-built manylinux wheel is accepted",
         rule.unacceptable(accepted) == [], str(rule.unacceptable(accepted)))

    universal = ("pdfluent-1.0.0-cp38-abi3-macosx_10_12_x86_64.macosx_11_0_arm64."
                 "macosx_10_12_universal2.whl")
    case("the macOS universal2 wheel is accepted",
         rule.unacceptable(universal) == [], str(rule.unacceptable(universal)))

    windows = "pdfluent-1.0.0-cp38-abi3-win_amd64.whl"
    case("the Windows wheel is accepted",
         rule.unacceptable(windows) == [], str(rule.unacceptable(windows)))

    case("a pure-Python wheel is accepted",
         rule.unacceptable("pdfluent-1.0.0-py3-none-any.whl") == [])

    # Why the rule reads the field and not the file name.
    mixed = "pdfluent-1.0.0-cp38-abi3-macosx_11_0_arm64.linux_x86_64.whl"
    case("one bad platform in a tag set refuses the file",
         rule.unacceptable(mixed) == ["linux_x86_64"], str(rule.unacceptable(mixed)))

    named = "macosx-tools-1.0.0-cp38-abi3-linux_x86_64.whl"
    case("a distribution named after a platform does not launder the tag",
         rule.unacceptable(named) == ["linux_x86_64"], str(rule.unacceptable(named)))

    case("a build tag between version and python tag is still read",
         rule.platforms("pdfluent-1.0.0-1-cp38-abi3-win_amd64.whl") == ["win_amd64"],
         str(rule.platforms("pdfluent-1.0.0-1-cp38-abi3-win_amd64.whl")))

    case("an sdist is not a wheel name",
         rule.platforms("pdfluent-1.0.0.tar.gz") == [])

    case("a name missing its version is not a wheel name",
         rule.platforms("pdfluent-cp38-abi3-win_amd64.whl") == [])

    # The two whole-run behaviours, over real files.
    with tempfile.TemporaryDirectory() as directory:
        empty = pathlib.Path(directory) / "dist"
        empty.mkdir()
        done = subprocess.run(
            [sys.executable, str(REPO / "scripts/release/wheel_platform_tag_is_publishable.py"),
             str(empty)], capture_output=True, text=True)
        case("a directory with no wheel in it is an error, not a pass",
             done.returncode == 1, done.stdout + done.stderr)

        (empty / accepted).write_bytes(b"")
        (empty / refused).write_bytes(b"")
        done = subprocess.run(
            [sys.executable, str(REPO / "scripts/release/wheel_platform_tag_is_publishable.py"),
             str(empty)], capture_output=True, text=True)
        case("one unpublishable wheel refuses the whole directory",
             done.returncode == 1 and refused in done.stderr, done.stdout + done.stderr)

        (empty / refused).unlink()
        done = subprocess.run(
            [sys.executable, str(REPO / "scripts/release/wheel_platform_tag_is_publishable.py"),
             str(empty)], capture_output=True, text=True)
        case("a directory of publishable wheels passes",
             done.returncode == 0, done.stdout + done.stderr)

        # A name is not a file. Given the path a successful build WOULD have
        # written, this must refuse rather than approve the name -- the zero
        # wheel case arriving one argument at a time.
        done = subprocess.run(
            [sys.executable, str(REPO / "scripts/release/wheel_platform_tag_is_publishable.py"),
             str(empty / windows)], capture_output=True, text=True)
        case("a wheel named on the command line that does not exist is refused",
             done.returncode == 1 and "does not exist" in done.stderr,
             done.stdout + done.stderr)

        (empty / windows).write_bytes(b"")
        done = subprocess.run(
            [sys.executable, str(REPO / "scripts/release/wheel_platform_tag_is_publishable.py"),
             str(empty / windows)], capture_output=True, text=True)
        case("the same wheel, once it is on the disk, passes",
             done.returncode == 0, done.stdout + done.stderr)

    if cases < MINIMUM_CASES:
        print(f"\nFAIL: {cases} cases ran, at least {MINIMUM_CASES} were written")
        return 1
    if failures:
        print(f"\nFAIL: {len(failures)} of {cases} cases")
        return 1
    print(f"\nOK: {cases} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
