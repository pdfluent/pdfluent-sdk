#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The release consistency gate knows which crates are new (build 2).

Before their first publish, new crates are legitimately absent from
crates.io — but the gate failed every absent name, so the four new 1.1.0
names failed release-smoke's consistency check. scripts/first_publish.toml
lists the first-publish crates for the release: a listed-but-absent crate
reports NEW (first publish), an absent crate that is NOT listed still
fails, and a listed crate that IS on crates.io fails stale (remove it).

Every case runs the real main() in a fixture tree with a stubbed
registry, so the verdicts are judged on behaviour with no network.
"""
from __future__ import annotations

import importlib.util
import os
import pathlib
import sys
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
_spec = importlib.util.spec_from_file_location(
    "check_release_consistency", REPO / "scripts" / "check_release_consistency.py"
)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_release_consistency"] = checker
_spec.loader.exec_module(checker)

MINIMUM_CASES = 4  # FLOOR

# The first-publish list, pinned: a crate added to or removed from the
# release's new names must update this file too, naming the bytes it
# vouches for.
FIRST_PUBLISH = {
    "epub-extract",
    "pdfluent-cli",
    "pptx-extract",
    "xlsx-extract",
}


def _fixture(root: pathlib.Path, crates: dict[str, str]) -> None:
    (root / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "1.1.0"\n'
        'repository = "https://example.invalid/pdfluent"\n'
        'homepage = "https://example.invalid"\n'
        'documentation = "https://example.invalid/docs"\n'
    )
    for name, version in crates.items():
        d = root / "crates" / name
        d.mkdir(parents=True)
        (d / "Cargo.toml").write_text(
            "[package]\n"
            f'name = "{name}"\n'
            f'version = "{version}"\n'
            'description = "fixture"\n'
            'repository = "https://example.invalid/pdfluent"\n'
            'homepage = "https://example.invalid"\n'
            'documentation = "https://example.invalid/docs"\n'
        )
        (d / "README.md").write_text(f"# {name}\n")


def _present(version: str) -> dict:
    return {
        "crate": {"newest_version": version},
        "versions": [{"num": version, "yanked": False}],
    }


def _run_main(root: pathlib.Path, registry: dict, first_publish: set[str]):
    """main() in the fixture tree with a stubbed registry and list."""
    here = os.getcwd()
    os.chdir(root)
    try:
        with (
            mock.patch.object(checker, "fetch_crates_io", side_effect=lambda n: registry.get(n)),
            mock.patch.object(checker.time, "sleep", lambda *_a: None),
            mock.patch.object(checker, "FIRST_PUBLISH_TOML", root / "first_publish.toml"),
            mock.patch.object(sys, "argv", ["check_release_consistency.py"]),
        ):
            (root / "first_publish.toml").write_text(
                "# fixture list\ncrates = ["
                + ", ".join(f'"{n}"' for n in sorted(first_publish))
                + "]\n"
            )
            import io

            out = io.StringIO()
            err = io.StringIO()
            with mock.patch.object(sys, "stdout", out), mock.patch.object(sys, "stderr", err):
                code = checker.main()
            return code, out.getvalue() + err.getvalue()
    finally:
        os.chdir(here)


class FirstPublishTests(unittest.TestCase):
    def test_the_list_names_the_four_new_crates(self):
        self.assertEqual(
            checker.load_first_publish(checker.FIRST_PUBLISH_TOML),
            FIRST_PUBLISH,
        )

    def test_listed_but_absent_is_new_not_a_failure(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _fixture(root, {"new-crate": "1.1.0", "old-crate": "1.1.0"})
            code, output = _run_main(
                root,
                {"old-crate": _present("1.1.0")},
                {"new-crate"},
            )
            self.assertEqual(code, 0, output)
            self.assertIn("NEW (first publish)", output)

    def test_unlisted_absent_still_fails(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _fixture(root, {"new-crate": "1.1.0", "old-crate": "1.1.0"})
            code, output = _run_main(
                root,
                {"old-crate": _present("1.1.0")},
                set(),
            )
            self.assertEqual(code, 1, output)
            self.assertIn("FAIL  new-crate: not found on crates.io", output)

    def test_listed_but_present_fails_stale(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _fixture(root, {"new-crate": "1.1.0", "old-crate": "1.1.0"})
            code, output = _run_main(
                root,
                {"new-crate": _present("1.1.0"), "old-crate": _present("1.1.0")},
                {"new-crate"},
            )
            self.assertEqual(code, 1, output)
            self.assertIn("remove it from the first-publish list", output)


if __name__ == "__main__":
    unittest.main()
