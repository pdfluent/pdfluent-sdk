#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
r"""No release step reaches for `gh`, and none pipes into `grep -q`.

The `gh` CLI is not installed on the self-hosted runners, by design —
build 3's natives smokes all reported `gh: command not found` and every
leg, including the five that built, SKIPPED itself. And `grep -q` under
`set -o pipefail` fails the pipe even where `gh` exists: the early exit
is EPIPE. What those steps did through `gh` they now do over curl
against `$GITHUB_API_URL` with `$GH_TOKEN`, parsed by python —
capture-then-compare, never a pipe into `grep -q`.

This holds both halves, of the same five release workflows the
runner-pool routing test reads: every step whose `run:` body (a) calls
`gh` as a command, or (b) pipes into `grep -q`/`grep -qx`, is a
violation. Comments (`#` at line start or after whitespace) are
stripped first, so `# gh` in a comment passes; `gh` inside a longer
word (`high`) is not a call. A plain `grep pattern file` without `-q`
passes: it is the quiet flag in a pipefail pipeline that kills the
pipe, not grep itself.

WORKFLOWS_DIR points the suite at another workflows tree; default is
the tree's .github/workflows.
"""
from __future__ import annotations

import os
import pathlib
import re
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = pathlib.Path(
    os.environ.get("WORKFLOWS_DIR", REPO / ".github" / "workflows")
)

FIVE = [
    "release-natives.yml",
    "release-binaries.yml",
    "node-bindings.yml",
    "wasm.yml",
    "release-smoke.yml",
]

# `gh` as a command: at the start of a line or after a separator,
# followed by whitespace or end of line. `high`, `through` and
# `action-gh-release` never match: the char before is a word char.
GH_CALL = re.compile(r"(?:^|[;&|(\s`])gh(?:\s|$)")

# A pipe into a quiet grep: the EPIPE trap under pipefail.
QUIET_GREP_PIPE = re.compile(r"\|\s*grep\s+-[a-zA-Z]*q")


def code_lines(run: str) -> str:
    """The body without its comments (`#` at line start or after space)."""
    kept = []
    for line in run.splitlines():
        cut = None
        for i, ch in enumerate(line):
            if ch == "#" and (i == 0 or line[i - 1] in " \t"):
                cut = i
                break
        kept.append(line[:cut] if cut is not None else line)
    return "\n".join(kept)


def violations_in(doc, name: str) -> list[str]:
    """Every step calling `gh` or piping into a quiet grep."""
    violations = []
    for job_name, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        for i, step in enumerate(job.get("steps") or []):
            if not isinstance(step, dict) or not isinstance(step.get("run"), str):
                continue
            label = step.get("name") or step.get("uses") or f"step {i}"
            for line in code_lines(step["run"]).splitlines():
                if GH_CALL.search(line):
                    violations.append(
                        f"{name}: {job_name} :: {label} :: gh call "
                        f"({line.strip()[:90]})"
                    )
                elif QUIET_GREP_PIPE.search(line):
                    violations.append(
                        f"{name}: {job_name} :: {label} :: pipe into "
                        f"quiet grep ({line.strip()[:90]})"
                    )
    return violations


def scan(workflows_dir: pathlib.Path) -> list[str]:
    """The violations across the five release workflows under this dir."""
    violations = []
    for name in FIVE:
        doc = yaml.safe_load((workflows_dir / name).read_text())
        violations.extend(violations_in(doc, name))
    return violations


class NoGhTests(unittest.TestCase):
    def test_tree_has_no_gh_call_and_no_quiet_grep_pipe(self):
        self.assertEqual(scan(WORKFLOWS), [])

    def test_a_gh_call_is_flagged(self):
        doc = {
            "jobs": {
                "smoke": {
                    "steps": [
                        {
                            "name": "Skip when this leg was never built",
                            "run": 'if gh api "repos/x/actions/runs/1/artifacts" | grep -qx foo; then',
                        }
                    ]
                }
            }
        }
        violations = violations_in(doc, "test.yml")
        # One line, one violation: the gh call is reported first.
        self.assertEqual(len(violations), 1)
        self.assertIn("gh call", violations[0])

    def test_a_quiet_grep_pipe_without_gh_is_flagged(self):
        doc = {
            "jobs": {
                "smoke": {
                    "steps": [
                        {"run": "curl -fsSL https://x | grep -q present"},
                    ]
                }
            }
        }
        violations = violations_in(doc, "test.yml")
        self.assertEqual(len(violations), 1)
        self.assertIn("quiet grep", violations[0])

    def test_gh_in_a_word_or_a_comment_passes(self):
        doc = {
            "jobs": {
                "smoke": {
                    "steps": [
                        {
                            "run": "# no gh on these runners, by design\n"
                            "echo high throughough\n"
                            "uses: softprops/action-gh-release@v2",
                        },
                    ]
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_loud_grep_passes(self):
        doc = {
            "jobs": {
                "smoke": {
                    "steps": [
                        {"run": "names=$(grep -m1 '^version = ' Cargo.toml | cut -d'\"' -f2)"},
                    ]
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_curl_capture_then_compare_passes(self):
        doc = {
            "jobs": {
                "smoke": {
                    "steps": [
                        {
                            "run": "curl -fsSL -H \"Authorization: Bearer $GH_TOKEN\" "
                            "https://x -o body\n"
                            '"$PY" -c \'import json,sys\' body\n'
                            'test -n "$names" || exit 1',
                        },
                    ]
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])


if __name__ == "__main__":
    unittest.main()
