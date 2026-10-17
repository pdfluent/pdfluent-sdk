#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every package name the release workflows look up exists in the workspace.

wasm.yml resolved the wasm crate's version by a hard-coded package name
(`pdfluent-xfa-wasm`) that no longer exists: the lookup matched nothing and
the step died with `IndexError` after a green 17-minute wasm-pack build (run
37451390781). The version now resolves by manifest path, and this test holds
every remaining `cargo metadata` name lookup and every `-p` / `--package`
pin in the five release workflows to naming a package
`cargo metadata --no-deps` knows. Offline: metadata needs no network.

A `-p` value that expands at run time (`-p "$CRATE"`, `-p "$1"`) is not a
name and is skipped, as is `mkdir -p`, which only `-p` lines on a
cargo/cross command reach past.
"""
from __future__ import annotations

import json
import pathlib
import re
import subprocess
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"
HELPER = REPO / "scripts" / "release" / "crate_version_from_manifest.py"

FIVE = [
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
    "release-smoke.yml",
]

# p["name"]=="foo" and p['name']=='foo', however spaced.
NAME_LOOKUP = re.compile(r"""\[\s*["']name["']\s*\]\s*==\s*["']([^"'\s]+)["']""")
# cargo/cross ... -p foo / --package foo.
PACKAGE_PIN = re.compile(r"(?:^|\s)(?:-p|--package)\s+([^\s]+)")

MINIMUM_CASES = 5  # FLOOR


def _workspace_names() -> set[str]:
    proc = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(REPO),
    )
    assert proc.returncode == 0, f"cargo metadata failed: {proc.stderr[-500:]}"
    return {p["name"] for p in json.loads(proc.stdout)["packages"]}


def _lookups(text: str) -> list[str]:
    """Every literal package name `text` looks up or pins."""
    found = list(NAME_LOOKUP.findall(text))
    logical = re.sub(r"\\\n", " ", text)
    for line in logical.splitlines():
        code = line.split("#", 1)[0]
        if "cargo" not in code and "cross" not in code:
            continue
        for value in PACKAGE_PIN.findall(f" {code}"):
            if value[0] in "$<{\"'":
                continue
            found.append(value.rstrip(",;\"'"))
    return found


def _transform_step() -> str:
    """The Transform package step of wasm.yml, by its header."""
    text = (WORKFLOWS / "wasm.yml").read_text()
    rest = text[text.index("Transform package for publishing") :]
    end = re.search(r"\n      - (?:name|uses):", rest)
    return rest[: end.start() if end else None]


class ReleasePackageNameTests(unittest.TestCase):
    def test_every_name_lookup_names_a_workspace_package(self):
        names = _workspace_names()
        unknown: list[str] = []
        checked = 0
        for workflow in FIVE:
            for name in _lookups((WORKFLOWS / workflow).read_text()):
                checked += 1
                if name not in names:
                    unknown.append(f"{workflow}: {name!r}")
        self.assertGreaterEqual(checked, MINIMUM_CASES)
        self.assertEqual(unknown, [])

    def test_wasm_version_resolves_by_manifest_path(self):
        step = _transform_step()
        self.assertIn("crate_version_from_manifest.py", step)
        self.assertIn("WASM_CRATE", step)
        self.assertIn("Cargo.toml", step)
        self.assertNotIn('p["name"]', step)
        self.assertNotIn("p['name']", step)


class CrateVersionHelperTests(unittest.TestCase):
    def test_helper_prints_the_workspace_version(self):
        proc = subprocess.run(
            ["python3", str(HELPER), "crates/xfa-wasm/Cargo.toml"],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(REPO),
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        meta = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(REPO),
        )
        expected = next(
            p["version"]
            for p in json.loads(meta.stdout)["packages"]
            if p.get("manifest_path", "")
            .replace("\\", "/")
            .endswith("/crates/xfa-wasm/Cargo.toml")
        )
        self.assertEqual(proc.stdout.strip(), expected)

    def test_helper_fails_clearly_when_no_package_matches(self):
        proc = subprocess.run(
            ["python3", str(HELPER), "crates/does-not-exist/Cargo.toml"],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(REPO),
        )
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")
        self.assertIn("crates/does-not-exist/Cargo.toml", proc.stderr)
        self.assertNotIn("Traceback", proc.stderr)
        self.assertNotIn("IndexError", proc.stderr)


if __name__ == "__main__":
    unittest.main()
