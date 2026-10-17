#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""orchestration-guard stays seconds of file scanning: no build steps.

The three binding suites (maturin develop, napi build, npm install) lived in
orchestration-guard behind a PUSH_BEFORE diff — and on a new branch, where
before=0000, the diff matches nothing and the light guard job compiled the
engine. They have their own job now. Both halves are held here: nothing that
compiles runs in the guard, and the suites still run in another job, so the
move cannot silently become a deletion. (A fourth suite, the pymupdf4llm
parity run, stood here until 1.1.0 shipped without the front door.)
"""
from __future__ import annotations

import pathlib
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".github" / "workflows" / "ci.yml"
# The compiling suites moved here, dispatch-only, off the desktop (#350).
# The third test searches both files: the suites must run somewhere outside
# the guard, whichever file holds them.
SUITES_WORKFLOW = REPO / ".github" / "workflows" / "binding-suites.yml"

MINIMUM_CASES = 3  # FLOOR

# The three compiling suites, by the script each step runs.
HEAVY_SUITES = (
    "scripts/release/run_office_tests.sh",
    "scripts/release/run_extract_tests.sh",
    "scripts/release/run_extract_js_tests.sh",
)

# What must never be inlined back into the guard either: the tools the
# suites are made of. Matched against run: lines with full-line comments
# stripped, so a comment pointing at binding-suites does not trip it.
HEAVY_TOOLS = ("maturin", "npm install", "napi build", "cargo build", "cargo test")


def _doc() -> dict:
    return yaml.safe_load(WORKFLOW.read_text())


def _code_lines(run: str) -> list[str]:
    return [
        line for line in run.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]


class OrchestrationGuardStaysLightTests(unittest.TestCase):
    def test_guard_runs_none_of_the_compiling_suites(self):
        doc = _doc()
        self.assertIn("orchestration-guard", doc["jobs"])
        runs = "\n".join(
            str(s.get("run", ""))
            for s in doc["jobs"]["orchestration-guard"]["steps"]
            if isinstance(s, dict)
        )
        for suite in HEAVY_SUITES:
            self.assertNotIn(
                suite, runs,
                f"orchestration-guard runs {suite}: the compiling suites "
                "live in binding-suites",
            )

    def test_guard_inlines_no_build_tool_either(self):
        doc = _doc()
        code = "\n".join(
            "\n".join(_code_lines(str(s.get("run", ""))))
            for s in doc["jobs"]["orchestration-guard"]["steps"]
            if isinstance(s, dict)
        )
        for tool in HEAVY_TOOLS:
            self.assertNotIn(
                tool, code,
                f"orchestration-guard inlines {tool!r}: this job is seconds "
                "of file scanning, no compilation",
            )

    def test_each_suite_still_runs_in_another_job(self):
        docs = [_doc(), yaml.safe_load(SUITES_WORKFLOW.read_text())]
        for suite in HEAVY_SUITES:
            elsewhere = [
                name for doc in docs for name, job in doc["jobs"].items()
                if name != "orchestration-guard"
                and isinstance(job, dict)
                and any(
                    suite in str(s.get("run", ""))
                    for s in (job.get("steps") or [])
                    if isinstance(s, dict)
                )
            ]
            self.assertTrue(
                elsewhere,
                f"{suite} runs nowhere outside orchestration-guard: the move "
                "must not become a deletion",
            )


if __name__ == "__main__":
    unittest.main()
