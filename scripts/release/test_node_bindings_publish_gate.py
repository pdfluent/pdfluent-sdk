#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The npm publish is manual and gated, on all seven targets.

A `v*` tag used to publish @pdfluent/node automatically, with no artefact
gate and no smoke over the fresh bytes — and linux-arm64-musl was missing
from the build matrix entirely. Now the workflow is dispatch-only, the
publish needs publish_npm=true behind the release gate (artefact gate plus
loader smoke over the fresh .node files), and the matrix builds all seven
napi targets with both musl legs through zig.
"""
from __future__ import annotations

import pathlib
import unittest

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".github" / "workflows" / "node-bindings.yml"

MINIMUM_CASES = 7  # FLOOR

SEVEN_TARGETS = (
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
)

MUSL_TARGETS = (
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
)


def _doc() -> dict:
    return yaml.safe_load(WORKFLOW.read_text())


def _run_of(steps: list, prefix: str) -> str:
    """The run block of the first step whose name starts with prefix."""
    for step in steps:
        if isinstance(step, dict) and str(step.get("name", "")).startswith(prefix):
            return str(step.get("run", ""))
    return ""


def _arm_body(run: str, triple: str) -> str | None:
    """The case-arm body (label included) of the arm naming triple."""
    lines = run.splitlines()
    for i, line in enumerate(lines):
        stripped = line.strip()
        if ")" not in stripped:
            continue
        label, rest = stripped.split(")", 1)
        if triple not in label or "case " in label:
            continue
        if ";;" in rest:
            return stripped
        collected = [stripped]
        for follow in lines[i + 1 :]:
            collected.append(follow.strip())
            if ";;" in follow:
                break
        return "\n".join(collected)
    return None


def _governing_arm_label(run: str, needle: str) -> str | None:
    """The case-arm label governing the first line containing needle."""
    label = None
    for line in run.splitlines():
        stripped = line.strip()
        if stripped.startswith("case "):
            label = None
        elif (
            stripped.endswith(")")
            and ";;" not in stripped
            and not stripped.startswith("#")
        ):
            label = stripped[:-1].strip()
        if needle in stripped and label is not None:
            return label
    return None


class NodePublishGateTests(unittest.TestCase):
    def test_no_tag_trigger_starts_this_workflow(self):
        doc = _doc()
        on = doc.get("on", doc.get(True, {}))
        push = on.get("push", {}) if isinstance(on, dict) else {}
        self.assertNotIn(
            "tags",
            push if isinstance(push, dict) else {},
            "a tag must not start this workflow: the publish is a dispatch "
            "decision, not a release moment",
        )
        self.assertIn("workflow_dispatch", on)

    def test_publish_needs_an_explicit_decision_not_a_tag(self):
        publish = _doc()["jobs"]["publish-npm"]
        condition = str(publish.get("if", ""))
        self.assertIn("inputs.publish_npm == 'true'", condition)
        self.assertNotIn("startsWith(github.ref, 'refs/tags/v')", condition)
        # The pre-release clause stays: a dispatch aimed at an rc tag is the
        # same publish with a person's name on it (#358).
        self.assertIn("github.ref_type == 'tag'", condition)
        self.assertIn("contains(github.ref_name, '-')", condition)
        # The standalone tag requirement, not just the occurrence inside
        # the pre-release clause: a branch dispatch with publish_npm=true
        # publishes nothing.
        self.assertIn("&& github.ref_type == 'tag'", condition)

    def test_publish_waits_for_the_release_gate(self):
        publish = _doc()["jobs"]["publish-npm"]
        needs = publish.get("needs", [])
        needs = [needs] if isinstance(needs, str) else list(needs)
        for job in ("build-and-test", "build-release", "release-gate"):
            self.assertIn(job, needs)

    def test_release_gate_judges_the_fresh_bytes_and_smokes_them(self):
        gate = _doc()["jobs"]["release-gate"]
        runs = "\n".join(
            str(s.get("run", "")) for s in gate.get("steps", []) if isinstance(s, dict)
        )
        self.assertIn("artifacts_cover_every_platform.py --node-dir", runs)
        self.assertIn("--require-all node", runs)
        self.assertIn("generate-npm-dirs.cjs", runs)
        self.assertIn("guard-prepublish.cjs", runs)
        self.assertIn("npm test", runs)

    def test_matrix_builds_all_seven_targets(self):
        include = (
            _doc()["jobs"]["build-release"]["strategy"]["matrix"]["include"]
        )
        targets = sorted(e["target"] for e in include)
        self.assertEqual(targets, sorted(SEVEN_TARGETS))
        musl_arm = next(
            e for e in include if e["target"] == "aarch64-unknown-linux-musl"
        )
        self.assertEqual(musl_arm["os"], "ubuntu-latest")

    def test_both_musl_legs_build_through_zig(self):
        steps = _doc()["jobs"]["build-release"]["steps"]
        linker = _run_of(steps, "Cross linker")
        build = _run_of(steps, "Build for")
        self.assertNotEqual(linker, "", "no Cross linker step in build-release")
        self.assertNotEqual(build, "", "no Build for step in build-release")
        # The GitHub pool installs zig for BOTH musl legs (pip ziglang plus
        # cargo-zigbuild, like release-natives.yml): the case arm that names
        # a musl triple must install ziglang. And musl-tools is gone: the
        # host's musl-gcc cannot link the dynamic-CRT cdylib (no musl
        # libgcc_s: "/usr/bin/ld: cannot find libgcc_s.so.1").
        self.assertNotIn("musl-tools", linker)
        for triple in MUSL_TARGETS:
            arm = _arm_body(linker, triple)
            self.assertIsNotNone(arm, f"{triple} has no arm in the linker step")
            self.assertIn("ziglang", arm)
            self.assertIn("cargo-zigbuild", arm)
        # The bundled zig lands on PATH: pip's ziglang ships no `zig`
        # script, only `python -m ziglang`, which napi never calls.
        self.assertIn("GITHUB_PATH", linker)
        # --zig: the napi 3 flag the aarch64 leg used to pass dies on the
        # pinned 2.18 CLI ("Unsupported option name"), which is held
        # precisely — per flag on every `napi build` line — by
        # test_napi_build_flags.py. The build line's flag sequence, not the
        # comments that name the flag: removing --zig from the build must
        # fail this. (--features rides between --zig and --target since the
        # bindings ship the model tiers on.)
        self.assertIn("--platform --zig --features ort-layout,ort-tables --target", build)
        # The arm label governing the --zig build line covers both musl
        # legs: narrowing it back to one triple fails this, and so does a
        # separate single-triple musl arm beside it.
        self.assertEqual(_governing_arm_label(build, "--platform --zig"), "*-musl")
        self.assertNotRegex(build, r"(?m)^\s*[^\s#*][^\n]*-musl\)")
        # The host C compiler stays the host's: napi --zig exports its musl
        # wrapper as the global CC, which would emit musl ELF into the
        # host proc-macro link without HOST_CC. The export itself, not the
        # comment that names it.
        self.assertIn("HOST_CC=cc", build)
        self.assertIn("HOST_CXX=c++", build)

    def test_self_hosted_musl_legs_take_the_zig_tool(self):
        # The self pool gets zig from the setup action (pip ziglang plus
        # cargo-zigbuild), on the musl legs: a setup step whose tools list
        # carries zig under each musl leg's condition.
        steps = _doc()["jobs"]["build-release"]["steps"]
        setups = [
            s
            for s in steps
            if isinstance(s, dict)
            and str(s.get("uses", "")) == "./.github/actions/setup-selfhosted"
        ]
        self.assertTrue(setups, "no self-hosted setup step in build-release")
        zigged = [
            s
            for s in setups
            if "zig"
            in {
                token.strip()
                for token in str((s.get("with") or {}).get("tools", "")).split(
                    ","
                )
            }
        ]
        self.assertTrue(
            zigged,
            "no setup step carries the zig tool",
        )
        for triple in MUSL_TARGETS:
            self.assertTrue(
                any(triple in str(s.get("if", "")) for s in zigged),
                f"no setup step carries the zig tool for {triple}",
            )

    def test_musl_legs_take_the_cdylib_flags_from_the_helper(self):
        # P0 follow-through: napi appends -crt-static itself but drops the
        # config's rustflags when it exports RUSTFLAGS, so the musl legs
        # source the helper for the scrubbing beside the flag.
        steps = _doc()["jobs"]["build-release"]["steps"]
        runs = "\n".join(
            str(s.get("run", "")) for s in steps if isinstance(s, dict)
        )
        self.assertIn("musl_cdylib_rustflags.sh", runs)


if __name__ == "__main__":
    unittest.main()
