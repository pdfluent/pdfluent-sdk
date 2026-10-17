#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No bash step on a Windows leg runs under the default shell.

The default `run` shell on a Windows runner is pwsh. A step whose body is
bash -- `[[ ... ]]`, `$(...)`, `export`, `set -e` -- dies there with a
ParserError, as the natives version check did in release-natives run
37451377812 ("Missing type name after '['"). Our Windows runners carry Git
Bash first on PATH, so `shell: bash` is the fix, and this guard holds it.

What it asks, of the same five release workflows the runner-pool routing
test reads: every step that (a) can run on a Windows leg of either pool,
(b) carries a bash-only marker, and (c) resolves to the default shell --
no step-level `shell:`, no job- or workflow-level `defaults.run.shell` --
is a violation. A step with an explicit `shell:` (bash or the deliberate
powershell of the MinGW install) passes unjudged.

Two deliberate simplifications, both documented so they can be revisited:

WINDOWS LEG. A matrix leg is a Windows leg when any of its values names
Windows (`windows-latest`, a self label list holding `windows`, a triple
or tag holding `windows`/`win32`/`win-...`). A plain job is one when its
`runs-on` does. `darwin` is not Windows: the `win-` token only matches at
the start of a value.

NARROWED AWAY. A step whose `if:` pins `runner.os` with `==` to a value
that does not name Windows, or pins `matrix.*` with `==` to literals none
of which names a Windows leg value, cannot run on a Windows leg and is
skipped. `!=` keeps the leg reachable (`runner.os != 'Linux'` runs on
Windows). Anything subtler -- `contains()`, `&&` chains -- is treated as
reachable: louder is safer than quieter here.

The bash markers are substrings, not a parser: `[[`, `$(`, `export `,
`set -e`, `<<` (heredoc), and `||` together with `exit`. Plain commands
(`npm install`, `pip install`, `cargo install`) carry none and pass, on
any shell.
"""
from __future__ import annotations

import pathlib
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

# The jobs the scan must find a Windows leg in. A leg detection that sees
# no Windows anywhere reports a clean tree in the same words as a clean
# tree; this floor keeps that quiet.
WINDOWS_LEGGED_JOBS = {
    "release-natives.yml": ["natives", "smoke"],
    "release-binaries.yml": ["build"],
    "node-bindings.yml": ["build-and-test", "build-release"],
}


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

    Only `==` narrows: `runner.os == 'Linux'`, or a `matrix.*` comparison
    whose literals name no Windows leg value (the musl/gnu cross-linker
    pins, the `use_cross` pin). `!=` and `contains()` stay reachable.
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


def bash_markers(run: str) -> list[str]:
    """The bash-only markers this body carries, if any (see docstring)."""
    found = []
    if "[[" in run:
        found.append("[[")
    if "$(" in run:
        found.append("$(")
    if "export " in run or "export\t" in run:
        found.append("export")
    if "set -e" in run:
        found.append("set -e")
    if "<<" in run:
        found.append("<< (heredoc)")
    if "||" in run and "exit" in run:
        found.append("|| ... exit")
    return found


def violations_in(doc, name: str) -> list[str]:
    """Every bash step on a Windows leg that resolves to the default shell."""
    violations = []
    for job_name, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict) or not job_has_windows_leg(job):
            continue
        for i, step in enumerate(job.get("steps") or []):
            if not isinstance(step, dict) or not isinstance(step.get("run"), str):
                continue
            if resolved_shell(step, job, doc) is not None:
                continue
            if narrowed_away_from_windows(step):
                continue
            markers = bash_markers(step["run"])
            if not markers:
                continue
            label = step.get("name") or step.get("uses") or f"step {i}"
            violations.append(
                f"{name}: {job_name} :: {label} :: bash-only "
                f"({', '.join(markers)}) under the default shell"
            )
    return violations


def scan(workflows_dir: pathlib.Path) -> list[str]:
    """The violations across the five release workflows under this dir."""
    violations = []
    for name in FIVE:
        doc = yaml.safe_load((workflows_dir / name).read_text())
        violations.extend(violations_in(doc, name))
    return violations


class WindowsShellTests(unittest.TestCase):
    def test_tree_has_no_bash_step_under_the_default_shell(self):
        self.assertEqual(scan(WORKFLOWS), [])

    def test_the_scan_sees_every_windows_leg(self):
        for name, jobs in WINDOWS_LEGGED_JOBS.items():
            doc = yaml.safe_load((WORKFLOWS / name).read_text())
            for job_name in jobs:
                with self.subTest(f"{name} {job_name}"):
                    self.assertTrue(job_has_windows_leg(doc["jobs"][job_name]))
        doc = yaml.safe_load((WORKFLOWS / "wasm.yml").read_text())
        for job_name, job in doc["jobs"].items():
            with self.subTest(f"wasm.yml {job_name}"):
                self.assertFalse(job_has_windows_leg(job))

    def test_a_bash_step_without_shell_is_flagged(self):
        doc = {
            "jobs": {
                "natives": {
                    "runs-on": "${{ matrix.runs-on }}",
                    "strategy": {
                        "matrix": {"include": [{"runs-on": "windows-latest"}]}
                    },
                    "steps": [
                        {
                            "name": "The version input is x.y.z",
                            "run": '[[ "$VERSION" =~ ^[0-9]+ ]] || exit 1',
                        }
                    ],
                }
            }
        }
        violations = violations_in(doc, "test.yml")
        self.assertEqual(len(violations), 1)
        self.assertIn("natives :: The version input is x.y.z", violations[0])
        self.assertIn("[[", violations[0])

    def test_each_marker_is_flagged(self):
        bodies = {
            "[[ $A == b ]]": "[[",
            "OUT=$(some-command)": "$(",
            "export FOO=1": "export",
            "set -euo pipefail": "set -e",
            "cat <<'EOF'\nbody\nEOF": "<< (heredoc)",
            "test -n \"$X\" || exit 1": "|| ... exit",
        }
        for body, marker in bodies.items():
            with self.subTest(marker):
                doc = {
                    "jobs": {
                        "build": {
                            "runs-on": "windows-latest",
                            "steps": [{"run": body}],
                        }
                    }
                }
                violations = violations_in(doc, "test.yml")
                self.assertEqual(len(violations), 1)
                self.assertIn(marker, violations[0])

    def test_an_explicit_shell_passes(self):
        for shell in ("bash", "powershell"):
            with self.subTest(shell):
                doc = {
                    "jobs": {
                        "build": {
                            "runs-on": "windows-latest",
                            "steps": [
                                {
                                    "shell": shell,
                                    "run": '[[ "$A" == b ]] || exit 1',
                                }
                            ],
                        }
                    }
                }
                self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_job_default_shell_passes(self):
        doc = {
            "jobs": {
                "smoke": {
                    "runs-on": "windows-latest",
                    "defaults": {"run": {"shell": "bash"}},
                    "steps": [{"run": '[[ "$A" == b ]] || exit 1'}],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_plain_commands_pass_on_any_shell(self):
        bodies = (
            "npm install",
            "npm run build",
            "pip install foo",
            "cargo install cross --locked",
        )
        for body in bodies:
            with self.subTest(body):
                doc = {
                    "jobs": {
                        "build": {
                            "runs-on": "windows-latest",
                            "steps": [{"run": body}],
                        }
                    }
                }
                self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_a_linux_only_job_is_ignored(self):
        doc = {
            "jobs": {
                "package": {
                    "runs-on": "ubuntu-latest",
                    "steps": [{"run": '[[ "$A" == b ]] || exit 1'}],
                }
            }
        }
        self.assertEqual(violations_in(doc, "test.yml"), [])

    def test_narrowed_steps_are_ignored(self):
        for cond in (
            "runner.os == 'Linux'",
            "matrix.target == 'x86_64-unknown-linux-musl'",
            "matrix.use_cross == true",
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
                                    "run": '[[ "$A" == b ]] || exit 1',
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
                            "run": '[[ "$A" == b ]] || exit 1',
                        }
                    ],
                }
            }
        }
        self.assertEqual(len(violations_in(doc, "test.yml")), 1)

    def test_darwin_is_not_windows(self):
        self.assertFalse(mentions_windows("darwin-x64"))
        self.assertFalse(mentions_windows("macos-latest"))
        self.assertTrue(mentions_windows("windows-latest"))
        self.assertTrue(mentions_windows(["self-hosted", "windows", "x64"]))
        self.assertTrue(mentions_windows("x86_64-pc-windows-msvc"))
        self.assertTrue(mentions_windows("win32-x64-msvc"))

    def test_the_mingw_install_stays_powershell(self):
        # The one deliberate pwsh step across the five: the github-pool
        # MinGW install runs choco, so it names its shell on purpose and
        # this guard must never ask for bash there.
        doc = yaml.safe_load((WORKFLOWS / "release-binaries.yml").read_text())
        steps = doc["jobs"]["build"]["steps"]
        installs = [
            s
            for s in steps
            if isinstance(s, dict)
            and s.get("name") == "Install MinGW for the windows-gnu target (github)"
        ]
        self.assertEqual(len(installs), 1)
        self.assertEqual(installs[0].get("shell"), "powershell")


if __name__ == "__main__":
    unittest.main()
