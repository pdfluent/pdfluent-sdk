#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""cli_build_archives.sh stages the gate's Linux triples, and no ldd.

The script kept its own default triple list beside the canonical platform
table: a platform added to the gate was not staged until somebody edited a
second list. The default now comes from CLI_PLATFORMS' Linux rows, and the
musl fail-fast reads the staged binary's own headers through the gate's
elf_is_static instead of asking ldd — which on an x86 host calls every
arm64 binary "not a dynamic executable" and waves a dynamic one through.

The script compiles nothing: it stages archives from prebuilt binaries
passed with --binary, one per triple. Each case runs the script in a
fixture tree with stub binaries, so the triple defaulting, the static
verdict and the archive layout are judged on behaviour, with no toolchain
and no network.
"""
from __future__ import annotations

import importlib.util
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "artifacts_cover_every_platform", HERE / "artifacts_cover_every_platform.py"
)
gate = importlib.util.module_from_spec(_spec)
sys.modules["artifacts_cover_every_platform"] = gate
_spec.loader.exec_module(gate)

REPO = HERE.parents[1]
SCRIPT = HERE / "cli_build_archives.sh"

MINIMUM_CASES = 4  # FLOOR

MACHINE = {"x86-64": 62, "aarch64": 183}
INTERP = {
    "x86-64": ("/lib64/ld-linux-x86-64.so.2", "libc.so.6"),
    "aarch64": ("/lib/ld-musl-aarch64.so.1", "libc.musl-aarch64.so.1"),
}




def _fixture_binary(*, arch: str, static: bool) -> bytes:
    """A minimal x86-64/aarch64 ELF64: static, or dynamic with interp+NEEDED."""
    blob = bytearray(2048)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2
    blob[5] = 1
    struct.pack_into("<HHI", blob, 16, 2, MACHINE[arch], 1)
    struct.pack_into("<Q", blob, 32, 64)
    struct.pack_into("<H", blob, 54, 56)
    struct.pack_into("<H", blob, 56, 3 if static else 3)
    struct.pack_into("<IIQQQQQQ", blob, 64, 1, 5, 0, 0x400000, 0, 2048, 2048, 0x1000)
    if static:
        struct.pack_into("<H", blob, 56, 1)
        return bytes(blob)
    interp, soname = INTERP[arch]
    raw = interp.encode() + b"\0"
    struct.pack_into(
        "<IIQQQQQQ", blob, 120, 3, 4, 512, 0x400000 + 512, 0,
        len(raw), len(raw), 1,
    )
    blob[512 : 512 + len(raw)] = raw
    dyn_off, str_off = 640, 768
    struct.pack_into(
        "<IIQQQQQQ", blob, 176, 2, 4, dyn_off, 0x400000 + dyn_off, 0, 64, 64, 8
    )
    strtab = b"\0" + soname.encode() + b"\0"
    struct.pack_into("<qQ", blob, dyn_off, 1, 1)
    struct.pack_into("<qQ", blob, dyn_off + 16, 5, 0x400000 + str_off)
    struct.pack_into("<qQ", blob, dyn_off + 32, 10, len(strtab))
    struct.pack_into("<qQ", blob, dyn_off + 48, 0, 0)
    blob[str_off : str_off + len(strtab)] = strtab
    return bytes(blob)


def _fixture_tree(root: pathlib.Path) -> pathlib.Path:
    """A tree the script runs in: itself, the gate, and the staged texts."""
    for rel in (
        "scripts/release/cli_build_archives.sh",
        "scripts/release/artifacts_cover_every_platform.py",
        "scripts/release/license_audit.py",
        "crates/pdfluent-cli/Cargo.toml",
        "crates/pdfluent-cli/README.md",
        "LICENSE",
        "LICENSE-COMMERCIAL",
        "NOTICE",
        "THIRD_PARTY_LICENSES.txt",
    ):
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO / rel, target)
    shutil.copymode(REPO / "scripts/release/cli_build_archives.sh",
                    root / "scripts/release/cli_build_archives.sh")
    man = root / "docs" / "cli" / "man"
    man.mkdir(parents=True, exist_ok=True)
    for page in (REPO / "docs" / "cli" / "man").glob("*.1"):
        shutil.copy2(page, man / page.name)
    return root


def _run(
    root: pathlib.Path, binaries: list[bytes], *args: str
) -> subprocess.CompletedProcess:
    # The script stages what it is given: each stub binary is laid down as
    # a prebuilt and passed with --binary, in triple order.
    prebuilt = root / "prebuilt"
    prebuilt.mkdir(exist_ok=True)
    binary_args: list[str] = []
    for i, blob in enumerate(binaries):
        stub = prebuilt / f"pdfluent-stub-{i}"
        stub.write_bytes(blob)
        stub.chmod(0o755)
        binary_args += ["--binary", str(stub)]
    return subprocess.run(
        ["bash", str(root / "scripts/release/cli_build_archives.sh"),
         "--out", str(root / "out"), "--skip-gate", *binary_args, *args],
        capture_output=True,
        text=True,
        timeout=180,
        cwd=root,
    )


def _gate_linux_triples() -> list[str]:
    return sorted(
        triple for plat, triple, _ in gate.CLI_PLATFORMS if plat.startswith("linux")
    )


class CliBuildArchivesTests(unittest.TestCase):
    def test_default_triples_are_the_gate_linux_triples(self):
        # The default set is derived, not listed: the script imports the
        # canonical table, and no literal triple list remains beside it.
        body = SCRIPT.read_text()
        self.assertIn("artifacts_cover_every_platform", body)
        self.assertIn("CLI_PLATFORMS", body)
        self.assertNotIn("TRIPLES=(x86_64", body)
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            # One stub per default triple, in the table's order: the
            # script pairs --binary with its derived triples by position.
            default = [
                triple
                for plat, triple, _ in gate.CLI_PLATFORMS
                if plat.startswith("linux")
            ]
            stubs = [
                _fixture_binary(arch="x86-64", static=True) for _ in default
            ]
            proc = _run(root, stubs)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            line = next(
                l for l in proc.stdout.splitlines() if "triples=" in l
            )
            got = sorted(line.split("triples=", 1)[1].split())
            self.assertEqual(got, _gate_linux_triples(), proc.stdout + proc.stderr)

    def test_static_musl_binary_is_staged(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            proc = _run(
                root,
                [_fixture_binary(arch="x86-64", static=True)],
                "--triple", "x86_64-unknown-linux-musl",
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("statically linked", proc.stdout)
            archives = sorted((root / "out").glob("pdfluent-*.tar.gz"))
            self.assertEqual(len(archives), 1, proc.stdout + proc.stderr)

    def test_dynamic_musl_binary_is_refused(self):
        # The verdict reads the binary's own headers: a dynamic musl
        # binary fails here, naming the triple, on whatever host.
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            proc = _run(
                root,
                [_fixture_binary(arch="x86-64", static=False)],
                "--triple", "x86_64-unknown-linux-musl",
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("dynamically linked", proc.stdout + proc.stderr)
            self.assertIn("x86_64-unknown-linux-musl", proc.stdout + proc.stderr)

    def test_dynamic_arm64_musl_binary_is_refused_on_this_host(self):
        # The cross case ldd could never judge: an arm64 binary on an x86
        # host, where ldd answers "not a dynamic executable" for everything.
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            proc = _run(
                root,
                [_fixture_binary(arch="aarch64", static=False)],
                "--triple", "aarch64-unknown-linux-musl",
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("dynamically linked", proc.stdout + proc.stderr)

    def test_no_ldd_remains_in_the_static_verdict(self):
        body = SCRIPT.read_text()
        self.assertIn("elf_is_static", body)
        self.assertNotIn("command -v ldd", body)
        self.assertNotIn("not a dynamic executable", body)

    def test_archives_carry_the_man_pages(self):
        # The R1 pages ride every archive under man/man1/: the gate
        # requires man/man1/pdfluent.1 and Homebrew installs the rest.
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            proc = _run(
                root,
                [_fixture_binary(arch="x86-64", static=True)],
                "--triple", "x86_64-unknown-linux-gnu",
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            archives = sorted((root / "out").glob("pdfluent-*.tar.gz"))
            self.assertEqual(len(archives), 1, proc.stdout + proc.stderr)
            with tarfile.open(archives[0], "r:gz") as tf:
                names = tf.getnames()
            version = re.search(
                r'^version\s*=\s*"([^"]+)"',
                (root / "crates" / "pdfluent-cli" / "Cargo.toml").read_text(),
                re.M,
            ).group(1)
            top = f"pdfluent-{version}-x86_64-unknown-linux-gnu"
            self.assertIn(f"{top}/man/man1/pdfluent.1", names)
            # The archive layout the gate watches: the binary, both
            # licence halves, the notice set, and the crate README.
            for member in (
                f"{top}/pdfluent",
                f"{top}/LICENSE",
                f"{top}/LICENSE-COMMERCIAL",
                f"{top}/NOTICE",
                f"{top}/THIRD_PARTY_LICENSES.txt",
                f"{top}/README.md",
            ):
                self.assertIn(member, names)

    def test_binary_count_must_match_the_triples(self):
        # One prebuilt per triple, in order: a short --binary list names
        # both counts instead of staging the wrong binary under a triple.
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            proc = _run(
                root,
                [_fixture_binary(arch="x86-64", static=True)],
                "--triple", "x86_64-unknown-linux-gnu",
                "--triple", "aarch64-unknown-linux-gnu",
            )
            self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
            self.assertIn("1 --binary for 2 triples", proc.stdout + proc.stderr)

    def test_old_binary_name_is_refused(self):
        # The [[bin]] rename has landed: a prebuilt still called
        # pdfluent-cli is refused, not staged as pdfluent.
        with tempfile.TemporaryDirectory() as tmp:
            root = _fixture_tree(pathlib.Path(tmp))
            legacy = root / "pdfluent-cli"
            legacy.write_bytes(_fixture_binary(arch="x86-64", static=True))
            proc = subprocess.run(
                ["bash", str(root / "scripts/release/cli_build_archives.sh"),
                 "--out", str(root / "out"), "--skip-gate",
                 "--triple", "x86_64-unknown-linux-gnu",
                 "--binary", str(legacy)],
                capture_output=True,
                text=True,
                timeout=180,
                cwd=root,
            )
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("still named pdfluent-cli", proc.stdout + proc.stderr)


class ZipCliStageTests(unittest.TestCase):
    """zip_cli_stage.py zips the stage with forward-slash entries.

    Build 2: Compress-Archive wrote man\\man1\\pdfluent.1 into the win-x64
    zip, and the gate refused the archive. Both Windows legs cut their zip
    through the helper now, on every pool.
    """

    HELPER = HERE / "zip_cli_stage.py"

    def _stage(self, root: pathlib.Path) -> pathlib.Path:
        stage = root / "stage"
        (stage / "man" / "man1").mkdir(parents=True)
        (stage / "pdfluent.exe").write_bytes(b"\0" * 64)
        (stage / "man" / "man1" / "pdfluent.1").write_text(".TH PDFLUENT 1\n")
        return stage

    def test_zip_holds_the_stage_contents_with_forward_slashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            out = root / "pdfluent-1.1.0-x86_64-pc-windows-msvc.zip"
            proc = subprocess.run(
                [sys.executable, str(self.HELPER),
                 str(self._stage(root)), str(out)],
                capture_output=True, text=True, timeout=120,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            with zipfile.ZipFile(out) as zf:
                names = zf.namelist()
            self.assertIn("man/man1/pdfluent.1", names)
            self.assertIn("pdfluent.exe", names)
            self.assertEqual([n for n in names if "\\" in n], [])

    def test_missing_stage_dir_fails_naming_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            proc = subprocess.run(
                [sys.executable, str(self.HELPER),
                 str(root / "no-such-stage"), str(root / "out.zip")],
                capture_output=True, text=True, timeout=120,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("no-such-stage", proc.stdout + proc.stderr)

    def test_empty_stage_dir_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "stage").mkdir()
            proc = subprocess.run(
                [sys.executable, str(self.HELPER),
                 str(root / "stage"), str(root / "out.zip")],
                capture_output=True, text=True, timeout=120,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
