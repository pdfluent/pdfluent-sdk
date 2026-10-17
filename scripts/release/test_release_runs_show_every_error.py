#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A release run shows every error, not the first.

The matrices are already `fail-fast: false`; what still stopped a run after
one error was serial structure: a downstream job that `needs:` a matrix is
skipped when any leg fails, so the run reported the first failed leg and
silence below it. The validation jobs in the five release workflows now
carry `if: ${{ !cancelled() }}`: they run on the legs that did succeed, name
every skipped leg explicitly, and fail at the end when any is missing.

This holds the two halves of that change:

  * no PUBLISH job carries `!cancelled()`, `always()` or `continue-on-error`
    -- what gets published and when is exactly what it was;
  * every VALIDATION job that waits on another job carries `!cancelled()`,
    so one failed leg never skips the validation again.

WHAT COUNTS AS A PUBLISH JOB. By what the job does, not by its name: a job
is a publish job when any step matches a publish signature -- `npm publish`,
`cargo publish`, `gh release create`, an upload to a release (`gh release
upload`, `actions/upload-release-asset`), `softprops/action-gh-release`,
`actions/create-release`, `scripts/release/publish.sh`, `napi publish`,
`twine upload`, `dotnet nuget push`, `mvn deploy`, `docker push` -- or when
any step reads a publish credential (`NPM_TOKEN`, `NODE_AUTH_TOKEN`, a
registry token env). `permissions: contents: write` alone is not a signal:
it is also what a job that only files an issue carries. `${{ github.token }}`
read as `GH_TOKEN` for the artefact downloads is not a signal either: that
token only reads this repository's artefacts, and the natives smoke legs
all carry it without publishing anything.

WHAT COUNTS AS A VALIDATION JOB. Every other job in the five release
workflows that has `needs:`. That is deliberately a superset of "needs a
matrix": the natives smoke legs wait on the single package job and the wasm
gate waits on the single wasm build, and both must run -- and name their
skips -- rather than vanish when the job above them fails. Jobs without
`needs:` (the build matrices, the single release-smoke job) are not judged
here: there is no upstream failure for them to survive.
"""
from __future__ import annotations

import pathlib
import re
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

FIVE = [
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
    "release-smoke.yml",
]

# The publish jobs, by what they do (see the docstring). Held as a set so
# that a new publishing step cannot arrive without someone reading this
# file: a publish the heuristic does not recognise is a publish this guard
# does not protect.
EXPECTED_PUBLISH_JOBS = {
    ("release-binaries.yml", "create-release"),
    ("node-bindings.yml", "publish-npm"),
    ("wasm.yml", "publish-npm"),
}

# Substrings of a step's `uses:` or `run:` that mean the step publishes.
PUBLISH_SIGNATURES = (
    "npm publish",
    "napi publish",
    "scripts/release/publish.sh",
    "cargo publish",
    "twine upload",
    "dotnet nuget push",
    "mvn deploy",
    "gh release create",
    "gh release upload",
    "docker push",
    "softprops/action-gh-release",
    "actions/create-release",
    "actions/upload-release-asset",
)

# A publish credential read by a step. `github.token` is deliberately
# absent: read as GH_TOKEN for the artefact downloads it only fetches this
# repository's own artefacts.
PUBLISH_CREDENTIALS = (
    "NPM_TOKEN",
    "NODE_AUTH_TOKEN",
    "CARGO_REGISTRY_TOKEN",
    "PYPI_TOKEN",
    "NUGET_API_KEY",
)

# `always()`, `cancelled()` and `failure()` in a job's own `if:` replace the
# `success()` GitHub otherwise prefixes: the job then runs when everything
# it needs was skipped or failed. That is the point on a validation job and
# a widened gate on a publish job.
STATUS_OVERRIDE = re.compile(r"\b(always|cancelled|failure)\s*\(")

# The fan-out a validation job carries: `if: ${{ !cancelled() }}`, however
# spaced. A bare `cancelled()` without the negation runs only on a
# cancellation, which is not this.
NOT_CANCELLED = re.compile(r"!\s*cancelled\s*\(\s*\)")


def publish_reason(job: dict) -> str | None:
    """Why this job publishes, or None when it does not."""
    for step in job.get("steps") or []:
        if not isinstance(step, dict):
            continue
        haystack = "\n".join(
            [
                str(step.get("uses") or ""),
                str(step.get("run") or ""),
                yaml.safe_dump(step.get("env") or {}),
            ]
        )
        for needle in PUBLISH_SIGNATURES:
            if needle in haystack:
                return needle
        for credential in PUBLISH_CREDENTIALS:
            if credential in haystack:
                return f"credential {credential}"
    return None


def _step_continues_on_error(step: dict) -> bool:
    value = step.get("continue-on-error")
    if isinstance(value, bool):
        return value
    return isinstance(value, str) and value.strip().lower() == "true"


def publish_violations(name: str, job_name: str, job: dict) -> list[str]:
    """Every fan-out a publish job carries (it must carry none)."""
    violations = []
    job_if = job.get("if")
    if isinstance(job_if, str) and STATUS_OVERRIDE.search(job_if):
        violations.append(
            f"{name}: publish job `{job_name}` carries `{job_if.strip()}`: "
            "a publish job keeps its hard needs and success conditions."
        )
    for step in job.get("steps") or []:
        if not isinstance(step, dict):
            continue
        if _step_continues_on_error(step):
            label = step.get("name") or step.get("uses") or "a step"
            violations.append(
                f"{name}: publish job `{job_name}` runs {label} with "
                "continue-on-error: a publish step that fails must fail."
            )
    return violations


def validation_violations(name: str, job_name: str, job: dict) -> list[str]:
    """The missing fan-out on a validation job that waits on another job."""
    if not job.get("needs"):
        return []
    job_if = job.get("if")
    if isinstance(job_if, str) and NOT_CANCELLED.search(job_if):
        return []
    return [
        f"{name}: validation job `{job_name}` waits on another job without "
        "`if: ${{ !cancelled() }}`: one failed leg skips it in silence."
    ]


def scan(workflows_dir: pathlib.Path) -> tuple[list[str], set[tuple[str, str]]]:
    """Every violation across the five release workflows, plus the jobs
    the heuristic recognised as publishing."""
    violations: list[str] = []
    publish_jobs: set[tuple[str, str]] = set()
    for name in FIVE:
        doc = yaml.safe_load((workflows_dir / name).read_text())
        for job_name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            if publish_reason(job) is not None:
                publish_jobs.add((name, str(job_name)))
                violations.extend(publish_violations(name, str(job_name), job))
            else:
                violations.extend(
                    validation_violations(name, str(job_name), job)
                )
    return violations, publish_jobs


def _job(reason_run: str = "", **overrides) -> dict:
    """A minimal job whose single step runs `reason_run`."""
    job: dict = {"steps": [{"run": reason_run}]}
    job.update(overrides)
    return job


class PublishHeuristicTests(unittest.TestCase):
    def test_each_publish_signature_is_recognised(self):
        for signature in (
            "npm publish ./pkg --access public",
            "cargo publish --locked",
            "gh release create v1.0.0 dist/*",
            "gh release upload v1.0.0 dist/*",
        ):
            with self.subTest(signature=signature):
                reason = publish_reason(_job(signature))
                self.assertIsNotNone(reason, signature)

    def test_each_publish_action_is_recognised(self):
        for uses in (
            "softprops/action-gh-release@v2",
            "actions/create-release@v1",
            "actions/upload-release-asset@v1",
        ):
            with self.subTest(uses=uses):
                job = {"steps": [{"uses": uses}]}
                self.assertIsNotNone(publish_reason(job), uses)

    def test_the_publish_script_is_recognised(self):
        job = _job("bash scripts/release/publish.sh")
        self.assertEqual(publish_reason(job), "scripts/release/publish.sh")

    def test_a_registry_token_is_recognised(self):
        job = {
            "steps": [
                {
                    "env": {"NPM_TOKEN": "${{ secrets.NPM_TOKEN }}"},
                    "run": "npm run deploy",
                }
            ]
        }
        self.assertIsNotNone(publish_reason(job))

    def test_a_gate_is_not_a_publish_job(self):
        job = _job(
            "python3 ../../scripts/release/artifacts_cover_every_platform.py"
            " --node-dir . --require-all node",
            needs=["build-release"],
        )
        self.assertIsNone(publish_reason(job))

    def test_a_read_only_github_token_is_not_a_publish_job(self):
        # The natives smoke legs download other workflows' artefacts with
        # GH_TOKEN: that token only reads this repository, so it must not
        # mark the job as publishing.
        job = {
            "steps": [
                {
                    "env": {"GH_TOKEN": "${{ github.token }}"},
                    "run": 'curl -fsSL -H "Authorization: Bearer $GH_TOKEN" "$URL" -o body',
                }
            ]
        }
        self.assertIsNone(publish_reason(job))


class FanOutRuleTests(unittest.TestCase):
    def test_a_publish_job_with_not_cancelled_is_refused(self):
        job = _job(
            "npm publish ./pkg",
            needs=["release-gate"],
            **{"if": "${{ !cancelled() }}"},
        )
        violations = publish_violations("wasm.yml", "publish-npm", job)
        self.assertEqual(len(violations), 1)
        self.assertIn("publish job `publish-npm`", violations[0])

    def test_a_publish_job_with_always_is_refused(self):
        job = _job(
            "npm publish ./pkg",
            needs=["release-gate"],
            **{"if": "${{ always() }}"},
        )
        self.assertEqual(
            len(publish_violations("wasm.yml", "publish-npm", job)), 1
        )

    def test_a_publish_job_with_continue_on_error_is_refused(self):
        job = {
            "needs": ["release-gate"],
            "steps": [
                {"run": "npm publish ./pkg", "continue-on-error": True},
            ],
        }
        violations = publish_violations("wasm.yml", "publish-npm", job)
        self.assertEqual(len(violations), 1)
        self.assertIn("continue-on-error", violations[0])

    def test_a_clean_publish_job_passes(self):
        job = _job(
            "npm publish ./pkg",
            needs=["wasm-build", "release-gate"],
            **{
                "if": "github.ref_type == 'tag'"
                " && inputs.publish_npm == 'true'"
            },
        )
        self.assertEqual(
            publish_violations("wasm.yml", "publish-npm", job), []
        )

    def test_a_validation_job_with_not_cancelled_passes(self):
        job = _job(
            "python3 ../artifacts_cover_every_platform.py --require-all cli",
            needs=["build"],
            **{"if": "${{ !cancelled() }}"},
        )
        self.assertEqual(
            validation_violations("release-binaries.yml", "release-gate", job),
            [],
        )

    def test_a_validation_job_without_it_is_refused(self):
        job = _job(
            "python3 ../artifacts_cover_every_platform.py --require-all cli",
            needs=["build"],
        )
        violations = validation_violations(
            "release-binaries.yml", "release-gate", job
        )
        self.assertEqual(len(violations), 1)
        self.assertIn("`release-gate`", violations[0])

    def test_a_bare_cancelled_without_the_negation_is_refused(self):
        job = _job("echo validate", needs=["build"], **{"if": "${{ cancelled() }}"})
        self.assertEqual(
            len(validation_violations("wasm.yml", "release-gate", job)), 1
        )

    def test_a_job_without_needs_is_not_judged(self):
        self.assertEqual(validation_violations("wasm.yml", "wasm-build", _job()), [])
        self.assertEqual(
            validation_violations("release-smoke.yml", "smoke", _job()), []
        )


class ReleaseWorkflowsTests(unittest.TestCase):
    def test_the_publish_jobs_are_exactly_those_three(self):
        _, publish_jobs = scan(WORKFLOWS)
        self.assertEqual(publish_jobs, EXPECTED_PUBLISH_JOBS)

    def test_no_publish_job_carries_a_fan_out(self):
        violations, _ = scan(WORKFLOWS)
        publish = [v for v in violations if "publish job" in v]
        self.assertEqual(publish, [])

    def test_every_validation_job_with_needs_survives_a_failed_leg(self):
        violations, _ = scan(WORKFLOWS)
        self.assertEqual(violations, [])


if __name__ == "__main__":
    unittest.main()
