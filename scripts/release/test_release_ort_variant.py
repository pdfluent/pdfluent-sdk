#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The release workflows ship the balanced/TATR (ort) variant on all targets.

The release CLI builds with `--features ort-layout,ort-tables`; the wheel
and the node addon build as their pyproject/package.json say (T1's
always-on wiring, expected, not added here); the ort-rust legs and the
non-musl smoke legs download the models-v1 assets they need from the
PUBLIC models-v1 release and verify every byte against the tree pins
before use -- no Actions artefact carries the models (D18), and no
credential is involved. Every non-musl smoke leg installs layout+tables
from the staged dir into a temp model dir and runs a balanced extract;
and a per-target Rust job runs the tiered pdf-engine suite with
`ORT_DYLIB_PATH` on its platform's library. The default Rust crate and the
WASM build stay pure Rust: no ort anywhere near them.

T1 owns two halves of this shape -- the `models install layout tables` /
`PDFLUENT_MODELS_BASE` install surface and the bindings' always-on wiring
-- so the cases below read the tree's state and hold whichever half is
expected: while the Cargo features are absent the smokes must carry the
fail-closed "requires the T1 job" gates; once a binding declares them, its
always-on wiring must be on. A half-landed T1 (features without wiring)
is red either way.
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
WORKFLOWS = REPO / ".github" / "workflows"

NATIVES = WORKFLOWS / "release-natives.yml"
BINARIES = WORKFLOWS / "release-binaries.yml"
NODE = WORKFLOWS / "node-bindings.yml"
SMOKE = WORKFLOWS / "release-smoke.yml"
WASM = WORKFLOWS / "wasm.yml"

SMOKE_SCRIPT = HERE / "consumer_smokes" / "smoke_ort_variant.sh"
STAGE_MODELS = HERE / "stage_models.py"
FETCH_MODELS = HERE / "fetch_models_release.py"
MODELS_PINS = HERE / "models_pins.toml"
MODELS_NOTICE = HERE / "models-v1" / "MODELS-NOTICE.txt"
MODELS_NOTICE_PLACEHOLDER = HERE / "MODELS-NOTICE.txt"
ORT_PINS = HERE / "ort_pins.toml"
CMD_MODELS = REPO / "crates" / "pdfluent-cli" / "src" / "cmd_models.rs"

PYPROJECT = REPO / "crates" / "pdf-python" / "pyproject.toml"
PDF_PYTHON_CARGO = REPO / "crates" / "pdf-python" / "Cargo.toml"
PDF_NODE_CARGO = REPO / "crates" / "pdf-node" / "Cargo.toml"
PACKAGE_JSON = REPO / "crates" / "pdf-node" / "package.json"

MINIMUM_CASES = 19  # FLOOR

ORT_FEATURES = ("ort-layout", "ort-tables")

# `ort` as a word: `import`, `sort` and `report` never match, on either
# capitalisation. Full-line comments are stripped before the scan, so a
# comment saying "no ort here" passes; anything else fails.
ORT_WORD = re.compile(r"\bort(-layout|-tables)?\b", re.IGNORECASE)

EXPECTED_ORT_TARGETS = [
    ("x86_64-unknown-linux-gnu", "onnxruntime-linux-x86_64-1.23.2.so"),
    ("aarch64-unknown-linux-gnu", "onnxruntime-linux-arm64-1.23.2.so"),
    ("x86_64-apple-darwin", "onnxruntime-macos-x86_64-1.23.2.dylib"),
    ("aarch64-apple-darwin", "onnxruntime-macos-arm64-1.23.2.dylib"),
    ("x86_64-pc-windows-msvc", "onnxruntime-windows-x86_64-1.23.2.dll"),
]

# The fourteen models-v1 release assets. The downloader stages the
# thirteen installables; SHA256SUMS is informational only (the tree pins
# are the authority) and is never staged.
EXPECTED_MODELS_ASSETS = [
    "heron_ep03.onnx",
    "tatr-v1.1-all.onnx",
    "onnxruntime-linux-x86_64-1.23.2.so",
    "onnxruntime-linux-arm64-1.23.2.so",
    "onnxruntime-macos-arm64-1.23.2.dylib",
    "onnxruntime-macos-x86_64-1.23.2.dylib",
    "onnxruntime-windows-x86_64-1.23.2.dll",
    "onnxruntime-LICENSE.txt",
    "onnxruntime-ThirdPartyNotices.txt",
    "MODELS-NOTICE.txt",
    "LICENSE-Apache-2.0.txt",
    "LICENSE",
    "LICENSE-COMMERCIAL",
    "SHA256SUMS",
]

# The root texts of the models-v1 release: the staged name and the tree
# file whose bytes pin it. The downloader verifies the release's texts
# against these files, so a rewording of any text fails until the release
# follows deliberately.
TREE_TEXTS = {
    "MODELS-NOTICE.txt": MODELS_NOTICE,
    "LICENSE-Apache-2.0.txt": REPO / "LICENSE-APACHE",
    "LICENSE": REPO / "LICENSE",
    "LICENSE-COMMERCIAL": REPO / "LICENSE-COMMERCIAL",
}

# The owner-approved models-v1 notice wording, pinned so a rewording fails
# here until the job's pin follows deliberately.
MODELS_NOTICE_SHA256 = (
    "5eb1f80e9689fedb88c3e1f6c415c8341283e0e37ead0e12f0862e6191ac4b8b"
)

RELEASE_WORKFLOWS = [WORKFLOWS / name for name in (
    "release-natives.yml",
    "release-binaries.yml",
    "release-smoke.yml",
)]

EXPECTED_ORT_SMOKE_PLATFORMS = {
    "linux-x64",
    "linux-arm64",
    "osx-x64",
    "osx-arm64",
    "win-x64",
}


def _doc(path: pathlib.Path):
    return yaml.safe_load(path.read_text())


def _steps(doc, job: str) -> list:
    return [s for s in doc["jobs"][job]["steps"] if isinstance(s, dict)]


def _step(doc, job: str, name: str) -> dict:
    for step in _steps(doc, job):
        if step.get("name") == name:
            return step
    raise AssertionError(f"{job} has no step {name!r}")


def _code(run: str) -> str:
    return "\n".join(
        line for line in run.splitlines() if not line.strip().startswith("#")
    )


def _job_text(doc, job: str) -> str:
    return "\n".join(
        f"{s.get('name', '')}\n{s.get('uses', '')}\n{s.get('run', '')}"
        for s in _steps(doc, job)
    )


def _cargo_features(manifest: pathlib.Path) -> dict:
    return tomllib.loads(manifest.read_text()).get("features", {})


def _models_v1_artifact_steps(doc) -> list[str]:
    """Every step moving an artefact named models-v1, as `job :: step`."""
    found = []
    for job in doc["jobs"]:
        for step in _steps(doc, job):
            uses = step.get("uses", "")
            if uses.startswith("actions/upload-artifact") or uses.startswith(
                "actions/download-artifact"
            ):
                if step.get("with", {}).get("name") == "models-v1":
                    found.append(f"{job} :: {step.get('name', '')}")
    return found


def _release_base_from_downloader(script: str) -> str:
    match = re.search(r'^RELEASE_BASE = "([^"]+)"', script, re.MULTILINE)
    if match is None:
        raise AssertionError("the downloader pins no RELEASE_BASE")
    return match.group(1)


def _release_base_from_cmd_models() -> str:
    match = re.search(
        r'MODELS_RELEASE_BASE:\s*&str\s*=\s*\n\s*"([^"]+)"',
        CMD_MODELS.read_text(),
    )
    if match is None:
        raise AssertionError("cmd_models.rs pins no MODELS_RELEASE_BASE")
    return match.group(1)


def _sha256_of(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class OrtVariantTests(unittest.TestCase):
    def test_cli_build_uses_both_ort_features(self) -> None:
        run = _step(_doc(BINARIES), "build", "Build binary")["run"]
        self.assertEqual(
            _code(run).count("--features ort-layout,ort-tables"),
            2,
            "the cross line and the cargo line both build with the ort tiers",
        )

    def test_wheel_build_takes_no_feature_flags(self) -> None:
        # The wheel builds as the pyproject says: T1's always-on wiring
        # lives in [tool.maturin], and a flag on this line would override
        # it -- or, worse, contradict it in silence.
        run = _step(_doc(NATIVES), "natives", "Build the wheel")["run"]
        for flag in ("--features", "--no-default-features", "--all-features"):
            self.assertNotIn(flag, _code(run))

    def test_default_rust_legs_carry_no_ort(self) -> None:
        doc = _doc(SMOKE)
        self.assertIn("cargo test -p pdfluent", _job_text(doc, "smoke"))
        matches = [
            line.strip()
            for step in _steps(doc, "smoke")
            for line in _code(step.get("run", "")).splitlines()
            if ORT_WORD.search(line)
        ]
        self.assertEqual(matches, [])

    def test_wasm_carries_no_ort(self) -> None:
        doc = _doc(WASM)
        matches = [
            f"{job}: {line.strip()}"
            for job in doc["jobs"]
            for step in _steps(doc, job)
            for line in _code(
                f"{step.get('uses', '')}\n{step.get('run', '')}"
            ).splitlines()
            if ORT_WORD.search(line)
        ]
        self.assertEqual(matches, [])

    def test_smoke_job_waits_on_package(self) -> None:
        needs = _doc(NATIVES)["jobs"]["smoke"]["needs"]
        self.assertEqual(needs, ["package"])

    def test_every_non_musl_smoke_leg_has_a_models_install_smoke(self) -> None:
        doc = _doc(NATIVES)
        include = doc["jobs"]["smoke"]["strategy"]["matrix"]["include"]
        non_musl = {e["platform"] for e in include if e.get("musl") is not True}
        self.assertEqual(non_musl, EXPECTED_ORT_SMOKE_PLATFORMS)
        download = _step(doc, "smoke", "Download models-v1 from the public release")
        self.assertIn("inputs.runner_pool == 'self'", download.get("if", ""))
        self.assertIn("matrix.musl != true", download.get("if", ""))
        # The step fetches through the downloader, which the byte case
        # holds to the release base and the tree pins; here the step must
        # name it, stage the full set, and land it where the channels
        # consume it via file:// (held by the file-base case below).
        run = download.get("run", "")
        self.assertIn("fetch_models_release.py", run)
        self.assertIn("--out models-v1", run)
        self.assertIn("--all", run)
        for channel in ("CLI", "python", "node"):
            with self.subTest(channel=channel):
                step = _step(doc, "smoke", f"Smoke the ort variant ({channel})")
                self.assertIn("inputs.runner_pool == 'self'", step.get("if", ""))
                self.assertIn("matrix.musl != true", step.get("if", ""))
                self.assertIn("smoke_ort_variant.sh", step["run"])
                self.assertIn(f"--channel {channel.lower()}", step["run"])
                self.assertIn("--models models-v1", step["run"])

    def test_ort_smoke_installs_from_a_file_models_base(self) -> None:
        script = SMOKE_SCRIPT.read_text()
        base = [
            line.strip()
            for line in script.splitlines()
            if "PDFLUENT_MODELS_BASE=" in line
            and ("file://" in line or "MODELS_URL" in line)
        ]
        if any("MODELS_URL" in line for line in base):
            self.assertIn('MODELS_URL="$(smoke_file_url ', script)
        self.assertTrue(base, "no PDFLUENT_MODELS_BASE=file:// install in the smoke")
        self.assertIn("models install layout tables", script)
        self.assertIn("models status", script)
        self.assertIn('quality="balanced"', script)
        self.assertIn('{ quality: "balanced" }', script)

    def test_ort_smoke_fails_closed_while_t1_is_absent(self) -> None:
        script = SMOKE_SCRIPT.read_text()
        self.assertGreaterEqual(
            script.count("requires the T1 install job"), 2,
            "the tables and the MODELS_BASE gates",
        )
        self.assertGreaterEqual(
            script.count("requires the T1 bindings job"), 2,
            "the python and the node gates",
        )

    def test_no_step_uses_a_secret_for_gcs(self) -> None:
        for job in ("ort-rust", "smoke"):
            text = _job_text(_doc(NATIVES), job)
            for forbidden in (
                "secrets.",
                "gcloud auth",
                "GOOGLE_APPLICATION_CREDENTIALS",
            ):
                self.assertNotIn(forbidden, text, job)
        for path in (FETCH_MODELS, STAGE_MODELS, SMOKE_SCRIPT):
            for forbidden in (
                "secrets.",
                "gcloud auth",
                "GOOGLE_APPLICATION_CREDENTIALS",
            ):
                self.assertNotIn(forbidden, path.read_text(), path.name)
        # Extended (D18): no step may reach for the models-v1 Actions
        # artefact either -- the release carries no such artefact anymore.
        self.assertEqual(_models_v1_artifact_steps(_doc(NATIVES)), [])

    def test_no_models_package_job_and_no_models_v1_upload(self) -> None:
        doc = _doc(NATIVES)
        self.assertNotIn("models-package", doc["jobs"])
        uploads = [
            s
            for job in doc["jobs"]
            for s in _steps(doc, job)
            if s.get("uses", "").startswith("actions/upload-artifact")
        ]
        self.assertTrue(uploads, "no upload-artifact step left to hold?")
        for step in uploads:
            self.assertNotEqual(
                step.get("with", {}).get("name"), "models-v1",
                "an upload-artifact step still names models-v1",
            )

    def test_github_pool_skips_the_self_only_model_legs(self) -> None:
        # Re-targeted: the skip message is gone with the models-package
        # job; what stays self-only is the model legs themselves, and
        # this case holds the gating that skips them on the github pool.
        doc = _doc(NATIVES)
        job = doc["jobs"]["ort-rust"]
        self.assertIn("inputs.runner_pool == 'self'", job.get("if", ""))
        download = _step(doc, "smoke", "Download models-v1 from the public release")
        self.assertIn("inputs.runner_pool == 'self'", download.get("if", ""))
        for channel in ("CLI", "python", "node"):
            with self.subTest(channel=channel):
                step = _step(doc, "smoke", f"Smoke the ort variant ({channel})")
                self.assertIn("inputs.runner_pool == 'self'", step.get("if", ""))

    def test_ort_rust_runs_the_tiered_suite_per_target(self) -> None:
        doc = _doc(NATIVES)
        job = doc["jobs"]["ort-rust"]
        needs = job.get("needs") or []
        if isinstance(needs, str):
            needs = [needs]
        self.assertNotIn("models-package", needs)
        self.assertIn("!cancelled()", job.get("if", ""))
        self.assertIn("inputs.runner_pool == 'self'", job.get("if", ""))
        include = job["strategy"]["matrix"]["include"]
        self.assertEqual(
            [(e["target"], e["ort-lib"]) for e in include], EXPECTED_ORT_TARGETS
        )
        download = _step(doc, "ort-rust", "Download models-v1 from the public release")
        self.assertIn("fetch_models_release.py", download.get("run", ""))
        self.assertIn("--out models-v1", download.get("run", ""))
        self.assertIn("matrix.ort-lib", download.get("run", ""))
        run = _step(doc, "ort-rust", "Run the pdf-engine suite with the ort tiers")["run"]
        self.assertIn(
            "cargo test --locked -p pdf-engine --features ort-layout,ort-tables", run
        )
        self.assertIn("ORT_DYLIB_PATH", run)
        self.assertIn("matrix.ort-lib", run)

    def test_t1_python_wiring(self) -> None:
        if "ort-layout" in _cargo_features(PDF_PYTHON_CARGO):
            features = tomllib.loads(PYPROJECT.read_text())["tool"]["maturin"]["features"]
            for feature in ORT_FEATURES:
                self.assertIn(
                    feature, features,
                    "T1 declared the Cargo features but the wheel builds without them",
                )
        else:
            self.assertNotIn("ort-layout", _cargo_features(PDF_PYTHON_CARGO))
            features = tomllib.loads(PYPROJECT.read_text())["tool"]["maturin"]["features"]
            for feature in ORT_FEATURES:
                self.assertNotIn(feature, features)
            self.assertIn("requires the T1 bindings job", SMOKE_SCRIPT.read_text())

    def test_t1_node_wiring(self) -> None:
        if "ort-layout" in _cargo_features(PDF_NODE_CARGO):
            build = _step(
                _doc(NODE), "build-release", "Build for ${{ matrix.target }}"
            )["run"]
            package = PACKAGE_JSON.read_text()
            for feature in ORT_FEATURES:
                self.assertTrue(
                    feature in build or feature in package,
                    f"T1 declared {feature} but neither the release napi line "
                    "nor package.json enables it",
                )
        else:
            build = _step(
                _doc(NODE), "build-release", "Build for ${{ matrix.target }}"
            )["run"]
            for feature in ORT_FEATURES:
                self.assertNotIn(feature, _code(build))
                self.assertNotIn(feature, PACKAGE_JSON.read_text())
            self.assertIn("requires the T1 bindings job", SMOKE_SCRIPT.read_text())

    def test_models_notice_file_exists(self) -> None:
        # The downloader verifies the release's notice against this file,
        # and the script-existence guard only covers .sh/.py -- without
        # this case the pin breaks unheld.
        self.assertTrue(MODELS_NOTICE.is_file())
        self.assertTrue(MODELS_PINS.is_file())
        self.assertFalse(
            MODELS_NOTICE_PLACEHOLDER.exists(),
            "m490's TODO placeholder must not linger beside the final text",
        )

    def test_models_notice_is_the_pinned_owner_text(self) -> None:
        self.assertEqual(_sha256_of(MODELS_NOTICE), MODELS_NOTICE_SHA256)
        first = MODELS_NOTICE.read_text().splitlines()[0]
        self.assertFalse(
            first.startswith("TODO"),
            "the staged notice is still a placeholder",
        )

    def test_no_step_names_a_models_v1_actions_artefact(self) -> None:
        # D18: the models-v1 Actions artefact is gone -- no step in the
        # workflow may upload or download an artefact under that name.
        self.assertEqual(_models_v1_artifact_steps(_doc(NATIVES)), [])

    def test_every_downloaded_byte_is_verified_against_the_tree_pins(self) -> None:
        # Pin enforcement moved from the deleted assemble step into the
        # downloader both consumers call: the base it fetches from is the
        # release base cmd_models.rs pins, every staged asset verifies
        # against the tree pins, and anything else refuses.
        doc = _doc(NATIVES)
        script = FETCH_MODELS.read_text()
        for job in ("ort-rust", "smoke"):
            with self.subTest(job=job):
                run = _step(
                    doc, job, "Download models-v1 from the public release"
                )["run"]
                self.assertIn("fetch_models_release.py", run)
                self.assertIn("--out models-v1", run)
        self.assertIn("models_pins.toml", script)
        self.assertIn("ort_pins.toml", script)
        self.assertIn("sha256", script)
        self.assertIn("REFUSED", script)
        base = _release_base_from_downloader(script)
        self.assertEqual(base, _release_base_from_cmd_models())
        self.assertIn("releases/download/models-v1/", base)
        # The staged universe is the release's installable set: all
        # thirteen names known, nothing else admitted. Against an empty
        # file:// base the run fails fetching -- never on an unknown
        # asset -- and a bogus name refuses up front instead.
        installables = [a for a in EXPECTED_MODELS_ASSETS if a != "SHA256SUMS"]
        self.assertEqual(len(installables), 13)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            base_dir = root / "base"
            base_dir.mkdir()
            out = root / "staged"
            file_base = base_dir.as_uri() + "/"
            flags: list[str] = []
            for asset in installables:
                flags += ["--asset", asset]
            proc = subprocess.run(
                [sys.executable, str(FETCH_MODELS), "--out", str(out),
                 *flags, "--base", file_base],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertNotIn("unknown asset", proc.stderr)
            self.assertIn("REFUSED", proc.stderr)
            proc = subprocess.run(
                [sys.executable, str(FETCH_MODELS), "--out", str(out),
                 "--asset", "not-a-model.onnx", "--base", file_base],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("unknown asset", proc.stderr)
            # A tampered tree text refuses and leaves nothing behind ...
            for asset, path in TREE_TEXTS.items():
                (base_dir / asset).write_bytes(path.read_bytes() + b"tampered")
            tree_flags: list[str] = []
            for asset in TREE_TEXTS:
                tree_flags += ["--asset", asset]
            proc = subprocess.run(
                [sys.executable, str(FETCH_MODELS), "--out", str(out),
                 *tree_flags, "--base", file_base],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("REFUSED", proc.stderr)
            leftovers = sorted(p.name for p in out.glob("*")) if out.is_dir() else []
            self.assertEqual(leftovers, [])
            # ... while the genuine tree bytes verify and land identical.
            for asset, path in TREE_TEXTS.items():
                (base_dir / asset).write_bytes(path.read_bytes())
            proc = subprocess.run(
                [sys.executable, str(FETCH_MODELS), "--out", str(out),
                 *tree_flags, "--base", file_base],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            for asset, path in TREE_TEXTS.items():
                with self.subTest(asset=asset):
                    self.assertEqual((out / asset).read_bytes(), path.read_bytes())

    def test_release_cli_steps_demand_a_pinned_installer(self) -> None:
        # T1's self-check (cmd_models.rs: PDFLUENT_RELEASE_PINS_REQUIRED=1
        # fails while any models-v1 pin is still TODO) only bites where the
        # env is set, so every release-workflow step that builds or tests
        # pdfluent-cli with ort-layout carries it. The matched list must be
        # non-empty, or this case would pass on zero steps.
        matched: list[str] = []
        for path in RELEASE_WORKFLOWS:
            doc = _doc(path)
            for job, body in doc["jobs"].items():
                for step in body.get("steps", []) or []:
                    if not isinstance(step, dict):
                        continue
                    code = _code(step.get("run", ""))
                    if "pdfluent-cli" in code and "ort-layout" in code:
                        matched.append(
                            f"{path.name} {job} {step.get('name', '')}"
                        )
                        env = step.get("env", {}) or {}
                        self.assertEqual(
                            env.get("PDFLUENT_RELEASE_PINS_REQUIRED"),
                            "1",
                            f"{path.name} builds pdfluent-cli with ort-layout "
                            "without demanding pinned models",
                        )
        self.assertTrue(
            matched, "no release step builds pdfluent-cli with ort-layout?"
        )


if __name__ == "__main__":
    cases = unittest.TestLoader().loadTestsFromTestCase(OrtVariantTests).countTestCases()
    if cases < MINIMUM_CASES:  # FLOOR
        print(
            f"[test-release-ort-variant] FATAL: {cases} cases, floor is "
            f"{MINIMUM_CASES}. A suite that lost its cases must not report green.",
        )
        raise SystemExit(1)
    unittest.main(verbosity=2)
