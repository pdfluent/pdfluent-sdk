#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The binary release is manual, gated, and cut from the gate's own table.

A `v*` tag used to create the public GitHub Release with no CLI gate, from
archives the gate would not recognise (its own names, a LICENSE-only
member list). Now tags start nothing: the matrix is the gate's seven
triples plus an eighth windows-gnu leg that crosses from linux x64 on the
self pool, the archives are the gate's shape with the licence set inside,
the gate and a smoke hold the release, and cutting it is a dispatch at the
release tag with create_release. (The node-bindings shape; see
test_node_bindings_publish_gate.py.)
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "artifacts_cover_every_platform", HERE / "artifacts_cover_every_platform.py"
)
gate = importlib.util.module_from_spec(_spec)
sys.modules["artifacts_cover_every_platform"] = gate
_spec.loader.exec_module(gate)

REPO = HERE.parents[1]
WORKFLOW = REPO / ".github" / "workflows" / "release-binaries.yml"

MINIMUM_CASES = 6  # FLOOR


def _text() -> str:
    return WORKFLOW.read_text()


def _block(name: str) -> str:
    """The job block `name`, by the same header split the node test uses."""
    chunks = re.split(r"^  ([a-z][\w-]*):", _text(), flags=re.M)
    for i in range(1, len(chunks), 2):
        if chunks[i] == name:
            return chunks[i + 1]
    raise AssertionError(f"job {name} not found")


class ReleaseBinariesGateTests(unittest.TestCase):
    def test_no_tag_trigger_dispatch_with_a_decision(self):
        text = _text()
        self.assertNotIn("tags:", text, "a tag must not start this workflow")
        self.assertIn("workflow_dispatch:", text)
        self.assertIn("create_release:", text)

    def test_matrix_is_the_gate_table(self):
        build = _block("build")
        targets = sorted(re.findall(r"- target: (\S+)", build))
        want = sorted(
            [triple for _, triple, _ in gate.CLI_PLATFORMS] + ["x86_64-pc-windows-gnu"]
        )
        self.assertEqual(targets, want)

    def test_windows_gnu_cross_builds_on_linux(self):
        text = _text()
        entry = re.search(
            r"- target: x86_64-pc-windows-gnu\n"
            r"            os: (windows-latest)\n"
            r"            os-self: (\[self-hosted, linux, x64, pdfluent-release\])\n"
            r"            use_cross: (false)\n"
            r"            ext: ('\.exe')\n",
            text,
        )
        self.assertIsNotNone(
            entry,
            "the gnu leg rides windows-latest on github, linux x64"
            " on self, as a plain cargo build",
        )
        build = _block("build")
        self.assertIn("scripts/ci/apt_get.sh install -y -qq mingw-w64", build)
        self.assertIn("cargo-zigbuild is the documented fallback", build)
        self.assertIn("choco install mingw", build)
        self.assertIn(
            "cargo build --release -p pdfluent-cli --bin pdfluent --target",
            build,
        )
        self.assertIn("zip_cli_stage.py", build)

    def test_archives_are_the_gate_shape_with_the_licence_set(self):
        build = _block("build")
        self.assertIn("pdfluent-${VERSION}-${TARGET}.tar.gz", build)
        self.assertIn("pdfluent-${VERSION}-${TARGET}.zip", build)
        self.assertNotIn("pdfluent-$TARGET-v$VERSION", build)
        for member in (
            "LICENSE-COMMERCIAL", "NOTICE", "THIRD_PARTY_LICENSES.txt", "README.md",
        ):
            self.assertIn(member, build, f"archive member {member} missing")
        self.assertIn("man/man1", build, "the man pages ride every archive")
        self.assertIn(
            "crates/pdfluent-cli/Cargo.toml", build,
            "the version comes from the crate manifest, not the ref",
        )

    def test_windows_zips_use_forward_slashes_on_every_pool(self):
        # Build 2: Compress-Archive wrote man\man1\pdfluent.1 into the
        # win-x64 zip. Both Windows legs cut their zip through the stdlib
        # helper now, on every pool.
        build = _block("build")
        code = "\n".join(line.split("#", 1)[0] for line in build.splitlines())
        self.assertNotIn("Compress-Archive", code)
        self.assertNotIn("powershell.exe", code)
        self.assertIn("zip_cli_stage.py", build)

    def test_windows_packaging_has_python_on_both_pools(self):
        # The zip step runs python: the self pool installs it through the
        # setup tools, the github pool pins it for the windows legs.
        build = _block("build")
        self.assertIn("tools: rust,docker,python", build)
        self.assertIn("actions/setup-python@v5", build)
        self.assertIn("contains(matrix.target, 'windows')", build)

    def test_gate_runs_the_cli_gate_and_a_smoke(self):
        block = _block("release-gate")
        self.assertIn("artifacts_cover_every_platform.py --cli-dir", block)
        self.assertIn("--cli-version", block)
        self.assertIn("--require-all cli", block)
        self.assertIn("SHA256SUMS", block)
        self.assertIn("smoke_binary.sh", block)

    def test_release_waits_for_the_build_and_the_gate(self):
        release = _block("create-release")
        self.assertIn("needs: [build, release-gate]", release)

    def test_release_needs_a_tag_a_version_and_the_decision(self):
        release = _block("create-release")
        self.assertIn(
            "!(github.ref_type == 'tag' && contains(github.ref_name, '-'))",
            release,
            "the pre-release clause stays, in the ref_type spelling",
        )
        self.assertIn("github.ref_type == 'tag'", release)
        self.assertIn("inputs.create_release == 'true'", release)
        self.assertNotIn(
            "startsWith(github.ref, 'refs/tags/v')",
            release,
            "a tag must not arm the release",
        )
        self.assertIn(
            "The tag names the crate version", release,
            "a dispatch can aim at any ref; the release pins the version",
        )


if __name__ == "__main__":
    unittest.main()
