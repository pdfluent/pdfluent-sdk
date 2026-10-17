#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The wasm-licence guard refuses what it should, and only that (#358).

The cases are the shapes the published package has actually had: the manifest
pointing at a file nobody packs, `files` naming one of the three documents, and
the correct shape. The last block runs the guard over the real transform script,
so dropping the licence files out of the `files` list it writes turns this red
rather than leaving a guard that describes a script nobody has.
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import the_wasm_package_carries_its_licence as guard  # noqa: E402

MINIMUM_CASES = 7  # FLOOR


def package(root: pathlib.Path, licence: str, files: list[str],
            present: tuple[str, ...] = guard.LICENCE_FILES) -> pathlib.Path:
    """A package directory with a chosen manifest and chosen files on disk."""
    root.mkdir(parents=True, exist_ok=True)
    for name in present:
        (root / name).write_text("licence text\n", encoding="utf-8")
    (root / "package.json").write_text(json.dumps(
        {"name": "@pdfluent/sdk-wasm", "version": "1.0.0",
         "license": licence, "files": files}, indent=2) + "\n", encoding="utf-8")
    return root


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as directory:
        tmp = pathlib.Path(directory)

        pkg = package(tmp / "correct", "SEE LICENSE IN LICENSE-OFFER",
                      ["xfa_wasm.js", *guard.LICENCE_FILES])
        found = guard.judge(pkg)
        case("all three documents in files passes", found == [], str(found))

        # The shape every published version up to beta.17.5 had.
        pkg = package(tmp / "promise-broken", "SEE LICENSE IN LICENSE-OFFER",
                      ["xfa_wasm.js"])
        found = guard.judge(pkg)
        case("a manifest pointing at a file npm will not pack is caught",
             any("nobody receives" in x for x in found), str(found))
        case("and it names the file it promised",
             any("LICENSE-OFFER" in x for x in found), str(found))

        pkg = package(tmp / "one-of-three", "SEE LICENSE IN LICENSE-OFFER",
                      ["xfa_wasm.js", "LICENSE-OFFER"])
        found = guard.judge(pkg)
        case("the note without the two texts it names is caught",
             any("LICENSE-COMMERCIAL" in x for x in found)
             and any("LICENSE" in x for x in found), str(found))

        # A file that is not in the directory cannot be missing from the
        # tarball, so it is not this guard's finding.
        pkg = package(tmp / "absent", "SEE LICENSE IN LICENSE",
                      ["xfa_wasm.js", "LICENSE"], present=("LICENSE",))
        found = guard.judge(pkg)
        case("a document that is not in the directory is not reported",
             found == [], str(found))

        # An SPDX expression makes no promise about a file. npm rejects the
        # compound one, which is why this package uses the pointer form -- but
        # the guard must not invent a finding if that ever changes.
        pkg = package(tmp / "expression", "MIT",
                      ["xfa_wasm.js", *guard.LICENCE_FILES])
        found = guard.judge(pkg)
        case("an expression rather than a pointer is not read as a promise",
             not any("nobody receives" in x for x in found), str(found))

    print("\n  -- against the real transform script --")
    done = subprocess.run([sys.executable, str(HERE / "the_wasm_package_carries_its_licence.py")],
                          capture_output=True, text=True)
    case("transform-wasm-pkg.sh writes a files list the tarball keeps",
         done.returncode == 0, done.stdout[-500:] + done.stderr[-500:])

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
