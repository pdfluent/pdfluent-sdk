#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Exercise the purchase decision without contacting either live API."""
import io
import json
import os
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import yaml
import wave_needs_instance as wave

ROOT = pathlib.Path(__file__).resolve().parents[2]


class History:
    def __init__(self, conclusion="failure", job=None, active=None, sha="abc", active_status="queued"):
        self.conclusion = conclusion
        self.sha = sha
        self.job = job if job is not None else dict(
            name="workspace", status="completed", conclusion="failure", runner_id=1)
        self.active = active or []
        self.active_status = active_status

    def items(self, path, key, **filters):
        if key == "jobs":
            return iter([self.job])
        if filters.get("status") != "completed":
            return iter(self.active if filters["status"] == self.active_status else [])
        return iter([dict(id=10, head_sha=self.sha, conclusion=self.conclusion)])


class WaveTests(unittest.TestCase):
    def test_completed_verdict_is_not_automatically_repeated(self):
        for verdict in ("success", "failure", "timed_out"):
            with self.subTest(verdict=verdict):
                self.assertIn("already ran", wave.skip_reason(History(verdict), 20, "abc", "schedule"))

    def test_new_revision_runs(self):
        self.assertIsNone(wave.skip_reason(History(), 20, "def", "schedule"))

    def test_push_uses_the_same_revision_deduplication(self):
        self.assertIn("already ran", wave.skip_reason(History(), 20, "abc", "push"))
        self.assertIsNone(wave.skip_reason(History(), 20, "new", "push"))

    def test_manual_only_trigger_keeps_paid_work_serial(self):
        doc = yaml.safe_load((ROOT / ".github/workflows/ci-ephemeral.yml").read_text())
        triggers = doc.get("on", doc.get(True))
        self.assertEqual(set(triggers), {"workflow_dispatch"})
        self.assertEqual(doc["concurrency"]["group"], "ci-ephemeral")
        self.assertIs(doc["concurrency"]["cancel-in-progress"], False)
        self.assertEqual(doc["concurrency"].get("queue", "single"), "single")

    def test_older_pending_run_cannot_suppress_the_latest_push(self):
        self.assertIsNone(wave.skip_reason(History(active=[dict(id=9)]), 20, "new", "push"))

    def test_manual_retry_runs(self):
        self.assertIsNone(wave.skip_reason(History(), 20, "abc", "workflow_dispatch"))

    def test_active_older_wave_blocks_both_events(self):
        for event in ("push", "schedule", "workflow_dispatch"):
            self.assertIn("Earlier wave", wave.skip_reason(
                History(active=[dict(id=9)], active_status="in_progress"), 20, "abc", event))

    def test_current_or_newer_wave_does_not_block(self):
        self.assertIsNone(wave.skip_reason(
            History(active=[dict(id=20), dict(id=21)]), 20, "new", "schedule"))

    def test_cancelled_or_skipped_run_is_not_a_verdict(self):
        for verdict in ("cancelled", "skipped"):
            self.assertIsNone(wave.skip_reason(History(verdict), 20, "abc", "schedule"))

    def test_skipped_workspace_and_provisioning_failure_do_not_consume_revision(self):
        for job in (dict(name="workspace", status="completed", conclusion="skipped", runner_id=0),
                    dict(name="create-runner", status="completed", conclusion="failure", runner_id=2)):
            self.assertIsNone(wave.skip_reason(History(job=job), 20, "abc", "schedule"))

    def test_same_run_id_is_not_prior_evidence(self):
        self.assertIsNone(wave.skip_reason(History(), 10, "abc", "schedule"))

    def test_api_failure_refuses_provisioning_without_go_ahead_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = pathlib.Path(temporary) / "output"
            env = dict(GITHUB_REPOSITORY="example/test", GH_TOKEN="fixture",
                       GITHUB_RUN_ID="20", GITHUB_SHA="abc", GITHUB_EVENT_NAME="schedule",
                       GITHUB_OUTPUT=str(output))
            with patch.dict(os.environ, env), patch.object(wave.GitHub, "items", side_effect=OSError("unavailable")), patch("sys.stderr", new_callable=io.StringIO) as errors:
                self.assertEqual(wave.main(), 1)
            self.assertIn("Provisioning refused", errors.getvalue())
            self.assertFalse(output.exists())

    def test_output_controls_provisioning(self):
        for reason, expected in (("already tested", "true"), (None, "false")):
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as temporary:
                output = pathlib.Path(temporary) / "output"
                env = dict(GITHUB_REPOSITORY="example/test", GH_TOKEN="fixture",
                           GITHUB_RUN_ID="20", GITHUB_SHA="abc", GITHUB_EVENT_NAME="schedule",
                           GITHUB_OUTPUT=str(output))
                with patch.dict(os.environ, env), patch.object(wave, "skip_reason", return_value=reason), patch("sys.stdout", new_callable=io.StringIO):
                    self.assertEqual(wave.main(), 0)
                self.assertEqual(output.read_text(), f"overslaan={expected}\n")

    def test_pagination_reads_beyond_first_page(self):
        pages = [dict(workflow_runs=[dict(id=i) for i in range(100)]),
                 dict(workflow_runs=[dict(id=1234)])]
        with patch.object(wave.urllib.request, "urlopen", side_effect=[
                io.BytesIO(json.dumps(page).encode()) for page in pages]) as request:
            rows = list(wave.GitHub("example/test", "fixture").items("runs", "workflow_runs"))
        self.assertEqual(rows[-1]["id"], 1234)
        self.assertIn("page=2", request.call_args[0][0].full_url)

    def test_malformed_history_does_not_mean_empty_history(self):
        with patch.object(wave.urllib.request, "urlopen", return_value=io.BytesIO(b'{}')):
            with self.assertRaises(KeyError):
                list(wave.GitHub("example/test", "fixture").items("runs", "workflow_runs"))

    def test_guard_runs_before_any_instance_can_be_requested(self):
        doc = yaml.safe_load((ROOT / ".github/workflows/ci-ephemeral.yml").read_text())
        steps = doc["jobs"]["create-runner"]["steps"]
        guard = next(i for i, step in enumerate(steps) if step.get("id") == "golf")
        self.assertEqual(steps[guard]["run"], "python3 scripts/ci/wave_needs_instance.py")
        self.assertFalse(steps[guard].get("continue-on-error", False))
        for i, step in enumerate(steps):
            if step.get("id") in ("hergebruik", "create", "cpx62", "cpx52", "cpx42"):
                self.assertLess(guard, i)
                self.assertIn("steps.golf.outputs.overslaan != 'true'", step["if"])

    def test_snapshot_requires_manual_dispatch(self):
        doc = yaml.safe_load((ROOT / ".github/workflows/gate-snapshot-refresh.yml").read_text())
        triggers = doc.get("on", doc.get(True))
        self.assertEqual(set(triggers), {"workflow_dispatch"})

    def test_cases_run_in_push_ci(self):
        doc = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        self.assertIn("python3 scripts/ci/test_wave_needs_instance.py",
                      [step.get("run") for step in doc["jobs"]["guards"]["steps"]])


if __name__ == "__main__":
    unittest.main()
