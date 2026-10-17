#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every runs-on in the six release workflows routes through runner_pool.

runner_pool (github|self, default github) picks the pool on dispatch; self
runs land on the self-hosted labels, and a push (tag) trigger counts as
self, because a tag must never spend hosted minutes. The matrix entries
hold both labels and the expression picks one:

  ubuntu-latest     -> [self-hosted, linux, x64, pdfluent-release]
  ubuntu-24.04-arm  -> [self-hosted, linux, arm64, pdfluent-release]  (native arm64 tests only)
  macos-*           -> [self-hosted, macos, arm64, pdfluent-release]  (osx-x64 cross-built there)
  windows-latest    -> [self-hosted, windows, x64, pdfluent-release]

The fourth label is the pool: without it [self-hosted, linux, x64] matches
the WSL desktop runner too, because GitHub compares labels
case-insensitively, and the release run landed on xfa-desktop-fast while
the GCP VM sat idle (run 37299529520). Every self target -- matrix entries
and plain fromJSON sets alike -- carries pdfluent-release, which only the
ephemeral release VMs register.

Cross builds stay on linux x64: only legs that RUN on arm64 -- the smoke
legs on ubuntu-24.04-arm -- map to the arm64 runner. One leg crosses off
its own OS: release-binaries' x86_64-pc-windows-gnu target builds with
mingw-w64 on [self-hosted, linux, x64, pdfluent-release] while the
github pool keeps windows-latest.

build-wheels.yml is the sixth: the manual wheel fallback, dispatch-only,
so no push trigger and no push-counts-as-self spelling. Its windows job
is the first plain (matrix-less) job whose github fallback is not
ubuntu-latest, so the plain-job case reads the fallback label off the
runs-on and expects that label's release set.
"""
from __future__ import annotations

import json
import pathlib
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"

SIX = [
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
    "release-smoke.yml",
    "build-wheels.yml",
]

EXPECTED_JOBS = {
    "release-natives.yml": [
        "natives",
        "ort-rust",
        "package",
        "smoke",
    ],
    "release-binaries.yml": ["build", "create-release", "release-gate"],
    "node-bindings.yml": [
        "build-and-test",
        "build-release",
        "publish-npm",
        "release-gate",
    ],
    "wasm.yml": ["publish-npm", "release-gate", "wasm-build"],
    "release-smoke.yml": ["smoke"],
    "build-wheels.yml": ["linux", "macos", "sdist", "windows"],
}

SELF_LABELS = {
    "ubuntu-latest": ["self-hosted", "linux", "x64", "pdfluent-release"],
    "ubuntu-24.04-arm": ["self-hosted", "linux", "arm64", "pdfluent-release"],
    "macos-latest": ["self-hosted", "macos", "arm64", "pdfluent-release"],
    "macos-15-intel": ["self-hosted", "macos", "arm64", "pdfluent-release"],
    "windows-latest": ["self-hosted", "windows", "x64", "pdfluent-release"],
}

MINIMUM_CASES = 14  # FLOOR


def load(name):
    return yaml.safe_load((WORKFLOWS / name).read_text())


def _top_level_split(expr: str, op: str) -> tuple[str, str] | None:
    """Split expr on the first op at paren depth 0, outside single quotes."""
    depth = 0
    in_single = False
    i = 0
    while i < len(expr):
        c = expr[i]
        if c == "'":
            if in_single and i + 1 < len(expr) and expr[i + 1] == "'":
                i += 2
                continue
            in_single = not in_single
            i += 1
            continue
        if in_single:
            i += 1
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth = max(0, depth - 1)
        elif depth == 0 and expr.startswith(op, i):
            return expr[:i], expr[i + len(op) :]
        i += 1
    return None


def _self_condition_true_on_push(runs_on: str) -> bool:
    """The runs-on self branch is taken on a push with an empty runner_pool.

    Shape is `${{ COND && SELF || HOSTED }}`; COND is evaluated with
    event_name=push and runner_pool empty (a tag push carries no dispatch
    input) as well as github (the dispatch default), and must be true in
    both, so no push-triggered run can resolve to the hosted label.
    """
    inner = runs_on
    if "${{" in inner:
        inner = inner.split("${{", 1)[1].rsplit("}}", 1)[0]
    split = _top_level_split(inner, "&&")
    if split is None:
        return False
    cond, _rest = split
    for pool in ("", "github"):
        expr = cond.replace("github.event_name", repr("push")).replace(
            "inputs.runner_pool", repr(pool)
        )
        expr = expr.replace("&&", " and ").replace("||", " or ")
        try:
            if not eval(expr, {"__builtins__": {}}):  # noqa: S307 - literals only
                return False
        except Exception:
            return False
    return True


def _step_condition_on_push(cond: str) -> bool | None:
    """A step if: under event_name=push; None when it needs matrix values."""
    if "matrix." in cond:
        return None
    expr = cond
    if "${{" in expr:
        expr = expr.split("${{", 1)[1].rsplit("}}", 1)[0]
    expr = expr.replace("github.event_name", repr("push")).replace(
        "inputs.runner_pool", repr("")
    )
    expr = expr.replace("&&", " and ").replace("||", " or ")
    try:
        return bool(eval(expr, {"__builtins__": {}}))  # noqa: S307 - literals only
    except Exception:
        return None


class RunnerPoolTests(unittest.TestCase):
    def test_runner_pool_input_declared(self):
        for name in SIX:
            with self.subTest(name):
                doc = load(name)
                on = doc.get("on", doc.get(True))
                runner_pool = on["workflow_dispatch"]["inputs"]["runner_pool"]
                self.assertEqual(runner_pool["default"], "github")
                self.assertEqual(runner_pool["type"], "choice")
                self.assertEqual(sorted(runner_pool["options"]), ["github", "self"])

    def test_every_runs_on_routes_through_runner_pool(self):
        cases = 0
        for name in SIX:
            doc = load(name)
            self.assertEqual(sorted(doc["jobs"]), EXPECTED_JOBS[name])
            for job_name, job in doc["jobs"].items():
                with self.subTest(f"{name} {job_name}"):
                    runs_on = job["runs-on"]
                    self.assertIsInstance(runs_on, str)
                    self.assertIn("inputs.runner_pool", runs_on)
                    cases += 1
        self.assertGreaterEqual(cases, MINIMUM_CASES)

    def test_matrix_entries_carry_both_labels(self):
        arm64_native = 0
        for name in SIX:
            doc = load(name)
            for job_name, job in doc["jobs"].items():
                matrix = (job.get("strategy") or {}).get("matrix")
                if not matrix:
                    continue
                entries = matrix.get("include", [])
                self.assertTrue(entries, f"{name} {job_name} has no include entries")
                github_key = "runs-on" if "runs-on" in entries[0] else "os"
                self_key = f"{github_key}-self"
                runs_on = job["runs-on"]
                self.assertIn("inputs.runner_pool == 'self'", runs_on)
                self.assertIn(f"matrix.{self_key}", runs_on)
                self.assertIn(f"matrix.{github_key}", runs_on)
                for entry in entries:
                    with self.subTest(f"{name} {entry}"):
                        self.assertIn(github_key, entry)
                        self.assertIn(self_key, entry)
                        github_label = entry[github_key]
                        self.assertIn(github_label, SELF_LABELS)
                        if (
                            name == "release-binaries.yml"
                            and entry.get("target") == "x86_64-pc-windows-gnu"
                        ):
                            self.assertEqual(
                                entry[self_key],
                                ["self-hosted", "linux", "x64", "pdfluent-release"],
                            )
                        else:
                            self.assertEqual(entry[self_key], SELF_LABELS[github_label])
                        if entry[self_key] == ["self-hosted", "linux", "arm64", "pdfluent-release"]:
                            arm64_native += 1
                            self.assertEqual(github_label, "ubuntu-24.04-arm")
        self.assertGreater(
            arm64_native, 0, "no native arm64 leg maps to the arm64 runner"
        )

    def test_plain_jobs_fall_back_to_github(self):
        plain = 0
        for name in SIX:
            doc = load(name)
            for job_name, job in doc["jobs"].items():
                if (job.get("strategy") or {}).get("matrix"):
                    continue
                runs_on = job["runs-on"]
                with self.subTest(f"{name} {job_name}"):
                    self.assertIn("inputs.runner_pool == 'self'", runs_on)
                    # The github fallback names the hosted label; the self
                    # branch carries that label's release set. One fallback,
                    # not zero and not two: a second `||` is a runner the
                    # routing no longer chooses between.
                    fallbacks = [
                        label
                        for label in SELF_LABELS
                        if f"|| '{label}'" in runs_on
                    ]
                    self.assertEqual(
                        len(fallbacks), 1, f"one hosted fallback: {runs_on}"
                    )
                    self.assertIn(
                        json.dumps(SELF_LABELS[fallbacks[0]]), runs_on
                    )
                    plain += 1
        self.assertGreater(plain, 0)

    def test_push_triggered_workflows_resolve_to_self(self):
        found = 0
        for name in SIX:
            doc = load(name)
            on = doc.get("on", doc.get(True)) or {}
            tags = ((on.get("push") or {}).get("tags")) or []
            if not tags:
                continue
            found += 1
            for job_name, job in doc["jobs"].items():
                with self.subTest(f"{name} {job_name}"):
                    runs_on = job["runs-on"]
                    self.assertIsInstance(runs_on, str)
                    self.assertIn("github.event_name == 'push'", runs_on)
                    self.assertIn("inputs.runner_pool == 'self'", runs_on)
                    self.assertTrue(
                        _self_condition_true_on_push(runs_on),
                        f"{name} {job_name} can resolve to hosted on push: {runs_on}",
                    )
        self.assertGreater(found, 0, "no push-triggered release workflow found")

    def test_push_triggered_pool_steps_follow_push(self):
        checked = 0
        for name in SIX:
            doc = load(name)
            on = doc.get("on", doc.get(True)) or {}
            tags = ((on.get("push") or {}).get("tags")) or []
            if not tags:
                continue
            for job_name, job in doc["jobs"].items():
                for idx, step in enumerate(job.get("steps") or []):
                    if not isinstance(step, dict):
                        continue
                    cond = step.get("if", "")
                    if not isinstance(cond, str) or "inputs.runner_pool" not in cond:
                        continue
                    if "create_release" in cond or "publish_npm" in cond:
                        continue
                    checked += 1
                    with self.subTest(f"{name} {job_name} step {idx}"):
                        self.assertIn("github.event_name", cond)
                        result = _step_condition_on_push(cond)
                        if result is None:
                            continue
                        if "== 'self'" in cond:
                            self.assertTrue(result, f"self branch dead on push: {cond}")
                        elif "!= 'self'" in cond:
                            self.assertFalse(
                                result, f"hosted branch live on push: {cond}"
                            )
        self.assertGreater(
            checked, 0, "no pool-switched step found in a push-triggered workflow"
        )


if __name__ == "__main__":
    unittest.main()
