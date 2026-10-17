#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the `cargo install pdfluent-cli` gate (T6 §3).

Direct path dependencies with versions pass the static check; one path-only
normal or build dep fails with its name. Transitive and dev path deps are
exempt, matching what `cargo publish` enforces. The publish-output
classifier tells `publish = false` (precondition, named) apart from a real
dry-run failure.
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "cli_cratesio_gate", HERE / "cli_cratesio_gate.py"
)
gate = importlib.util.module_from_spec(_spec)
sys.modules["cli_cratesio_gate"] = gate
_spec.loader.exec_module(gate)

MINIMUM_CASES = 11  # FLOOR


def _dep(name, req="^1", path=None, kind=None):
    return {"name": name, "rename": None, "req": req, "kind": kind,
            "path": path, "source": None if path else "registry+crates.io"}


def _metadata(*, drifted: str | None = None):
    """pdfluent-cli -> pdfluent -> {pdf-syntax, pdf-interpret}.

    `drifted` names a package whose path dep drops its version requirement.
    """
    def req_of(name):
        return "*" if name == drifted else "=9.9.9"

    cli_id = "pdfluent-cli 1.0.0 (path+file:///w/crates/pdfluent-cli)"
    pdfluent_id = "pdfluent 1.0.0 (path+file:///w/crates/pdfluent)"
    syntax_id = "pdf-syntax 0.5.7 (path+file:///w/crates/pdf-syntax)"
    interpret_id = "pdf-interpret 0.5.9 (path+file:///w/crates/pdf-interpret)"
    return {
        "packages": [
            {"id": cli_id, "name": "pdfluent-cli",
             "manifest_path": "/w/crates/pdfluent-cli/Cargo.toml",
             "dependencies": [
                 _dep("anyhow"),
                 _dep("pdfluent", req_of("pdfluent"),
                      "/w/crates/pdfluent"),
             ]},
            {"id": pdfluent_id, "name": "pdfluent",
             "manifest_path": "/w/crates/pdfluent/Cargo.toml",
             "dependencies": [
                 _dep("pdf-syntax", req_of("pdf-syntax"),
                      "/w/crates/pdf-syntax"),
                 _dep("pdf-interpret", req_of("pdf-interpret"),
                      "/w/crates/pdf-interpret"),
             ]},
            {"id": syntax_id, "name": "pdf-syntax",
             "manifest_path": "/w/crates/pdf-syntax/Cargo.toml",
             "dependencies": []},
            {"id": interpret_id, "name": "pdf-interpret",
             "manifest_path": "/w/crates/pdf-interpret",
             "dependencies": []},
        ],
        "resolve": {
            "nodes": [
                {"id": cli_id,
                 "deps": [{"pkg": pdfluent_id}]},
                {"id": pdfluent_id,
                 "deps": [{"pkg": syntax_id}, {"pkg": interpret_id}]},
                {"id": syntax_id, "deps": []},
                {"id": interpret_id, "deps": []},
            ]
        },
    }


class StaticCheckTests(unittest.TestCase):
    def test_versioned_closure_passes(self):
        self.assertEqual(gate.find_path_deps_without_version(_metadata()), [])

    def test_direct_path_only_dep_fails_with_its_name(self):
        failures = gate.find_path_deps_without_version(_metadata(drifted="pdfluent"))
        self.assertEqual(len(failures), 1)
        self.assertIn("pdfluent", failures[0])
        self.assertIn("path-only", failures[0])

    def test_build_path_only_dep_fails_with_its_name(self):
        doc = _metadata()
        doc["packages"][0]["dependencies"].append(
            _dep("codegen", "*", "/w/crates/codegen", kind="build")
        )
        failures = gate.find_path_deps_without_version(doc)
        self.assertEqual(len(failures), 1)
        self.assertIn("codegen", failures[0])
        self.assertIn("build", failures[0])

    def test_transitive_path_only_dep_is_not_flagged(self):
        # Only the published crate's own manifest matters to cargo: a
        # transitive path dep resolves through the registry version.
        self.assertEqual(
            gate.find_path_deps_without_version(_metadata(drifted="pdf-syntax")),
            [],
        )

    def test_dev_path_only_dep_is_exempt(self):
        # Verified 01-10-2026: `cargo publish --dry-run` accepts a path-only
        # dev-dependency, so flagging it would cry about nothing.
        doc = _metadata()
        doc["packages"][0]["dependencies"].append(
            _dep("pdfluent-lopdf", "*", "/w/crates/lopdf", kind="dev")
        )
        self.assertEqual(gate.find_path_deps_without_version(doc), [])

    def test_registry_dep_without_path_is_ignored(self):
        doc = _metadata()
        doc["packages"][0]["dependencies"].append(_dep("anyhow", "*"))
        self.assertEqual(gate.find_path_deps_without_version(doc), [])

    def test_patch_section_fails_with_its_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "Cargo.toml"
            manifest.write_text(
                '[package]\nname = "x"\n'
                '[patch.crates-io]\npdf-syntax = { path = "../pdf-syntax" }\n'
            )
            failures = gate.find_patch_sections([manifest])
            self.assertEqual(len(failures), 1)
            self.assertIn("patch.crates-io", failures[0])

    def test_clean_manifest_has_no_patch_sections(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "Cargo.toml"
            manifest.write_text('[package]\nname = "x"\nversion = "1.0.0"\n')
            self.assertEqual(gate.find_patch_sections([manifest]), [])

    def test_package_list_missing_readme_fails(self):
        failures = gate.check_package_list(["Cargo.toml", "LICENSE", "src/main.rs"])
        self.assertEqual(len(failures), 1)
        self.assertIn("README.md", failures[0])

    def test_package_list_complete_passes(self):
        self.assertEqual(
            gate.check_package_list(["Cargo.toml", "LICENSE", "README.md"]), []
        )

    def test_publish_classifier_names_not_yet_publishable(self):
        self.assertEqual(
            gate.classify_publish_output(
                "error: `pdfluent-cli` cannot be published.\n"
                "`package.publish` must be set to `true` or a non-empty "
                "list in Cargo.toml to publish.\n"
            ),
            "not-yet-publishable",
        )
        self.assertEqual(
            gate.classify_publish_output("error: failed to verify the project"),
            "failed",
        )


if __name__ == "__main__":
    unittest.main()
