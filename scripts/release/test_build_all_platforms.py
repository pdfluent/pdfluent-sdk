#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every build-lane channel gates what it built.

The lane used to count three wheels against six platforms and report
"partial (3/6)" forever, judging nothing. Now each channel runs the
artefact gate in --allow-missing mode: what is built is judged and the
rest is named as `not built: ...` lines. Java and dotnet keep their
unconditional gates because their staging plus the pom enforcer refuse a
partial set first.

And the lane itself has a job: build-lane.yml dispatches it by channel,
so the script is exercised by the pipeline rather than living as dead
code with a by-hand excuse.
"""
from __future__ import annotations

import json
import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
LANE = REPO / "scripts" / "release" / "build_all_platforms.sh"
WORKFLOW = REPO / ".github" / "workflows" / "build-lane.yml"
GUARD_JOBS = REPO / "scripts" / "ci" / "every_guard_has_a_job.py"
NODE_PKG = REPO / "crates" / "pdf-node" / "package.json"

MINIMUM_CASES = 5  # FLOOR


def _sections() -> dict[str, str]:
    text = LANE.read_text()
    marks = [m for m in re.finditer(r"^# -+ (\w+) -+$", text, re.M)]
    sections = {}
    for i, mark in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
        sections[mark.group(1)] = text[mark.start() : end]
    return sections


class BuildLaneGateTests(unittest.TestCase):
    def test_partial_channels_gate_the_built_set_with_allow_missing(self):
        # Have/want counting is gone: the lane gates what it built and the
        # gate names the rest as `not built: ...` lines. Counting three
        # wheels against six platforms reported "partial (3/6)" forever
        # without judging anything.
        text = LANE.read_text()
        self.assertNotIn("gate_when_complete", text)
        self.assertNotIn("count_files", text)
        sections = _sections()
        for channel, flag in (
            ("python", "--wheels"),
            ("node", "--node-dir"),
            ("capi", "--capi-dir"),
        ):
            body = sections[channel]
            self.assertIn("artifacts_cover_every_platform.py", body, channel)
            self.assertIn(flag, body, channel)
            self.assertIn("--allow-missing", body, channel)

    def test_complete_by_construction_channels_gate_unconditionally(self):
        sections = _sections()
        self.assertIn("stage_java_natives", sections["java"])
        self.assertIn("--jar", sections["java"])
        self.assertIn("maven_channel_guard", sections["java"])
        self.assertIn("stage_dotnet_natives", sections["dotnet"])
        self.assertIn("--nupkg", sections["dotnet"])

    def test_lane_lists_no_channel_without_a_gate(self):
        sections = _sections()
        for channel in ("python", "node", "capi", "java", "dotnet"):
            self.assertIn(
                "artifacts_cover",
                sections[channel],
                f"{channel} builds but never gates",
            )

    def test_python_lane_builds_the_sdist_beside_the_wheels(self):
        # The gate requires exactly one sdist beside the wheels, lane mode
        # included — so the lane builds it before it gates.
        body = _sections()["python"]
        self.assertIn("build_pdfluent_sdist.sh", body)
        self.assertLess(
            body.index("build_pdfluent_sdist.sh"),
            body.index("--wheels"),
        )


def _napi_cli_major() -> int:
    """The major version of the pinned @napi-rs/cli."""
    pkg = json.loads(NODE_PKG.read_text())
    pinned = pkg["devDependencies"]["@napi-rs/cli"]
    return int(re.search(r"(\d+)", pinned).group(1))


def _logical_lines(text: str) -> list[str]:
    """Shell lines with backslash continuations joined."""
    out = []
    buf = ""
    for line in text.splitlines():
        stripped = line.rstrip()
        if stripped.endswith("\\"):
            buf += stripped[:-1]
        else:
            out.append(buf + line)
            buf = ""
    if buf:
        out.append(buf)
    return out


class NapiArgsTests(unittest.TestCase):
    def test_no_output_dir_beside_napi_build_on_cli_2x(self):
        # The 2.x CLI takes the destination as a positional [destDir];
        # --output-dir beside napi build is an "Unsupported option name"
        # and the lane dies before it compiles. Held only while the
        # pinned CLI is 2.x: a 3.x pin may rename the flag again, and
        # this test must not forbid what it has not measured.
        if _napi_cli_major() != 2:
            self.skipTest("pinned napi CLI is not 2.x")
        sources = [LANE, *sorted((REPO / ".github" / "workflows").glob("*.yml"))]
        offenders = []
        for src in sources:
            for line in _logical_lines(src.read_text()):
                if "napi build" in line and "--output-dir" in line:
                    offenders.append(f"{src.name}: {line.strip()}")
        self.assertEqual(offenders, [])

    def test_lane_node_channel_passes_a_positional_dest_dir(self):
        # The shape that parses: napi build with a trailing path, not a
        # flag, in the lane's node section.
        if _napi_cli_major() != 2:
            self.skipTest("pinned napi CLI is not 2.x")
        body = _sections()["node"]
        builds = [
            line
            for line in _logical_lines(body)
            if "napi build" in line
        ]
        self.assertEqual(len(builds), 1, builds)
        self.assertNotIn("--output", builds[0])
        self.assertRegex(builds[0], r"\. ?\)?$")


def _run_lane(
    channel: str, installed_targets: str, *, check_only: bool = False
) -> "subprocess.CompletedProcess[str]":
    """Run the lane with stubbed tools and a canned `rustup target list`.

    Every tool the lane shells out to is a stub, so the case needs nothing
    installed and builds nothing: the check-only mode stops after the
    prerequisite and target checks, which is what these cases judge.
    """
    import os
    import shutil
    import subprocess
    import tempfile

    tmp = pathlib.Path(tempfile.mkdtemp(prefix="lane_targets_"))
    bindir = tmp / "bin"
    bindir.mkdir()
    for tool in ("cargo", "rustc", "node", "npx"):
        stub = bindir / tool
        stub.write_text("#!/bin/sh\nexit 0\n")
        stub.chmod(0o755)
    rustup = bindir / "rustup"
    rustup.write_text(f"#!/bin/sh\nprintf '%s\\n' '{installed_targets}'\n")
    rustup.chmod(0o755)
    env = dict(os.environ)
    env["PATH"] = f"{bindir}{os.pathsep}{env.get('PATH', '')}"
    bash = shutil.which("bash") or "/bin/bash"
    argv = [bash, str(LANE), "--channel", channel, "--out", str(tmp / "out")]
    if check_only:
        argv.append("--check-only")
    try:
        return subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=180,
            cwd=str(REPO),
            env=env,
        )
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


class RustTargetTests(unittest.TestCase):
    def test_missing_target_fails_naming_the_fix(self):
        proc = _run_lane("node", "x86_64-apple-darwin")
        self.assertNotEqual(proc.returncode, 0, proc.stdout)
        self.assertIn(
            "missing rust target aarch64-unknown-linux-musl: "
            "rustup target add aarch64-unknown-linux-musl",
            proc.stderr,
        )

    def test_installed_target_passes_the_check(self):
        # Through --check-only: an installed target passes the prerequisite
        # and target checks and exits 0 without building. The old shape ran
        # the real node build plus the artefact gate, which found no .node
        # on a clean host and failed the case for want of artefacts.
        proc = _run_lane(
            "node", "x86_64-apple-darwin\naarch64-unknown-linux-musl",
            check_only=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("[build-lane] prerequisites ok (node)", proc.stdout)

    def test_workflows_add_every_target_explicitly(self):
        # The lane refuses to guess; the workflows must not make it:
        # every rust-toolchain step that feeds a cross build names its
        # targets, none relies on the runner default.
        import yaml

        for name, job in (
            ("release-natives.yml", "natives"),
            ("node-bindings.yml", "build-release"),
            ("release-binaries.yml", "build"),
            ("build-lane.yml", "build-lane"),
            ("wasm.yml", "wasm-build"),
        ):
            doc = yaml.safe_load(
                (REPO / ".github" / "workflows" / name).read_text()
            )
            steps = doc["jobs"][job]["steps"]
            toolchains = [
                s
                for s in steps
                if isinstance(s, dict)
                and "rust-toolchain" in str(s.get("uses", ""))
            ]
            self.assertTrue(toolchains, f"{name} has no toolchain step")
            for step in toolchains:
                inputs = step.get("with") or {}
                self.assertIn(
                    "targets", inputs, f"{name} toolchain adds no targets"
                )
                self.assertTrue(
                    str(inputs["targets"]).strip(),
                    f"{name} toolchain targets are empty",
                )


def _triggers() -> dict:
    """The workflow's `on:` block (YAML 1.1 reads a bare `on` as True)."""
    import yaml

    doc = yaml.safe_load(WORKFLOW.read_text())
    on = doc.get("on", doc.get(True, {}))
    return on if isinstance(on, dict) else {}


class BuildLaneJobTests(unittest.TestCase):
    def test_lane_has_a_manual_job(self):
        # Dispatch-only: the lane builds release channels, so it runs on a
        # person's decision, never on a push — but it runs in the pipeline.
        import yaml

        doc = yaml.safe_load(WORKFLOW.read_text())
        on = _triggers()
        self.assertIn("workflow_dispatch", on)
        self.assertNotIn("push", on)
        self.assertNotIn("pull_request", on)
        runs = "\n".join(
            str(s.get("run", ""))
            for job in doc["jobs"].values()
            if isinstance(job, dict)
            for s in (job.get("steps") or [])
            if isinstance(s, dict)
        )
        self.assertIn("scripts/release/build_all_platforms.sh", runs)
        self.assertIn("--channel", runs)

    def test_lane_job_takes_every_channel(self):
        inputs = _triggers()["workflow_dispatch"]["inputs"]
        self.assertIn("channel", inputs)
        options = inputs["channel"].get("options", [])
        for channel in ("python", "node", "capi", "java", "dotnet", "all"):
            self.assertIn(channel, options, f"channel {channel} not dispatchable")

    def test_lane_is_no_longer_excused_as_by_hand(self):
        # The ALLOWED entry is gone with the excuse: the script has a job
        # now, and the excuse must not silently return beside it. (The
        # quoted form is the dict key; the ratchet comment names the file
        # unquoted and stays.)
        self.assertNotIn('"build_all_platforms.sh"', GUARD_JOBS.read_text())


if __name__ == "__main__":
    unittest.main()
