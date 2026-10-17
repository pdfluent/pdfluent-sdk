#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""release-natives.yml builds the gate's seven platforms, every channel.

The natives matrix platforms equal the gate's PLATFORMS keys, in order —
the gate is the single source of truth, the workflow derives from it.
Every channel the release owns is then built for every platform: the
natives legs cut the C ABI cdylib, the JNI cdylib and the wheel per
platform, and the package job stages the jar and the nupkg, collects the
C API set and the sdist, and runs the gate once with --require-all.
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
WORKFLOW = REPO / ".github" / "workflows" / "release-natives.yml"

MINIMUM_CASES = 8  # FLOOR

EXPECTED_RUNNERS = {
    "linux-x64": "ubuntu-latest",
    "linux-arm64": "ubuntu-latest",
    "linux-x64-musl": "ubuntu-latest",
    "linux-arm64-musl": "ubuntu-latest",
    "osx-x64": "macos-latest",
    "osx-arm64": "macos-latest",
    "win-x64": "windows-latest",
}

EXPECTED_CROSS = {
    "linux-x64": False,
    "linux-arm64": False,
    "linux-x64-musl": False,
    "linux-arm64-musl": False,
    "osx-x64": True,
    "osx-arm64": False,
    "win-x64": False,
}

EXPECTED_ZIG = {
    "linux-x64": (True, False),
    "linux-arm64": (True, False),
    "linux-x64-musl": (True, True),
    "linux-arm64-musl": (True, True),
    "osx-x64": (False, False),
    "osx-arm64": (False, False),
    "win-x64": (False, False),
}

PUBLISH_COMMANDS = (
    "twine upload",
    "npm publish",
    "dotnet nuget push",
    "mvn deploy",
    "gh release create",
    "softprops/action-gh-release",
)


def _gate():
    spec = importlib.util.spec_from_file_location(
        "natives_gate", HERE / "artifacts_cover_every_platform.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _doc() -> dict:
    return yaml.safe_load(WORKFLOW.read_text())


def _runs(job: dict) -> str:
    return "\n".join(
        str(step.get("run", ""))
        for step in job.get("steps", [])
        if isinstance(step, dict)
    )


class ReleaseNativesTests(unittest.TestCase):
    def test_matrix_platforms_are_the_gate_seven_in_order(self):
        gate = _gate()
        include = _doc()["jobs"]["natives"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            [entry["platform"] for entry in include],
            [row[0] for row in gate.PLATFORMS],
        )

    def test_matrix_runs_each_platform_where_it_builds(self):
        include = _doc()["jobs"]["natives"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            {entry["platform"]: entry["runs-on"] for entry in include},
            EXPECTED_RUNNERS,
        )

    def test_osx_x64_cross_builds_on_arm64_hardware(self):
        # The retired Intel label is gone: osx-x64 builds on macos-latest
        # (arm64) with --target x86_64-apple-darwin, and only osx-x64
        # crosses. Every entry carries the key, so a dropped key fails
        # here rather than scheduling a native build of the wrong arch.
        include = _doc()["jobs"]["natives"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            {entry["platform"]: entry.get("cross") for entry in include},
            EXPECTED_CROSS,
        )
        runs = _runs(_doc()["jobs"]["natives"])
        self.assertIn("matrix.cross", runs)
        self.assertIn('--target "${{ matrix.triple }}"', runs)

    def test_matrix_builds_linux_with_zig_and_musl_with_the_crt_flag(self):
        include = _doc()["jobs"]["natives"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            {entry["platform"]: (entry["zig"], entry["musl"]) for entry in include},
            EXPECTED_ZIG,
        )
        runs = _runs(_doc()["jobs"]["natives"])
        self.assertIn("cargo zigbuild", runs)
        self.assertIn("musl_cdylib_rustflags.sh", runs)

    def test_zig_tool_follows_the_matrix_zig_flag(self):
        # cargo-zigbuild is a Linux-cross tool: the win/osx legs never
        # invoke it (matrix.zig false there), so they must not install it
        # either — the Windows leg's zig install aborted setup (build 2).
        setup = next(
            step
            for step in _doc()["jobs"]["natives"]["steps"]
            if isinstance(step, dict)
            and str(step.get("uses", "")).endswith("setup-selfhosted")
        )
        tools = setup["with"]["tools"]
        self.assertIn("matrix.zig", tools)
        bare = re.sub(r"\$\{\{.*?\}\}", "", tools)
        self.assertNotIn("zig", bare)

    def test_natives_legs_build_capi_jni_and_wheel(self):
        runs = _runs(_doc()["jobs"]["natives"])
        self.assertIn("pdf-capi", runs)
        self.assertIn("pdf-java", runs)
        self.assertIn("maturin build", runs)
        for slot in ("capi", "java", "dotnet", "wheels"):
            self.assertIn(f"natives-$PLAT/{slot}", runs)

    def test_package_stages_the_jar_with_env_overrides(self):
        runs = _runs(_doc()["jobs"]["package"])
        self.assertIn("stage_java_natives.sh", runs)
        self.assertIn("mvn -B package", runs)
        self.assertIn("JNI_LINUX_X64=", runs)
        self.assertIn("JNI_LINUX_ARM64_MUSL=", runs)
        self.assertIn("CAPI_OSX_ARM64=", runs)

    def test_package_sets_partial_only_when_legs_are_missing(self):
        # Build 2: win-x64 missing, the staging refused, no
        # release-packages was uploaded and all seven smokes failed at
        # the download. A partial run now stages what exists — but only
        # behind a non-empty .missing-legs: a full run sets neither flag.
        steps = [
            s for s in _doc()["jobs"]["package"]["steps"]
            if isinstance(s, dict)
        ]
        partial = [s for s in steps if "PARTIAL_OK" in str(s.get("run", ""))]
        self.assertGreaterEqual(len(partial), 2, "jar staging and capi collect go partial")
        for step in partial:
            self.assertIn(
                ".missing-legs", str(step.get("run", "")),
                f"step {step.get('name')!r} sets PARTIAL_OK unconditionally",
            )
        self.assertNotIn("PARTIAL_OK", _runs(_doc()["jobs"]["smoke"]))

    def test_package_skips_the_enforcer_only_when_legs_are_missing(self):
        # The jar cannot build partial past the enforcer, so the partial
        # path packages past it (the gate below still judges the missing
        # natives); the full path packages under it, as before.
        run = next(
            str(s.get("run", ""))
            for s in _doc()["jobs"]["package"]["steps"]
            if isinstance(s, dict) and s.get("name") == "Stage and package the jar"
        )
        self.assertIn(".missing-legs", run)
        self.assertIn("mvn -B package -Denforcer.skip=true", run)
        self.assertIn("( cd bindings/java && mvn -B package )", run)

    def test_a_missing_jar_fails_loud_where_it_is_noticed(self):
        # Build 6: the enforcer refused the jar, so "Stage and package the
        # jar" never exported JAR, and "Lay out release-packages" (which runs
        # past a failed gate) died on `cp "$JAR"` with "JAR: unbound
        # variable" -- the consequence hiding the cause. Both steps refuse
        # aloud now: the stage step names the unproduced jar (a -f check,
        # because an unmatched glob would export its own literal), and the
        # layout step names the unpackaged jar/nupkg before copying.
        steps = {
            s.get("name"): str(s.get("run", ""))
            for s in _doc()["jobs"]["package"]["steps"]
            if isinstance(s, dict)
        }
        stage = steps["Stage and package the jar"]
        self.assertIn("no packaged jar", stage)
        self.assertIn('! -f "$JAR"', stage)
        layout = steps["Lay out release-packages"]
        for var, built in (("JAR", "jar was packaged"), ("NUPKG", "nupkg was packed")):
            guard = f'{var}:?release-natives: no {built}'
            self.assertIn(guard, layout)
            self.assertLess(layout.index(guard), layout.index(f'"${var}"'),
                            f"the {var} guard runs after its first use")

    def test_wheels_demand_seven_full_and_one_partial(self):
        run = next(
            str(s.get("run", ""))
            for s in _doc()["jobs"]["package"]["steps"]
            if isinstance(s, dict) and s.get("name") == "Collect the wheels and build the sdist"
        )
        self.assertIn('test "${#whls[@]}" = "7"', run)
        self.assertIn('test "${#whls[@]}" -ge 1', run)
        self.assertIn(".missing-legs", run)

    def test_package_uploads_what_exists_and_fails_at_the_end(self):
        # The gate fails on a partial set; the layout and the upload run
        # anyway (validate what exists), and the final step fails the job
        # naming the missing legs.
        steps = [
            s for s in _doc()["jobs"]["package"]["steps"]
            if isinstance(s, dict)
        ]
        for name in ("Lay out release-packages", "Fail if any leg was skipped"):
            step = next(s for s in steps if s.get("name") == name)
            self.assertIn("!cancelled()", str(step.get("if", "")), name)
        upload = next(
            s for s in steps
            if str(s.get("uses", "")).startswith("actions/upload-artifact")
        )
        self.assertIn("!cancelled()", str(upload.get("if", "")), "upload")
        final = next(s for s in steps if s.get("name") == "Fail if any leg was skipped")
        self.assertIn(".missing-legs", str(final.get("run", "")))
        self.assertIn("exit 1", str(final.get("run", "")))

    def test_smoke_leg_skips_before_downloading_when_never_built(self):
        # The leg whose natives were never built reports SKIPPED and never
        # reaches the download — no consequence error at Download.
        steps = [
            s for s in _doc()["jobs"]["smoke"]["steps"]
            if isinstance(s, dict)
        ]
        names = [s.get("name") for s in steps]
        skip = names.index("Skip when this leg was never built")
        self.assertLess(skip, names.index("Download release-packages"))
        run = str(steps[skip].get("run", ""))
        self.assertIn("github.run_id", run)
        self.assertIn("natives-${{ matrix.platform }}", run)
        self.assertIn("SKIPPED leg", run)
        self.assertIn("exit 1", run)

    def test_package_stages_the_nupkg_with_env_overrides(self):
        runs = _runs(_doc()["jobs"]["package"])
        self.assertIn("stage_dotnet_natives.sh", runs)
        self.assertIn("dotnet pack", runs)
        self.assertIn("CAPI_LINUX_MUSL_X64=", runs)
        self.assertIn("CAPI_WIN_X64=", runs)

    def test_package_collects_capi_wheels_sdist_and_runs_the_gate_once(self):
        runs = _runs(_doc()["jobs"]["package"])
        self.assertIn("collect_capi_release.sh", runs)
        self.assertIn("build_pdfluent_sdist.sh", runs)
        self.assertIn("python -m build integrations/pdfluent-extract", runs)
        gate_calls = [
            line
            for line in runs.splitlines()
            if "artifacts_cover_every_platform.py" in line
        ]
        self.assertEqual(len(gate_calls), 1, gate_calls)
        self.assertIn("--require-all jar wheels nupkg capi", runs)
        for flag in ("--jar", "--wheels", "--nupkg", "--capi-dir", "--extract-dir"):
            self.assertIn(flag, runs)

    def test_package_audits_the_licence_of_every_channel(self):
        # The jar needs all seven natives, so its audit can only run here,
        # after mvn package -- and the nupkg, the C API tarballs and the
        # wheels are audited where they are built too. Each audit runs
        # AFTER its build: an audit above its build would judge stale
        # bytes and fail the job for the wrong reason.
        runs = _runs(_doc()["jobs"]["package"])
        audit_lines = [
            line
            for line in runs.splitlines()
            if "license_audit.py" in line
        ]
        self.assertGreaterEqual(len(audit_lines), 4, audit_lines)
        joined = "\n".join(audit_lines)
        for artefact in ('"$JAR"', '"$NUPKG"', "capi-tarballs/", "wheels/"):
            self.assertIn(artefact, joined, audit_lines)
        for build, audit in (
            ("mvn -B package", 'license_audit.py "$JAR"'),
            ("dotnet pack", 'license_audit.py "$NUPKG"'),
            ("collect_capi_release.sh", "license_audit.py capi-tarballs/"),
            ("build_pdfluent_sdist.sh", "license_audit.py wheels/"),
        ):
            self.assertLess(
                runs.index(build),
                runs.index(audit),
                f"{audit} runs before its build",
            )

    def test_nothing_in_the_workflow_publishes(self):
        text = WORKFLOW.read_text()
        for command in PUBLISH_COMMANDS:
            self.assertNotIn(command, text)


if __name__ == "__main__":
    unittest.main()
