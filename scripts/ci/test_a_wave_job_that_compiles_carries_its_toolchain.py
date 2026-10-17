#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Drives a_wave_job_that_compiles_carries_its_toolchain.py. (#495)

The guard has one job and three ways to do it badly, and every one of them is
green:

  it looks at jobs that are not on a bought instance   -- then it is a guard
                                                          about ubuntu-latest,
                                                          where cargo is
                                                          installed for you
  it accepts a toolchain step AFTER the first step     -- which is exactly what
  that needs one                                          enterprise-acceptance
                                                          would have needed to
                                                          keep failing: its
                                                          cache action runs
                                                          `rustc -vV` first
  it reads `cargo` in a comment as a compile           -- and then it is red
                                                          about prose

Each of those is a case. The last two cases are about the image side, which is
half the repair and the half that lives in another file: a seal that stops
putting cargo where a profile-less shell can see it, and a boot probe that stops
asking whether an instance can compile, are both silent until the next wave.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "a_wave_job_that_compiles_carries_its_toolchain.py"

failures: list[str] = []
done = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global done
    done += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        failures.append(what)


TOOLCHAIN = ("      - uses: dtolnay/rust-toolchain@d1031067263f94b142dd6c0ce24c5eb9d02d52a0\n"
             "        with:\n"
             "          toolchain: stable\n")
CACHE = "      - uses: Swatinem/rust-cache@v2\n"
BUILD = "      - name: build\n        run: cargo build --release -p pdfluent\n"


def workflow(name: str, steps: str, runner: str = "${{ inputs.runner_label }}") -> str:
    return (f"name: {name}\n"
            "on:\n  workflow_call:\n    inputs:\n      runner_label:\n"
            "        required: true\n        type: string\n"
            "jobs:\n"
            f"  {name}:\n"
            f"    runs-on: {runner}\n"
            "    steps:\n"
            "      - uses: actions/checkout@v4\n"
            f"{steps}")


def run(files: dict[str, str], *extra: str) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as temporary:
        fake = pathlib.Path(temporary)
        (fake / ".github" / "workflows").mkdir(parents=True)
        for name, body in files.items():
            (fake / ".github" / "workflows" / f"{name}.yml").write_text(body)
        return subprocess.run([sys.executable, str(GUARD), "--repo", str(fake), *extra],
                              capture_output=True, text=True, timeout=120)


# Three jobs, because the guard refuses a scan that found almost nothing.
def three(**overrides: str) -> dict[str, str]:
    base = {
        "one": workflow("one", TOOLCHAIN + BUILD),
        "two": workflow("two", TOOLCHAIN + BUILD),
        "three": workflow("three", TOOLCHAIN + BUILD),
    }
    base.update(overrides)
    return base


print("a wave job that brings its toolchain passes")
good = run(three(), "--skip-image")
expect("three well-formed jobs pass", good.returncode == 0, good.stdout[-300:])

print("and one that does not is refused")
bad = run(three(three=workflow("three", BUILD)), "--skip-image")
expect("a compile with no toolchain step is refused", bad.returncode == 1, bad.stdout[-300:])
expect("and the job is named", "`three`" in bad.stdout, bad.stdout[-300:])

print("order is part of the rule")
late = run(three(three=workflow("three", CACHE + TOOLCHAIN + BUILD)), "--skip-image")
expect("a toolchain installed after the cache action is refused",
       late.returncode == 1, late.stdout[-400:])
expect("and it says which step already needed it",
       "rust-cache" in late.stdout, late.stdout[-400:])
early = run(three(three=workflow("three", TOOLCHAIN + CACHE + BUILD)), "--skip-image")
expect("and before it is fine", early.returncode == 0, early.stdout[-300:])

print("only jobs on a bought instance are judged")
hosted = run(three(three=workflow("three", BUILD, runner="ubuntu-latest")), "--skip-image")
expect("a hosted job that compiles is not this guard's business",
       hosted.returncode == 2 or "`three`" not in hosted.stdout,
       f"rc={hosted.returncode}: {hosted.stdout[-300:]}")
label = run(three(three=workflow("three", BUILD,
                                 runner="${{ needs.create-runner.outputs.label }}")), "--skip-image")
expect("the wave's own label counts as a bought instance", label.returncode == 1,
       label.stdout[-300:])

print("prose is not a compile")
prose = run(three(three=workflow(
    "three",
    "      - name: say something\n"
    "        run: |\n"
    "          # cargo build would be wrong here\n"
    "          echo 'this job does not run cargo'\n")), "--skip-image")
expect("a comment naming cargo needs no toolchain", prose.returncode == 0, prose.stdout[-300:])

print("a scan that finds nothing approves nothing")
thin = run({"one": workflow("one", TOOLCHAIN + BUILD)}, "--skip-image")
expect("one job is too few to be the real set", thin.returncode == 2, thin.stdout[-200:])
expect("and it says so", "approves everything" in thin.stdout, thin.stdout[-200:])
none = subprocess.run([sys.executable, str(GUARD), "--repo", "/nonexistent-tree"],
                      capture_output=True, text=True)
expect("no workflow directory is a refusal", none.returncode == 2, none.stdout[-200:])

print("the image side is half the repair, and it is checked where it lives")
with tempfile.TemporaryDirectory() as temporary:
    fake = pathlib.Path(temporary)
    (fake / ".github" / "workflows").mkdir(parents=True)
    for name, body in three().items():
        (fake / ".github" / "workflows" / f"{name}.yml").write_text(body)
    (fake / "scripts" / "cos").mkdir(parents=True)
    (fake / "scripts" / "ci").mkdir(parents=True)
    seal = fake / "scripts" / "cos" / "gate_instance_seal.sh"
    probe = fake / "scripts" / "ci" / "a_gate_image_answers_ssh.py"

    def judge(seal_text: str, probe_text: str) -> subprocess.CompletedProcess:
        seal.write_text(seal_text)
        probe.write_text(probe_text)
        return subprocess.run([sys.executable, str(GUARD), "--repo", str(fake)],
                              capture_output=True, text=True, timeout=60)

    whole = judge("ln -s $cargo_pad/cargo /usr/local/bin/cargo\ncargo --version\n",
                  'CHECKS = (("cargo answers", "cargo --version"),)\n')
    expect("a seal that links cargo and a probe that asks for it pass",
           whole.returncode == 0, whole.stdout[-300:])

    no_link = judge("echo sealing\ncargo --version\n",
                    'CHECKS = (("cargo answers", "cargo --version"),)\n')
    expect("a seal that stopped putting cargo in /usr/local/bin is refused",
           no_link.returncode == 1, no_link.stdout[-300:])

    no_probe = judge("ln -s $cargo_pad/cargo /usr/local/bin/cargo\ncargo --version\n",
                     'CHECKS = (("ssh answers", "true"),)\n')
    expect("a probe that stopped asking whether cargo answers is refused",
           no_probe.returncode == 1, no_probe.stdout[-300:])
    expect("and it says what would be published", "published as good" in no_probe.stdout,
           no_probe.stdout[-300:])

print("and this repository obeys the rule")
real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True, timeout=120)
expect("every wave job here brings its toolchain", real.returncode == 0, real.stdout[-800:])

print(f"test_a_wave_job_that_compiles_carries_its_toolchain: {done} case(s), {len(failures)} failure(s)")
sys.exit(1 if failures else 0)
