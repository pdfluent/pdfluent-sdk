#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Does the no-key guard find a planted check, and leave the tests alone?

Four things a plain "does it find it" test would miss, and every one of them
was live at some point while #226 was being written:

  the floor          `git ls-files` outside a repository exits 0 with no output,
                     and a scan over nothing prints a clean product in exactly
                     the words a real one uses.
  the test exclusion the tests that prove the key is gone must name the key.
                     A guard that refuses those refuses its own evidence, and
                     whoever hits that deletes the test rather than the guard.
  the inline module  `#[cfg(all(test, not(target_arch = "wasm32")))]` is the
                     spelling xfa-wasm uses. A cut that only knew the bare
                     `#[cfg(test)]` read that whole module as shipped code --
                     measured, not imagined: it was the guard's first failure.
  the shipped half   and the mirror of the line above. A file with a test module
                     at the bottom must still be scanned down to it, or a check
                     in the code above hides behind the module below.

The plant is written into a fixture repository, never into this one.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import subprocess
import sys
import tempfile
from pathlib import Path

HIER = Path(__file__).resolve().parent
sys.path.insert(0, str(HIER))
from no_licence_key_in_a_binding import (  # noqa: E402
    PATRONEN, in_scope, scan, shipped_part,
)


def _git(wd: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(wd), "-c", "core.hooksPath=/nonexistent", *args],
        capture_output=True, text=True, env=sealed_env(cwd=wd),
    )


# One plant per pattern, so a pattern that stops matching is a failed test rather
# than a quieter guard. Each is the real shape it had before #226 removed it.
PLANTEN = {
    "key entry point": 'pdfluent::set_license_key("tier:business").unwrap();',
    "environment key": 'let k = std::env::var("PDFLUENT_LICENSE_KEY").unwrap_or_default();',
    "capability gate": "license::require_capability(Capability::DocxExport)?;",
    "output mark": 'text: "PDFluent Free Tier — pdfluent.com".into(),',
}


def main() -> int:
    fouten: list[str] = []

    if not PATRONEN:
        fouten.append("the real pattern list is empty, so the guard accepts everything")

    soorten = {s for s, _, _ in PATRONEN}
    ontbreekt = soorten - set(PLANTEN)
    if ontbreekt:
        fouten.append(f"patterns with no plant in this test: {sorted(ontbreekt)}")

    # The scope decision, checked directly rather than through a fixture: these
    # are the exact paths that made the guard red on the day it was written.
    for pad, verwacht in [
        ("crates/pdfluent/src/document.rs", True),
        ("bindings/dotnet/src/PDFluent/Licensing.cs", True),
        ("crates/pdf-node/index.d.ts", True),
        ("crates/pdfluent/tests/text_edit_facade.rs", False),
        ("crates/pdf-node/tests/typed_error_layer.test.js", False),
        ("crates/pdf-python/tests/test_office_export.py", False),
        ("crates/pdf-node/examples/licence.ts", False),
        ("docs/licensing.md", False),
        ("scripts/ci/no_licence_key_in_a_binding.py", False),
    ]:
        if in_scope(pad) != verwacht:
            fouten.append(
                f"in_scope({pad!r}) is {in_scope(pad)}, expected {verwacht}"
            )

    # The cut, over both spellings of the attribute and over a file with none.
    kaal = "fn ship() {}\n#[cfg(test)]\nmod tests {\n    let k = \"PDFLUENT_LICENSE_KEY\";\n}\n"
    if "PDFLUENT_LICENSE_KEY" in shipped_part(kaal):
        fouten.append("the cut does not stop at a bare #[cfg(test)]")
    wasm = (
        "fn ship() {}\n#[cfg(all(test, not(target_arch = \"wasm32\")))]\n"
        "mod tests {\n    let k = \"PDFLUENT_LICENSE_KEY\";\n}\n"
    )
    if "PDFLUENT_LICENSE_KEY" in shipped_part(wasm):
        fouten.append("the cut does not stop at #[cfg(all(test, ...))]")
    if shipped_part("fn ship() {}\n") != "fn ship() {}\n":
        fouten.append("the cut removes something from a file with no test module")

    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            print("[test-no-key] FAIL: could not create the fixture repository",
                  file=sys.stderr)
            return 1

        (wd / "crates" / "pdfluent" / "src").mkdir(parents=True)
        (wd / "crates" / "pdfluent" / "tests").mkdir(parents=True)
        schoon = wd / "crates" / "pdfluent" / "src" / "document.rs"
        schoon.write_text(
            "//! A document, opened and saved, with nothing asked of the caller.\n"
            "pub fn open(bytes: &[u8]) -> Vec<u8> { bytes.to_vec() }\n",
            encoding="utf-8",
        )
        # A test that names every removed thing. It must not be a hit: this is
        # the evidence the guard exists to keep, not a violation of it.
        (wd / "crates" / "pdfluent" / "tests" / "no_key.rs").write_text(
            "\n".join(PLANTEN.values()) + "\n", encoding="utf-8"
        )
        # And the same names inside an inline test module in a shipped file.
        (wd / "crates" / "pdfluent" / "src" / "edit.rs").write_text(
            "pub fn edit() {}\n#[cfg(all(test, not(target_arch = \"wasm32\")))]\n"
            "mod tests {\n    #[test]\n    fn unmarked() {\n"
            "        assert!(!text.contains(\"PDFluent trial\"));\n    }\n}\n",
            encoding="utf-8",
        )
        _git(wd, "add", "-A")

        gelezen, treffers = scan(str(wd), floor=1)
        if treffers:
            fouten.append(
                "hits in a tree whose only mentions are a test file and an inline "
                f"test module: {[(t[0], t[2]) for t in treffers]}"
            )
        if gelezen < 2:
            fouten.append(f"read {gelezen} shipped file(s) from a tree that has two")

        # Now plant each check in shipped source, one at a time.
        for soort, regel in PLANTEN.items():
            schoon.write_text(
                "pub fn open(bytes: &[u8]) -> Vec<u8> {\n"
                f"    {regel}\n"
                "    bytes.to_vec()\n}\n",
                encoding="utf-8",
            )
            _git(wd, "add", "-A")
            _, treffers = scan(str(wd), floor=1)
            gevonden = {t[2] for t in treffers}
            if soort not in gevonden:
                fouten.append(
                    f"planted a {soort} in shipped source and the guard did not "
                    f"see it; it reported {sorted(gevonden) or 'nothing'}"
                )

        # The floor, over a directory that is not a repository at all.
        with tempfile.TemporaryDirectory() as leeg:
            try:
                scan(leeg)
            except SystemExit:
                pass
            else:
                fouten.append("a scan over no files passed instead of refusing")

    if fouten:
        print("[test-no-key] FAIL", file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"[test-no-key] OK: {len(PLANTEN)} planted check(s) found in shipped "
        "source, none in tests or inline test modules, floor refuses an empty scan."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
