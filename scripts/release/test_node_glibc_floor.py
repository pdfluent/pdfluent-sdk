#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""node_glibc_floor.py reads the glibc needs off the bytes, and the Node
workflow both builds the gnu legs against glibc 2.17 and gates on it.

The fixtures are synthetic ELF objects built here: the header, a string
table and a .gnu.version_r section, which is all the reader looks at. The
needs they carry are the ones measured on real bindings: the 1.0.0
linux-x64-gnu package (max GLIBC_2.17) and the 1.1.0 r9d candidates
(GLIBC_2.39 from libc.so.6 on x86_64, GLIBC_2.43 from libm.so.6 on
aarch64).
"""
from __future__ import annotations

import io
import pathlib
import struct
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
WORKFLOW = REPO / ".github" / "workflows" / "node-bindings.yml"
sys.path.insert(0, str(HERE))

import node_glibc_floor as floor  # noqa: E402

GNU_TRIPLES = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu")


def make_elf(needs: dict[str, list[str]], *, is64: bool = True, little: bool = True) -> bytes:
    """An ELF object whose .gnu.version_r carries `needs` {library: [versions]}."""
    end = "<" if little else ">"
    strtab = bytearray(b"\0")

    def add(s: str) -> int:
        off = len(strtab)
        strtab.extend(s.encode() + b"\0")
        return off

    verneed = bytearray()
    libs = list(needs.items())
    for i, (lib, versions) in enumerate(libs):
        aux_size = 16 * len(versions)
        vn_next = 16 + aux_size if i < len(libs) - 1 else 0
        verneed += struct.pack(end + "HHIII", 1, len(versions), add(lib), 16, vn_next)
        for j, ver in enumerate(versions):
            vna_next = 16 if j < len(versions) - 1 else 0
            verneed += struct.pack(end + "IHHII", 0, 0, 2 + j, add(ver), vna_next)

    ehsize = 64 if is64 else 52
    strtab_off = ehsize
    verneed_off = strtab_off + len(strtab)
    shoff = verneed_off + len(verneed)
    shoff += (-shoff) % 8
    shentsize = 64 if is64 else 40
    sh_fmt = end + ("IIQQQQIIQQ" if is64 else "IIIIIIIIII")
    shdrs = b"".join(
        [
            struct.pack(sh_fmt, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            struct.pack(sh_fmt, 0, 3, 0, 0, strtab_off, len(strtab), 0, 0, 1, 0),
            struct.pack(
                sh_fmt, 0, floor.SHT_GNU_VERNEED, 0, 0, verneed_off, len(verneed), 1, len(libs), 4, 0
            ),
        ]
    )
    ident = b"\x7fELF" + bytes([2 if is64 else 1, 1 if little else 2, 1]) + b"\0" * 9
    if is64:
        rest = struct.pack(end + "HHIQQQIHHHHHH", 3, 62, 1, 0, 0, shoff, 0, ehsize, 0, 0, shentsize, 3, 0)
    else:
        rest = struct.pack(end + "HHIIIIIHHHHHH", 3, 40, 1, 0, 0, shoff, 0, ehsize, 0, 0, shentsize, 3, 0)
    header = ident + rest
    assert len(header) == ehsize
    body = header + bytes(strtab) + bytes(verneed)
    body += b"\0" * (shoff - len(body))
    return body + shdrs


# Measured: npm pack @pdfluent/node-linux-x64-gnu@1.0.0 (pow@GLIBC_2.2.5 ... max 2.17).
NEEDS_1_0_0 = {
    "libgcc_s.so.1": ["GCC_3.0", "GCC_3.3", "GCC_4.2.0"],
    "libm.so.6": ["GLIBC_2.2.5"],
    "libpthread.so.0": ["GLIBC_2.2.5", "GLIBC_2.3.2", "GLIBC_2.12"],
    "libc.so.6": ["GLIBC_2.2.5", "GLIBC_2.3", "GLIBC_2.9", "GLIBC_2.14", "GLIBC_2.17"],
}
# Measured: the r9d x86_64 candidate, built without zig.
NEEDS_R9D_X64 = {
    "libm.so.6": ["GLIBC_2.2.5", "GLIBC_2.27", "GLIBC_2.29", "GLIBC_2.35"],
    "libc.so.6": ["GLIBC_2.2.5", "GLIBC_2.17", "GLIBC_2.34", "GLIBC_2.39"],
}
# Measured: the r9d aarch64 candidate, built without zig.
NEEDS_R9D_ARM = {
    "libc.so.6": ["GLIBC_2.17", "GLIBC_2.18"],
    "libm.so.6": ["GLIBC_2.17", "GLIBC_2.43"],
}


class ReaderTests(unittest.TestCase):
    def test_reads_every_need(self):
        needs = floor.version_needs(make_elf(NEEDS_R9D_ARM))
        self.assertEqual(
            sorted(needs),
            sorted((lib, v) for lib, vs in NEEDS_R9D_ARM.items() for v in vs),
        )

    def test_reads_elf32_big_endian_too(self):
        needs = floor.version_needs(make_elf({"libc.so.6": ["GLIBC_2.39"]}, is64=False, little=False))
        self.assertEqual(needs, [("libc.so.6", "GLIBC_2.39")])

    def test_not_elf_is_refused(self):
        with self.assertRaises(floor.NotElf):
            floor.version_needs(b"MZ\x90\x00" + b"\0" * 60)


class FloorTests(unittest.TestCase):
    def run_check(self, data: bytes, max_: str = "2.17") -> tuple[bool, str]:
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "index.linux-x64-gnu.node"
            path.write_bytes(data)
            ok, lines = floor.check(path, floor._version(max_))
        return ok, "\n".join(lines)

    def test_the_1_0_0_binding_is_within_2_17(self):
        ok, out = self.run_check(make_elf(NEEDS_1_0_0))
        self.assertTrue(ok, out)
        self.assertIn("max GLIBC_2.17", out)

    def test_the_r9d_x64_candidate_fails_naming_2_39(self):
        ok, out = self.run_check(make_elf(NEEDS_R9D_X64))
        self.assertFalse(ok, out)
        self.assertIn("max GLIBC_2.39", out)
        self.assertIn("GLIBC_2.39 from libc.so.6", out)
        self.assertIn("GLIBC_2.35 from libm.so.6", out)
        # 2.17 is the floor itself, not above it.
        self.assertNotIn("GLIBC_2.17 from", out)

    def test_the_r9d_arm_candidate_fails_naming_2_43(self):
        ok, out = self.run_check(make_elf(NEEDS_R9D_ARM))
        self.assertFalse(ok, out)
        self.assertIn("GLIBC_2.43 from libm.so.6", out)
        self.assertIn("GLIBC_2.18 from libc.so.6", out)

    def test_versions_compare_as_numbers_not_strings(self):
        # "2.9" > "2.17" as strings; as versions it is below the floor.
        ok, out = self.run_check(make_elf({"libc.so.6": ["GLIBC_2.9", "GLIBC_2.2.5"]}))
        self.assertTrue(ok, out)
        ok, out = self.run_check(make_elf({"libc.so.6": ["GLIBC_2.17.1"]}))
        self.assertFalse(ok, out)

    def test_no_glibc_need_fails(self):
        # A musl or mac binary under a gnu name asks glibc for nothing;
        # passing it would make the floor vacuous.
        ok, out = self.run_check(make_elf({"libgcc_s.so.1": ["GCC_3.0"]}))
        self.assertFalse(ok, out)
        self.assertIn("needs no GLIBC_ version", out)

    def test_glibc_private_fails(self):
        ok, out = self.run_check(make_elf({"libc.so.6": ["GLIBC_2.17", "GLIBC_PRIVATE"]}))
        self.assertFalse(ok, out)
        self.assertIn("GLIBC_PRIVATE", out)

    def test_cli_exit_codes(self):
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "good.node"
            bad = pathlib.Path(tmp) / "bad.node"
            good.write_bytes(make_elf(NEEDS_1_0_0))
            bad.write_bytes(make_elf(NEEDS_R9D_X64))
            missing = pathlib.Path(tmp) / "missing.node"
            sink = io.StringIO()
            with redirect_stdout(sink), redirect_stderr(sink):
                self.assertEqual(floor.main(["--max", "2.17", str(good)]), 0)
                self.assertEqual(floor.main(["--max", "2.17", str(good), str(bad)]), 1)
                self.assertEqual(floor.main(["--max", "2.17", str(missing)]), 1)
                self.assertEqual(floor.main(["--max", "two", str(good)]), 2)
            self.assertIn("GLIBC_2.39", sink.getvalue())


class WorkflowTests(unittest.TestCase):
    """The workflow builds the gnu legs against 2.17 and gates on it."""

    @classmethod
    def setUpClass(cls):
        cls.text = WORKFLOW.read_text()

    def _job(self, job: str, next_job: str) -> str:
        return self.text[self.text.index(f"  {job}:") : self.text.index(f"  {next_job}:")]

    def _step_block(self, job_text: str, name_prefix: str) -> str:
        """The text of the first step in job_text whose name starts with name_prefix."""
        lines = job_text.splitlines()
        for i, line in enumerate(lines):
            if line.strip().startswith(f"- name: {name_prefix}"):
                indent = len(line) - len(line.lstrip())
                out = [line]
                for nxt in lines[i + 1 :]:
                    stripped = nxt.lstrip()
                    depth = len(nxt) - len(stripped)
                    # The next step (or the job's end) closes this one.
                    if stripped and (depth < indent or (depth == indent and stripped.startswith("- "))):
                        break
                    out.append(nxt)
                return "\n".join(out)
        self.fail(f"no step named {name_prefix!r} in {WORKFLOW.name}")

    def test_release_gate_runs_the_floor_over_both_gnu_bindings(self):
        gate = self._job("release-gate", "publish-npm")
        runs = [l for l in gate.splitlines() if "node_glibc_floor.py" in l and not l.strip().startswith("#")]
        self.assertEqual(len(runs), 1, runs)
        self.assertIn("--max 2.17", runs[0])
        self.assertIn("index.linux-x64-gnu.node", runs[0])
        self.assertIn("index.linux-arm64-gnu.node", runs[0])
        step = self._step_block(gate, "The gnu bindings need no glibc above 2.17")
        self.assertNotIn("continue-on-error", step)

    def test_gnu_legs_build_through_zig_at_2_17(self):
        build = self._step_block(self._job("build-release", "release-gate"), "Build for")
        lines = [l for l in build.splitlines() if "napi build" in l and not l.strip().startswith("#")]
        zig_217 = [l for l in lines if "--zig --zig-abi-suffix 2.17" in l]
        self.assertEqual(len(zig_217), 1, lines)
        # The arm that carries it is the gnu arm.
        idx = build.index(zig_217[0])
        arm_labels = [l.strip() for l in build[:idx].splitlines() if l.strip().endswith(")") and not l.strip().startswith("#")]
        self.assertEqual(arm_labels[-1], "*-linux-gnu)")
        # And no gnu build is left without zig: the fallback arm builds
        # mac and Windows only.
        self.assertNotIn("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: aarch64-linux-gnu-gcc", build)

    def test_gnu_legs_install_zig_on_both_pools(self):
        job = self._job("build-release", "release-gate")
        setup = self._step_block(job, "Set up zig for the self-hosted")
        preflight = self._step_block(job, "Preflight the release requirements")
        linker = self._step_block(job, "Cross linker for")
        for triple in GNU_TRIPLES:
            self.assertIn(triple, setup, "self-hosted zig setup")
            self.assertIn(triple, preflight, "preflight --without")
            self.assertIn(triple, linker, "hosted-pool zig install")
        self.assertIn("ziglang", linker)


if __name__ == "__main__":
    unittest.main()
