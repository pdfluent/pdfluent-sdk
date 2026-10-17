#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
r"""No Windows-reachable bash step calls a bare `python3`.

The python.org installer the self-hosted setup runs provides `python.exe`
(and `py`), not `python3.exe` — and GitHub-hosted windows runners may lack
`python3` too. Build 3 died on it twice: the binaries `Package binary` step
(`zip_cli_stage.py`) and the natives `Build the wheel` step
(`fix_python_wheel_paths.py`) both ran `line 24: python3: command not found`
on win-x64.

So every bash step that can run on a Windows leg of either pool resolves
its interpreter first:

    PY="$(command -v python3 || command -v python)"

and calls `"$PY"` instead of a bare `python3`. This guard holds that: of
the same five release workflows the Windows-shell test reads, every step
that (a) can run on a Windows leg, (b) runs under bash, and (c) names a
bare `python3` command without the `command -v python3` fallback in the
same body is a violation. Linux-only steps keep their bare `python3`:
unfailing code is not rewritten for uniformity.

WINDOWS LEG and NARROWED AWAY are the Windows-shell test's rules,
repeated here so this file reads alone: a matrix leg is a Windows leg
when any of its values names Windows, and a step whose `if:` pins
`runner.os` or `matrix.*` with `==` to literals naming no Windows leg
cannot run on one. `!=` and `contains()` stay reachable.

A bare call is `python3` as a command — at the start of a line or after
`;&|(\`` or whitespace, followed by whitespace or end of line — with
`command -v python3` occurrences removed first, so the fallback itself
is never the violation. Comments (`#` after whitespace or at line
start) are stripped, so `# python3` in a comment passes. Package names
(`apk add ... python3`) match the same shape as a call; the one such
line in the five sits in a musl-only step, narrowed away from Windows,
so it never reaches the scan — a second one on a Windows leg would be
flagged, loudly, for a human to exempt.

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

# `python3` as a command, after the fallback occurrences are removed.
BARE_PYTHON3 = re.compile(r"(?:^|[;&|(\s`])python3(?:\s|$)")

# The fallback every Windows-reachable caller carries.
FALLBACK = "command -v python3"


def mentions_windows(value) -> bool:
    """Does this matrix value or condition text name a Windows leg?"""
    if isinstance(value, str):
        lowered = value.lower()
        return (
            "windows" in lowered
            or "win32" in lowered
            or lowered.startswith("win-")
            or "-win-" in lowered
        )
    if isinstance(value, list):
        return any(mentions_windows(v) for v in value)
    return False


def job_has_windows_leg(job) -> bool:
    """Does any leg of this job run on Windows, on either pool?"""
    matrix = (job.get("strategy") or {}).get("matrix") or {}
    entries = matrix.get("include") or []
    if entries:
        return any(
            any(mentions_windows(v) for v in entry.values())
            for entry in entries
            if isinstance(entry, dict)
        )
    return mentions_windows(job.get("runs-on"))


def narrowed_away_from_windows(step) -> bool:
    """Does the step's `if:` pin it off every Windows leg?

    Only `==` narrows: `runner.os == 'Linux'`, or a `matrix.*`
    comparison whose literals name no Windows leg value. `!=` and
    `contains()` stay reachable.
    """
    cond = step.get("if")
    if not isinstance(cond, str) or "==" not in cond:
        return False
    if "runner.os" not in cond and "matrix." not in cond:
        return False
    return not mentions_windows(cond)


def resolved_shell(step, job, doc):
    """The shell this step runs under, or None for the runner default."""
    if step.get("shell"):
        return step["shell"]
    for scope in (job, doc):
        run = ((scope.get("defaults") or {}).get("run")) or {}
        if isinstance(run, dict) and run.get("shell"):
            return run["shell"]
    return None


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


def bare_calls(run: str) -> list[str]:
    """The lines calling a bare `python3` without the fallback nearby."""
    body = code_lines(run)
    if FALLBACK in body:
        body = body.replace(FALLBACK, "")
    return [line for line in body.splitlines() if BARE_PYTHON3.search(line)]


def violations_in(doc, name: str) -> list[str]:
    """Every Windows-reachable bash step calling a bare `python3`."""
    violations = []
    for job_name, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict) or not job_has_windows_leg(job):
            continue
        for i, step in enumerate(job.get("steps") or []):
            if not isinstance(step, dict) or not isinstance(step.get("run"), str):
                continue
            shell = resolved_shell(step, job, doc)
            if shell is not None and shell != "bash":
                continue
            if narrowed_away_from_windows(step):
                continue
            calls = bare_calls(step["run"])
            if not calls:
                continue
            label = step.get("name") or step.get("uses") or f"step {i}"
            violations.append(
                f"{name}: {job_name} :: {label} :: bare python3 "
                f"({calls[0].strip()})"
            )
    return violations


def scan(workflows_dir: pathlib.Path) -> list[str]:
    """The violations across the five release workflows under this dir."""
    violations = []
    for name in FIVE:
        doc = yaml.safe_load((workflows_dir / name).read_text())
        violations.extend(violations_in(doc, name))
    return violations


class WindowsPythonTests(unittest.TestCase):
    def test_tree_has_no_bare_python3_on_a_windows_leg(self):
        self.assertEqual(scan(WORKFLOWS), [])

    def test_a_bare_call_on_a_windows_leg_is_flagged(self):
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "windows-latest",
                    "steps": [
                        {
                            "name": "Package binary",
                            "shell": "bash",
                            "run": "python3 scripts/release/zip_cli_stage.py a b",
                        }
                    ],
                }
            }
        }
        violations = violations_in(doc, "test.yml")
        self.assertEqual(len(violations), 1)
        self.assertIn("build :: Package binary", violations[0])

    def test_the_fallback_passes(self):
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "windows-latest",
                    "steps": [
                        {
                            "shell": "bash",
                            "run": 'PY="$(command -v python3 || command -v python)"\n'
                            '"$PY" scripts/release/zip_cli_stage.py a b',
                        }
                    ],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_comment_passes(self):
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "windows-latest",
                    "steps": [{"shell": "bash", "run": "# python3 is resolved above\necho ok"}],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_powershell_step_is_ignored(self):
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "windows-latest",
                    "steps": [
                        {"shell": "powershell", "run": "python3 foo.py"},
                    ],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_linux_only_job_is_ignored(self):
        doc = {
            "jobs": {
                "package": {
                    "runs-on": "ubuntu-latest",
                    "steps": [{"shell": "bash", "run": "python3 foo.py"}],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_narrowed_steps_are_ignored(self):
        for cond in (
            "runner.os == 'Linux'",
            "matrix.target == 'x86_64-unknown-linux-musl'",
        ):
            with self.subTest(cond):
                doc = {
                    "jobs": {
                        "build": {
                            "runs-on": "${{ matrix.os }}",
                            "strategy": {
                                "matrix": {"include": [{"os": "windows-latest"}]}
                            },
                            "steps": [
                                {
                                    "if": cond,
                                    "shell": "bash",
                                    "run": "python3 foo.py",
                                }
                            ],
                        }
                    }
                }
                self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_not_equals_condition_stays_reachable(self):
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "${{ matrix.os }}",
                    "strategy": {"matrix": {"include": [{"os": "windows-latest"}]}},
                    "steps": [
                        {
                            "if": "runner.os != 'Linux'",
                            "shell": "bash",
                            "run": "python3 foo.py",
                        }
                    ],
                }
            }
        }
        self.assertEqual(len(violations_in(doc, "test.yml")), 1)

    def test_python_without_a_3_passes(self):
        # `python` is what the fallback resolves to on Windows; it is
        # never itself the violation.
        doc = {
            "jobs": {
                "build": {
                    "runs-on": "windows-latest",
                    "steps": [{"shell": "bash", "run": "python -m build ."}],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])


if __name__ == "__main__":
    unittest.main()
