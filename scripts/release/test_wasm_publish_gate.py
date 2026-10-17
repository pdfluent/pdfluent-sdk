#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The wasm publish is manual, gated, and reads back its own bytes.

A `v*` tag used to run `npm publish @pdfluent/sdk-wasm` with no gate at
all. Now tags start nothing: publishing is a dispatch with publish_npm,
the packed tarball is audited and smoked first, and the gate reads back
the artifact the build uploaded, so the bytes judged and the bytes
published are the same ones. (The node-bindings shape; see
test_node_bindings_publish_gate.py.)
"""
from __future__ import annotations

import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".github" / "workflows" / "wasm.yml"

MINIMUM_CASES = 5  # FLOOR


def _text() -> str:
    return WORKFLOW.read_text()


def _block(name: str) -> str:
    """The job block `name`, by the same header split the node test uses."""
    chunks = re.split(r"^  ([a-z][\w-]*):", _text(), flags=re.M)
    for i in range(1, len(chunks), 2):
        if chunks[i] == name:
            return chunks[i + 1]
    raise AssertionError(f"job {name} not found")


class WasmPublishGateTests(unittest.TestCase):
    def test_no_tag_trigger_dispatch_with_a_decision(self):
        text = _text()
        self.assertNotIn("tags:", text, "a tag must not start this workflow")
        self.assertIn("workflow_dispatch:", text)
        self.assertIn("publish_npm:", text)

    def test_publish_needs_the_decision_and_no_tag_arm(self):
        publish = _block("publish-npm")
        self.assertIn("inputs.publish_npm == 'true'", publish)
        self.assertIn(
            "!(github.ref_type == 'tag' && contains(github.ref_name, '-'))",
            publish,
            "the pre-release clause stays, in the ref_type spelling",
        )
        self.assertNotIn(
            "startsWith(github.ref, 'refs/tags/v')",
            publish,
            "a tag must not arm the publish",
        )
        # The standalone tag requirement, not just the occurrence inside
        # the pre-release clause: a branch dispatch with publish_npm=true
        # publishes nothing.
        self.assertIn("&& github.ref_type == 'tag'", publish)

    def test_publish_waits_for_the_build_and_the_gate(self):
        publish = _block("publish-npm")
        self.assertIn("needs: [wasm-build, release-gate]", publish)

    def test_gate_audits_the_tarball_and_smokes_the_package(self):
        gate = _block("release-gate")
        self.assertIn("license_audit.py", gate)
        self.assertIn("smoke_wasm.sh", gate)

    def test_gate_reads_back_the_build_artifact(self):
        gate = _block("release-gate")
        self.assertIn("download-artifact", gate)
        self.assertIn("wasm-pkg", gate)


class WasmToolchainTests(unittest.TestCase):
    def test_bindgen_cli_matches_the_lock(self):
        # wasm-pack shells out to wasm-bindgen: a floating installer
        # drifts off the wasm-bindgen the lockfile resolved (0.2.114
        # installed against 0.2.121 locked). The build installs the CLI
        # at the version read from Cargo.lock -- and this executes the
        # workflow's own extraction to prove it reads the lock.
        import subprocess
        import sys
        import tomllib

        build = _block("wasm-build")
        self.assertIn("wasm-bindgen-cli", build)
        snippets = re.findall(r"python3 -c '([^']+)'", build)
        readers = [s for s in snippets if "Cargo.lock" in s]
        self.assertEqual(len(readers), 1, snippets)
        proc = subprocess.run(
            [sys.executable, "-c", readers[0]],
            capture_output=True,
            text=True,
            timeout=60,
            cwd=str(REPO),
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        with open(REPO / "Cargo.lock", "rb") as fh:
            lock = tomllib.load(fh)
        locked = next(
            p["version"] for p in lock["package"] if p["name"] == "wasm-bindgen"
        )
        self.assertEqual(proc.stdout.strip(), locked)
        self.assertNotRegex(build, r"wasm-bindgen-cli --version 0\.")


if __name__ == "__main__":
    unittest.main()
