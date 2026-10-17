#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Exercise source scope, unsafe bases, and the actual shell deferral boundary."""
import os
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import yaml
from fixture_env import sealed_env
import development_gate_scope as scope

ROOT = pathlib.Path(__file__).resolve().parents[2]


class ScopeTests(unittest.TestCase):
    def test_only_known_source_and_docs_defer_infrastructure_regressions(self):
        for paths in ({"crates/pdf-engine/src/lib.rs"}, {"docs/guide.md"}, set()):
            self.assertEqual(scope.deferred_gates(paths), scope.INFRA_REGRESSIONS)

    def test_shared_inputs_and_unknown_changes_keep_all_regressions(self):
        for path in ("Cargo.lock", "Cargo.toml", ".cargo/config.toml", "rust-toolchain.toml",
                     "scripts/ci/run_test.sh", ".githooks/pre-push", "crates/pdf-engine/Cargo.toml",
                     "crates/pdf-engine/tests/fixture.pdf", "unknown", ".github/workflows/ci.yml"):
            self.assertEqual(scope.deferred_gates({path, "crates/pdf-engine/src/lib.rs"}), set())
        self.assertEqual(scope.deferred_gates(None), set())

    def test_unreadable_git_data_is_not_an_empty_change(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertIsNone(scope.changed_paths(directory))

    def test_uncommitted_deleted_renamed_and_untracked_inputs_are_included(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            env = sealed_env(cwd=root)
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], env=env).decode().strip()
            git("init", "-q")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            (root / "Cargo.lock").write_text("base")
            git("add", ".")
            git("-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture")
            base = git("rev-parse", "HEAD")
            (root / "Cargo.lock").rename(root / "renamed")
            (root / "untracked").write_text("new")
            with patch.dict(os.environ, {**env, "LANDING_BASE": base}, clear=True):
                self.assertEqual(scope.changed_paths(root), {"Cargo.lock", "renamed", "untracked"})

    def test_real_shell_defers_only_selected_gates_and_full_still_fails(self):
        source = (ROOT / "scripts/ci/local_ci_gate.sh").read_text()
        run = source[source.index('run() {'):source.index('# The gates that cost minutes')]
        with tempfile.TemporaryDirectory() as directory:
            for full, selected, expected_fail in ((0, "seedtest", 0), (1, "seedtest", 1), (0, "", 1)):
                script = f'FULL={full}; DEVELOPMENT_DEFERRED="{selected}"; PLAATS=""; LOGDIR="{directory}"; fail=0; pass=0; uitgesteld=0; gevallen=""; hoort_hier() {{ return 0; }}\n'
                script += run + '\nrun seedtest false\necho result=$fail,$pass,$uitgesteld\n'
                output = subprocess.check_output(["bash", "-c", script], text=True)
                self.assertIn(f"result={expected_fail},0,{1-expected_fail}", output)

    def test_regressions_and_publication_audit_remain_wired_in_ci(self):
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn("python3 scripts/ci/test_development_gate_scope.py", ci)
        wave = yaml.safe_load((ROOT / ".github/workflows/ci-ephemeral.yml").read_text())
        job = wave["jobs"]["public-snapshot-build"]
        self.assertEqual(job["runs-on"], "${{ needs.create-runner.outputs.label }}")
        self.assertIn("public-snapshot-build", wave["jobs"]["reap"]["needs"])
        self.assertIn("bash scripts/ci/public_snapshot_audit.sh --with-build", [s.get("run") for s in job["steps"]])
        commands = []
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            doc = yaml.safe_load(path.read_text())
            on = doc.get("on", doc.get(True))
            if isinstance(on, dict) and "push" in on:
                for job in doc.get("jobs", {}).values():
                    commands.extend(s.get("run", "") for s in job.get("steps", []))

        local = (ROOT / "scripts/ci/local_ci_gate.sh").read_text()
        import re
        for gate in scope.INFRA_REGRESSIONS:
            command = re.search(r"^(?:run|zwaar)\s+" + gate + r"\s+(.+)$", local, re.M).group(1)
            self.assertTrue(any(command in c for c in commands), f"{gate} lost push CI coverage: {command}")


if __name__ == "__main__":
    unittest.main()
