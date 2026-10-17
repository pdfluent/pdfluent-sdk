#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Self-hosted runs never publish, in any of the three publishing workflows.

runner_pool == 'self' together with any publish input true fails the FIRST
step of every job -- before checkout, before the toolchain, and before any
step that could read a secret. No secrets.* is reachable on a self run: the
publish jobs are the only ones that read secrets, and they carry the guard
first, so a self run with a publish input armed dies at step one.
"""
from __future__ import annotations

import pathlib
import re
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

# workflow -> its publish input. release-natives and release-smoke publish
# nothing and take no such input.
PUBLISH_INPUTS = {
    "release-binaries.yml": "create_release",
    "node-bindings.yml": "publish_npm",
    "wasm.yml": "publish_npm",
}

PUBLISH_JOBS = {"publish-npm", "create-release"}

MINIMUM_CASES = 10  # FLOOR


def load(name):
    return yaml.safe_load((WORKFLOWS / name).read_text())


def evaluate_guard(condition, inputs):
    """The guard's if: is `pool == 'self' && publish == 'true'`; run it."""
    parts = [p for p in condition.split("&&")]
    assert len(parts) == 2, condition
    result = True
    for part in parts:
        match = re.fullmatch(r"\s*(inputs\.\w+) == '(true|false|self|github)'\s*", part)
        assert match, part
        result = result and (inputs.get(match.group(1), "") == match.group(2))
    return result


class SelfRunsNeverPublishTests(unittest.TestCase):
    def guards(self):
        found = []
        for name, publish_input in PUBLISH_INPUTS.items():
            doc = load(name)
            for job_name, job in doc["jobs"].items():
                first = job["steps"][0]
                found.append((name, job_name, publish_input, first))
        return found

    def test_guard_is_first_in_every_job(self):
        cases = 0
        for name, job_name, publish_input, first in self.guards():
            with self.subTest(f"{name} {job_name}"):
                self.assertEqual(first.get("name"), "Self-hosted runs never publish")
                condition = first.get("if", "")
                self.assertIn("inputs.runner_pool == 'self'", condition)
                self.assertIn(f"inputs.{publish_input} == 'true'", condition)
                self.assertEqual(first.get("shell"), "bash")
                run = first.get("run", "")
                self.assertIn("runner_pool=self", run)
                self.assertIn("exit 1", run)
                cases += 1
        self.assertGreaterEqual(cases, MINIMUM_CASES)

    def test_guard_runs_exactly_when_self_and_publish(self):
        for name, job_name, publish_input, first in self.guards():
            condition = first.get("if", "")
            self.assertIn("inputs.runner_pool", condition)
            publish_key = f"inputs.{publish_input}"
            with self.subTest(f"{name} {job_name}"):
                armed = {"inputs.runner_pool": "self", publish_key: "true"}
                self.assertTrue(evaluate_guard(condition, armed))
                self.assertFalse(
                    evaluate_guard(
                        condition, {"inputs.runner_pool": "self", publish_key: "false"}
                    )
                )
                self.assertFalse(
                    evaluate_guard(
                        condition, {"inputs.runner_pool": "github", publish_key: "true"}
                    )
                )
                self.assertFalse(evaluate_guard(condition, {}))

    def test_no_secrets_reachable_on_a_self_run(self):
        for name in list(PUBLISH_INPUTS) + ["release-natives.yml", "release-smoke.yml"]:
            doc = load(name)
            for job_name, job in doc["jobs"].items():
                text = yaml.safe_dump(job)
                if "secrets." not in text:
                    continue
                with self.subTest(f"{name} {job_name}"):
                    self.assertIn(job_name, PUBLISH_JOBS)
                    first = job["steps"][0]
                    self.assertEqual(
                        first.get("name"), "Self-hosted runs never publish"
                    )
                    self.assertNotIn("secrets.", yaml.safe_dump(first))
                    self.assertIn("github.ref_type == 'tag'", job.get("if", ""))

    def test_natives_and_smoke_have_no_publish_surface(self):
        for name in ["release-natives.yml", "release-smoke.yml"]:
            doc = load(name)
            with self.subTest(name):
                on = doc.get("on", doc.get(True))
                inputs = on["workflow_dispatch"]["inputs"]
                self.assertNotIn("publish_npm", inputs)
                self.assertNotIn("create_release", inputs)
                self.assertNotIn("secrets.", (WORKFLOWS / name).read_text())


if __name__ == "__main__":
    unittest.main()
