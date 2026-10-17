#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The Rosetta wrapper runs a command on the runner's x64 tools, or refuses.

rosetta_x64.sh refuses unless on Darwin with Rosetta available and
$PDFLUENT_TOOLS holding the four x64 installs; it runs the command under
arch -x86_64 with a PATH whose first entries are the x64 python, node,
java and dotnet dirs, then /usr/bin:/bin, and JAVA_HOME/DOTNET_ROOT on
the x64 sides. Before the command it logs uname -m, the x64 python's
platform.machine() and the x64 node's process.arch, refusing unless they
read x86_64/x64.

The static cases run everywhere; the stub-tree cases run only on Darwin,
where arch -x86_64 exists.
"""
from __future__ import annotations

import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "release" / "rosetta_x64.sh"

MINIMUM_CASES = 12  # FLOOR

ON_DARWIN = sys.platform == "darwin"


def _code(text: str) -> str:
    return "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("#")
    )


def _write_stub(path: pathlib.Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\n" + body + "\n")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def _stub_tree(root: pathlib.Path, python_arch: str = "x86_64") -> None:
    _write_stub(
        root / "python-3.11-x64" / "bin" / "python3", f"echo {python_arch}"
    )
    _write_stub(root / "node-x64" / "bin" / "node", "echo x64")
    _write_stub(root / "temurin-17-x64" / "bin" / "javac", "true")
    _write_stub(root / "dotnet-x64" / "dotnet", "true")


def _run(
    *args: str, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    run_env = dict(os.environ) if env is None else env
    return subprocess.run(
        ["bash", str(SCRIPT), *args],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=REPO,
        env=run_env,
    )


class RosettaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = SCRIPT.read_text()
        cls.code = _code(cls.text)

    def test_release_natives_runs_every_x64_runtime_smoke_under_rosetta(self):
        # The osx-x64 leg is cross-built on the arm64 Mac: each consumer smoke
        # that loads the x64 artefact into a runtime (or links it into a C
        # program) must go through the wrapper on the self pool.
        import yaml

        flow = yaml.safe_load((REPO / ".github" / "workflows" / "release-natives.yml").read_text())
        steps = {s.get("name"): str(s.get("run") or "") for s in flow["jobs"]["smoke"]["steps"] if isinstance(s, dict)}
        for name in ("Smoke python", "Smoke node", "Smoke java", "Smoke dotnet", "Smoke capi"):
            run = steps.get(name, "")
            self.assertIn("scripts/release/rosetta_x64.sh", run, f"{name} has no Rosetta path")
            self.assertIn('"osx-x64"', run, f"{name}: the wrapper must be gated on osx-x64")
            self.assertIn('"$RUNNER_POOL" == "self"', run, f"{name}: the wrapper must be gated on the self pool")
            env = next(s.get("env") or {} for s in flow["jobs"]["smoke"]["steps"] if isinstance(s, dict) and s.get("name") == name)
            # The pool arrives as data through env:, never pasted into the
            # script (scripts/ci/workflow_inputs_are_not_shell.py).
            self.assertEqual(env.get("RUNNER_POOL"), "${{ inputs.runner_pool }}", f"{name}: RUNNER_POOL must come from env")

    def test_the_c_smoke_compiles_for_the_platform_arch(self):
        # macOS cc targets the hardware even under arch -x86_64: run
        # 37355713891 built an arm64 program for the osx-x64 dylib.
        text = (REPO / "scripts" / "release" / "consumer_smokes" / "smoke_capi.sh").read_text()
        self.assertIn('osx-x64) ARCH_FLAG="-arch x86_64"', text)
        self.assertIn('osx-arm64) ARCH_FLAG="-arch arm64"', text)
        self.assertRegex(text, r'"\$CC_BIN" [^\n]*\$ARCH_FLAG')

    def test_refuses_off_darwin(self):
        self.assertIn('"$(uname -s)" = "Darwin"', self.code)

    def test_requires_rosetta(self):
        self.assertIn("arch -x86_64 /usr/bin/true", self.code)

    def test_path_starts_with_the_x64_dirs(self):
        path_line = next(
            line
            for line in self.code.splitlines()
            if line.startswith("new_path=") and "/usr/bin:/bin" not in line
        )
        self.assertLess(path_line.index("$py_x64/bin"), path_line.index("$node_x64/bin"))
        self.assertLess(path_line.index("$node_x64/bin"), path_line.index("$java_x64/bin"))
        self.assertLess(path_line.index("$java_x64/bin"), path_line.index("$dotnet_x64"))
        tail = next(
            line
            for line in self.code.splitlines()
            if line.startswith("new_path=") and "/usr/bin:/bin" in line
        )
        self.assertTrue(tail.rstrip().endswith(":/usr/bin:/bin\""))

    def test_sets_the_x64_roots(self):
        self.assertIn('export JAVA_HOME="$java_x64"', self.code)
        self.assertIn('export DOTNET_ROOT="$dotnet_x64"', self.code)

    def test_self_checks_judge_arch_python_and_node(self):
        self.assertIn("arch -x86_64 uname -m", self.code)
        self.assertIn("platform.machine()", self.code)
        self.assertIn("node -p process.arch", self.code)
        self.assertIn('"$machine" = "x86_64"', self.code)
        self.assertIn('"$py_machine" = "x86_64"', self.code)
        self.assertIn('"$node_arch" = "x64"', self.code)

    def test_runs_the_command_under_rosetta(self):
        self.assertIn('exec arch -x86_64 "$@"', self.code)

    def test_darwin_gate_reads_uname(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = pathlib.Path(tmp) / "bin"
            _write_stub(fake / "uname", "echo Linux")
            env = dict(os.environ)
            env["PATH"] = f"{fake}{os.pathsep}{env.get('PATH', '')}"
            proc = _run("true", env=env)
        self.assertEqual(proc.returncode, 2)
        self.assertIn("refusing on Linux", proc.stderr)

    @unittest.skipUnless(ON_DARWIN, "arch -x86_64 exists only on Darwin")
    def test_refuses_without_the_tool_tree(self):
        env = dict(os.environ)
        env.pop("PDFLUENT_TOOLS", None)
        proc = _run("true", env=env)
        self.assertEqual(proc.returncode, 2)
        self.assertIn("PDFLUENT_TOOLS is unset", proc.stderr)
        with tempfile.TemporaryDirectory() as tmp:
            env["PDFLUENT_TOOLS"] = tmp
            proc = _run("true", env=env)
            self.assertEqual(proc.returncode, 2)
            self.assertIn("no x64 python", proc.stderr)

    @unittest.skipUnless(ON_DARWIN, "arch -x86_64 exists only on Darwin")
    def test_stub_tree_runs_the_command_with_the_x64_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _stub_tree(root)
            env = dict(os.environ)
            env["PDFLUENT_TOOLS"] = str(root)
            proc = _run(
                "sh",
                "-c",
                "echo PATH=$PATH; echo JAVA_HOME=$JAVA_HOME DOTNET_ROOT=$DOTNET_ROOT",
                env=env,
            )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        first, *_, usr, system = proc.stdout.split("PATH=")[1].splitlines()[0].split(":")
        self.assertEqual(
            (first, usr, system),
            (f"{root}/python-3.11-x64/bin", "/usr/bin", "/bin"),
        )
        self.assertIn(f"JAVA_HOME={root}/temurin-17-x64", proc.stdout)
        self.assertIn(f"DOTNET_ROOT={root}/dotnet-x64", proc.stdout)

    @unittest.skipUnless(ON_DARWIN, "arch -x86_64 exists only on Darwin")
    def test_stub_tree_refuses_a_wrong_arch_python(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _stub_tree(root, python_arch="arm64")
            env = dict(os.environ)
            env["PDFLUENT_TOOLS"] = str(root)
            proc = _run("true", env=env)
        self.assertEqual(proc.returncode, 2)
        self.assertIn("not x86_64", proc.stderr)


if __name__ == "__main__":
    unittest.main()
