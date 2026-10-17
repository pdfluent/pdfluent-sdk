#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""NativeLoader reads musl off the JVM's own interpreter.

`isMuslLibc` used to glob `/lib` for `ld-musl-*`: a glibc host with the musl
package installed answered musl, and the loader picked the wrong natives. Now
it reads the PT_INTERP of the java launcher. This test compiles the shipped
NativeLoader (plus the same-package files it touches) with javac and runs
a small TestMain against fixture binaries — a musl ELF64, a glibc ELF64, a
musl ELF32, an ELF without interpreter, a Mach-O, garbage, a truncation and
a missing file — so the parse is judged on behaviour, not on source text.
Without the fix the TestMain does not compile (no such methods).

Without a working JDK — a Mac whose only javac is the /usr/bin stub, or no
JNA jar under ~/.m2 — the compile cases skip as `SKIPPED (not a pass)` on
stderr, never as a silent pass: this file sits in the fast lane, and a
missing toolchain is not this branch's fault. ci.yml provisions both
(setup-java plus the JNA fetch) so the test really runs there. In the
release lane (PDFLUENT_RELEASE_LANE=1, which ci.yml sets on this step) a
skip is a failure instead: a lane that vouches for natives must RED when
it cannot compile the check, not report green over an unrun one.
"""
from __future__ import annotations

import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
SRC = REPO / "bindings" / "java" / "src" / "main" / "java" / "com" / "pdfluent"

MINIMUM_CASES = 3  # FLOOR

SELFTEST_GUARD = "PDFLUENT_MUSL_SKIP_SELFTEST"

TEST_MAIN = """\
package com.pdfluent;

import java.nio.file.Path;
import java.nio.file.Paths;

public final class TestMain {
    private TestMain() {}

    static void check(boolean cond, String what) {
        if (!cond) {
            throw new AssertionError(what);
        }
        System.out.println("ok - " + what);
    }

    public static void main(String[] args) {
        Path dir = Paths.get(args[0]);
        check("/lib/ld-musl-x86_64.so.1".equals(
                NativeLoader.readInterpreter(dir.resolve("elf64-musl"))),
                "musl ELF64 interpreter");
        check(NativeLoader.isMuslInterpreter("/lib/ld-musl-x86_64.so.1"),
                "musl interpreter is musl");
        check("/lib64/ld-linux-x86-64.so.2".equals(
                NativeLoader.readInterpreter(dir.resolve("elf64-gnu"))),
                "glibc ELF64 interpreter");
        check(!NativeLoader.isMuslInterpreter("/lib64/ld-linux-x86-64.so.2"),
                "glibc interpreter is not musl");
        check("/lib/ld-musl-armhf.so.1".equals(
                NativeLoader.readInterpreter(dir.resolve("elf32-musl"))),
                "musl ELF32 interpreter");
        check(NativeLoader.readInterpreter(dir.resolve("elf64-nointerp")) == null,
                "ELF without PT_INTERP reads null");
        check(NativeLoader.readInterpreter(dir.resolve("mach-o")) == null,
                "Mach-O reads null");
        check(NativeLoader.readInterpreter(dir.resolve("garbage")) == null,
                "garbage reads null");
        check(NativeLoader.readInterpreter(dir.resolve("truncated")) == null,
                "truncated file reads null");
        check(NativeLoader.readInterpreter(dir.resolve("missing")) == null,
                "missing file reads null");
        check("linux-x64-musl".equals(
                NativeLoader.platformKey("Linux", "x86_64", true)),
                "platform key wiring unchanged");
        System.out.println("ALL-PASS");
    }
}
"""


def _elf64(interp: str | None) -> bytes:
    blob = bytearray(512)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2  # 64-bit
    struct.pack_into("<Q", blob, 32, 64)  # e_phoff
    struct.pack_into("<H", blob, 54, 56)  # e_phentsize
    struct.pack_into("<H", blob, 56, 2 if interp else 1)  # e_phnum
    struct.pack_into("<I", blob, 64, 1)  # PT_LOAD
    if interp:
        struct.pack_into("<I", blob, 120, 3)  # PT_INTERP
        struct.pack_into("<Q", blob, 128, 256)  # p_offset
        raw = interp.encode() + b"\0"
        blob[256 : 256 + len(raw)] = raw
    return bytes(blob)


def _elf32(interp: str) -> bytes:
    blob = bytearray(256)
    blob[0:4] = b"\x7fELF"
    blob[4] = 1  # 32-bit
    struct.pack_into("<I", blob, 28, 52)  # e_phoff
    struct.pack_into("<H", blob, 42, 32)  # e_phentsize
    struct.pack_into("<H", blob, 44, 1)  # e_phnum
    struct.pack_into("<I", blob, 52, 3)  # PT_INTERP
    struct.pack_into("<I", blob, 56, 128)  # p_offset
    raw = interp.encode() + b"\0"
    blob[128 : 128 + len(raw)] = raw
    return bytes(blob)


def _fixtures(root: pathlib.Path) -> None:
    (root / "elf64-musl").write_bytes(_elf64("/lib/ld-musl-x86_64.so.1"))
    (root / "elf64-gnu").write_bytes(_elf64("/lib64/ld-linux-x86-64.so.2"))
    (root / "elf32-musl").write_bytes(_elf32("/lib/ld-musl-armhf.so.1"))
    (root / "elf64-nointerp").write_bytes(_elf64(None))
    (root / "mach-o").write_bytes(
        b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + b"\0" * 100
    )
    (root / "garbage").write_bytes(b"not an elf file at all" + b"\0" * 100)
    (root / "truncated").write_bytes(b"\x7fELF\x02")


def _jna_jar() -> pathlib.Path | None:
    hits = sorted(
        pathlib.Path.home().glob(
            ".m2/repository/net/java/dev/jna/jna/*/jna-*.jar"
        )
    )
    non_sources = [h for h in hits if "sources" not in h.name and "javadoc" not in h.name]
    return non_sources[-1] if non_sources else None


JNA_HOW = "run `cd bindings/java && mvn dependency:resolve` once"


def _tool_reason(tool: str) -> str:
    """Why `tool` cannot compile here, or "" when `<tool> -version` runs.

    `which` alone is not the check: a Mac without a JDK still carries the
    /usr/bin/javac stub, which is on PATH and exits non-zero.
    """
    path = shutil.which(tool)
    if path is None:
        return f"{tool} not on PATH"
    try:
        proc = subprocess.run(
            [path, "-version"],
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
            timeout=60,
        )
    except (OSError, subprocess.SubprocessError) as e:
        return f"{tool} at {path} does not start ({e})"
    if proc.returncode != 0:
        detail = (proc.stderr.strip() or proc.stdout.strip()).splitlines()
        hint = f": {detail[0][:160]}" if detail else ""
        return f"{tool} at {path} is not a working JDK (exit {proc.returncode}{hint})"
    return ""


def _in_release_lane() -> bool:
    return os.environ.get("PDFLUENT_RELEASE_LANE") == "1"


def _skip_or_fail(case: unittest.TestCase, reason: str) -> None:
    """Skip in the fast lane; fail in the release lane.

    A missing toolchain is not the branch's fault on a developer machine,
    but a release lane that cannot compile this check must go RED — green
    over an unrun check is the pass this file exists to refuse.
    """
    if _in_release_lane():
        case.fail(f"release lane: {reason} (a skip is a failure here)")
    print(f"SKIPPED (not a pass): {reason}", file=sys.stderr)
    case.skipTest(reason)


def _require_jdk(case: unittest.TestCase) -> None:
    for tool in ("javac", "java"):
        reason = _tool_reason(tool)
        if reason:
            _skip_or_fail(case, reason)


def _require_jna(case: unittest.TestCase) -> pathlib.Path:
    jna = _jna_jar()
    if jna is None:
        reason = (
            "no JNA jar under ~/.m2/repository "
            f"(NativeLoader imports com.sun.jna; {JNA_HOW})"
        )
        _skip_or_fail(case, reason)
    assert jna is not None
    return jna


def _run_self_with_stub_javac(extra_env: dict[str, str]) -> subprocess.CompletedProcess:
    """This file, run with a stub javac/java fronting the PATH.

    A Mac without a JDK: /usr/bin/javac exists and exits non-zero.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        for tool in ("javac", "java"):
            stub = root / tool
            stub.write_text(
                "#!/bin/sh\n"
                'echo "xcode-select: note: no developer tools were found" >&2\n'
                "exit 1\n"
            )
            stub.chmod(0o755)
        env = dict(os.environ)
        env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
        env[SELFTEST_GUARD] = "1"
        env.update(extra_env)
        return subprocess.run(
            [sys.executable, str(pathlib.Path(__file__).resolve())],
            capture_output=True,
            text=True,
            timeout=300,
            env=env,
        )


class NativeLoaderMuslTests(unittest.TestCase):
    def test_javac_and_java_run(self):
        _require_jdk(self)

    def test_interpreter_parse_against_fixture_binaries(self):
        _require_jdk(self)
        jna = _require_jna(self)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _fixtures(root)
            src = root / "src" / "com" / "pdfluent"
            src.mkdir(parents=True)
            for name in (
                "NativeLoader.java",
                "PdfCapiLibrary.java",
                "PdfluentException.java",
                "PdfluentNativeLoadException.java",
            ):
                (src / name).write_bytes((SRC / name).read_bytes())
            (src / "TestMain.java").write_text(TEST_MAIN)
            out = root / "classes"
            javac = subprocess.run(
                [
                    "javac", "-source", "11", "-target", "11",
                    "-cp", str(jna), "-d", str(out),
                    str(src / "NativeLoader.java"),
                    str(src / "PdfCapiLibrary.java"),
                    str(src / "PdfluentException.java"),
                    str(src / "PdfluentNativeLoadException.java"),
                    str(src / "TestMain.java"),
                ],
                capture_output=True,
                text=True,
                timeout=180,
            )
            self.assertEqual(javac.returncode, 0, javac.stderr)
            run = subprocess.run(
                ["java", "-cp", f"{out}{':'}{jna}", "com.pdfluent.TestMain", str(root)],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertIn("ALL-PASS", run.stdout)

    def test_no_lib_glob_remains_in_the_loader(self):
        text = (SRC / "NativeLoader.java").read_text()
        self.assertNotIn("newDirectoryStream", text)
        self.assertNotIn('"/lib"', text)

    def test_stub_javac_skips_with_the_house_message(self):
        # A Mac without a JDK: /usr/bin/javac exists and exits non-zero.
        # `which` finds it, so the old check compiled with the stub and went
        # red; now the file skips, announcing itself on stderr. The dev lane
        # is explicit: the lane variable is scrubbed so a release-lane
        # parent cannot turn this skip red.
        if os.environ.get(SELFTEST_GUARD):
            return
        scrubbed = dict(os.environ)
        scrubbed.pop("PDFLUENT_RELEASE_LANE", None)
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for tool in ("javac", "java"):
                stub = root / tool
                stub.write_text(
                    "#!/bin/sh\n"
                    'echo "xcode-select: note: no developer tools were found" >&2\n'
                    "exit 1\n"
                )
                stub.chmod(0o755)
            env = dict(scrubbed)
            env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
            env[SELFTEST_GUARD] = "1"
            proc = subprocess.run(
                [sys.executable, str(pathlib.Path(__file__).resolve())],
                capture_output=True,
                text=True,
                timeout=300,
                env=env,
            )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("SKIPPED (not a pass)", proc.stderr)

    def test_release_lane_turns_a_missing_toolchain_red(self):
        # In the release lane a skip is a failure: without a JDK this file
        # must go red naming the toolchain, not SKIPPED-green.
        if os.environ.get(SELFTEST_GUARD):
            return
        proc = _run_self_with_stub_javac({"PDFLUENT_RELEASE_LANE": "1"})
        self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("release lane", proc.stdout + proc.stderr)
        self.assertIn("a skip is a failure", proc.stdout + proc.stderr)

    def test_ci_provisions_a_jdk_and_the_jna_jar(self):
        # The skip above must never be what CI reports: the lane provisions
        # both halves, so this file really compiles there — and the step
        # runs in the release lane, so a provisioning failure goes red
        # instead of SKIPPED-green.
        import yaml

        doc = yaml.safe_load(
            (REPO / ".github" / "workflows" / "ci.yml").read_text()
        )
        steps = doc["jobs"]["orchestration-guard"]["steps"]
        runs = [str(s.get("run", "")) for s in steps if isinstance(s, dict)]
        uses = [str(s.get("uses", "")) for s in steps if isinstance(s, dict)]
        self.assertIn("actions/setup-java@v4", uses)
        java_at = uses.index("actions/setup-java@v4")
        jna_at = next(
            i for i, r in enumerate(runs) if "repo1.maven.org" in r
        )
        musl_at = next(
            i for i, r in enumerate(runs)
            if "test_native_loader_musl.py" in r
        )
        self.assertLess(java_at, musl_at)
        self.assertLess(jna_at, musl_at)
        musl_step = steps[musl_at]
        lane = (musl_step.get("env") or {}).get("PDFLUENT_RELEASE_LANE")
        self.assertEqual(
            str(lane), "1",
            "the NativeLoader step runs in the release lane, so a skip is red",
        )


if __name__ == "__main__":
    unittest.main()
