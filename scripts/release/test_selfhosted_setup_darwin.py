#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Darwin setup installs both-arch tools under $PDFLUENT_TOOLS, no admin.

On Darwin every tool installs user-level into $PDFLUENT_TOOLS (refused
when unset), with no sudo and no Homebrew. Every download carries its
sha256 in the table at the top of the Darwin block -- a mismatch refuses
-- and python, node, java and dotnet install for both architectures while
only the arm64 side joins PATH. The x64 side is used only through
rosetta_x64.sh. All Darwin logic lives in setup_darwin_* functions called
from the dispatch only on Darwin; the Linux branches are untouched.
"""
from __future__ import annotations

import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "release" / "selfhosted_setup.sh"

MINIMUM_CASES = 8  # FLOOR

TOOLS = [
    "rust",
    "node",
    "python",
    "maturin",
    "java",
    "dotnet",
    "wasm",
    "napi",
    "zig",
    "docker",
]

TABLE_SHAS = [
    "DARWIN_RUSTUP_ARM64_SHA256",
    "DARWIN_PBS_311_ARM64_SHA256",
    "DARWIN_PBS_311_X64_SHA256",
    "DARWIN_PBS_312_ARM64_SHA256",
    "DARWIN_PBS_312_X64_SHA256",
    "DARWIN_NODE_20_ARM64_SHA256",
    "DARWIN_NODE_20_X64_SHA256",
    "DARWIN_NODE_24_ARM64_SHA256",
    "DARWIN_NODE_24_X64_SHA256",
    "DARWIN_TEMURIN_ARM64_SHA256",
    "DARWIN_TEMURIN_X64_SHA256",
    "DARWIN_MAVEN_SHA256",
    "DARWIN_DOTNET_INSTALL_SHA256",
]

BOTH_ARCH_SIDES = [
    ("python-${python_version}-arm64", "python-${python_version}-x64"),
    ("node-arm64", "node-x64"),
    ("temurin-17-arm64", "temurin-17-x64"),
    ("dotnet-arm64", "dotnet-x64"),
]

SHA_LINE = re.compile(r'^([A-Z0-9_]+_SHA256)="([0-9a-f]{64})"$', re.M)


def _code(text: str) -> str:
    return "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("#")
    )


def _darwin_block(text: str) -> str:
    start = text.index("--- Darwin: user-level tools")
    stop = text.index("setup_darwin_docker")
    return text[start:stop]


class DarwinSetupTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = SCRIPT.read_text()
        cls.code = _code(cls.text)
        cls.darwin = _code(_darwin_block(cls.text))

    def test_darwin_dispatch_routes_every_tool(self):
        for tool in TOOLS:
            with self.subTest(tool=tool):
                self.assertIn(f"setup_darwin_{tool}", self.code)
        self.assertIn('"setup_darwin_${tool}"', self.code)
        self.assertIn('"setup_${tool}"', self.code)

    def test_every_darwin_download_is_table_pinned(self):
        table = dict(SHA_LINE.findall(self.text))
        for name in TABLE_SHAS:
            with self.subTest(name=name):
                self.assertIn(name, table)
        # Every verified download takes a table sha, a side sha passed
        # down from one, or (node off the table) a SHASUMS entry.
        calls = 0
        for line in self.code.splitlines():
            if "()" in line or (
                "darwin_fetch" not in line
                and "darwin_install_python_side" not in line
                and "darwin_install_node_side" not in line
                and "darwin_install_temurin_side" not in line
            ):
                continue
            with self.subTest(line=line.strip()):
                self.assertRegex(
                    line, r'\$(?:[A-Z0-9_]+_SHA256|[a-z0-9_]*sha)"?\s*$'
                )
                calls += 1
        self.assertGreater(calls, 0)
        self.assertIn("SHASUMS256.txt", self.darwin)

    def test_both_arches_install_for_python_node_java_dotnet(self):
        for arm, x64 in BOTH_ARCH_SIDES:
            with self.subTest(arm=arm):
                self.assertIn(arm, self.darwin)
                self.assertIn(x64, self.darwin)
        self.assertIn("--architecture arm64", self.darwin)
        self.assertIn("--architecture x64", self.darwin)
        # Both python minors pinned, each for both machine archs.
        self.assertIn("3.11)", self.darwin)
        self.assertIn("3.12)", self.darwin)
        self.assertIn('"arm64" "aarch64"', self.darwin)
        self.assertIn('"x64" "x86_64"', self.darwin)

    def test_darwin_path_uses_no_sudo_or_brew(self):
        for token in ("sudo", "brew"):
            with self.subTest(token=token):
                self.assertNotIn(token, self.darwin.lower())

    def test_unset_tools_refuses_on_darwin(self):
        self.assertIn("PDFLUENT_TOOLS is unset", self.code)

    def test_maturin_uses_the_pinned_pip_form(self):
        # The Darwin side names the arm64 pip and hands over to the shared
        # setup_maturin, which holds the one pin and the "$PIP" install form.
        body = self.code[self.code.index("setup_darwin_maturin() {"):]
        body = body[: body.index("\n}\n")]
        self.assertIn('PIP="$arm/bin/pip"', body)
        self.assertIn("setup_maturin", body.split("{", 1)[1])
        self.assertNotIn("install", body)
        checks = (SCRIPT.parent / "_selfhosted_checks.sh").read_text()
        self.assertIn('MATURIN_VERSION="1.15.0"', checks)

    def test_only_the_arm64_side_joins_path(self):
        for line in self.code.splitlines():
            if "on_path" in line and "()" not in line:
                with self.subTest(line=line.strip()):
                    self.assertNotIn("x64", line)
        self.assertIn('to_env JAVA_HOME "$arm"', self.code)
        self.assertIn('to_env DOTNET_ROOT "$arm"', self.code)

    def test_every_tool_logs_its_version_and_arch(self):
        for tool in ("rust", "python", "node", "java", "dotnet"):
            with self.subTest(tool=tool):
                body = re.search(
                    r"setup_darwin_%s\(\) \{(.*?)(?=\n(?:setup_|darwin_|\Z))" % tool,
                    self.code,
                    re.S,
                )
                self.assertIsNotNone(body, f"setup_darwin_{tool} not found")
                assert body is not None
                self.assertIn("darwin_arch", body.group(1))


if __name__ == "__main__":
    unittest.main()
