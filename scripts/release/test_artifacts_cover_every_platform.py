#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the artefact gate (m30 requirement 1).

For each channel: a fixture artefact with ONE platform removed turns the gate
red with that platform's name in the message; a complete fixture turns it
green; a payload below the size floor turns it red. Each case builds its
fixture here, so the test says what the rule is rather than what one tree
happens to contain.
"""
from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import pathlib
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

MINIMUM_CASES = 29  # FLOOR

# Fixture natives are 2 KB with a valid header for their slot (kind, arch and
# libc markers the identity check reads); the floor in these tests is set to
# match (min_native_bytes=1000, margin=0 keeps the arithmetic exact).
MIN_NATIVE = 1000
MARGIN = 0.0

_AGPL_TEXT = b"GNU AFFERO GENERAL PUBLIC LICENSE\n" + b"terms...\n" * 20
_COMMERCIAL_TEXT = b"PDFluent Commercial Licence\n" + b"terms...\n" * 20


def _elf_shared(machine: int, soname: str | tuple[str, ...]) -> bytes:
    """A minimal ELF64 shared object whose DT_NEEDED names `soname`.

    What the libc verdict reads: one PT_LOAD (so the strtab address
    resolves through a segment, as in a real object) and one PT_DYNAMIC
    with DT_NEEDED, DT_STRTAB, DT_STRSZ and DT_NULL. No other markers:
    stray bytes elsewhere in the file must not move the verdict. A tuple
    names several DT_NEEDED entries, for the allowlist cases.
    """
    import struct

    sonames = (soname,) if isinstance(soname, str) else soname
    blob = bytearray(2048)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2  # 64-bit
    blob[5] = 1  # little-endian
    struct.pack_into("<HHI", blob, 16, 3, machine, 1)  # ET_DYN
    struct.pack_into("<Q", blob, 32, 64)  # e_phoff
    struct.pack_into("<H", blob, 54, 56)  # e_phentsize
    struct.pack_into("<H", blob, 56, 2)  # e_phnum
    # PT_LOAD covering the file, at a non-zero base so the VA mapping is real.
    struct.pack_into("<IIQQQQQQ", blob, 64, 1, 5, 0, 0x400000, 0, 2048, 2048, 0x1000)
    dyn_off, str_off = 512, 640
    struct.pack_into(
        "<IIQQQQQQ", blob, 120, 2, 4, dyn_off, 0x400000 + dyn_off, 0,
        (len(sonames) + 3) * 16, (len(sonames) + 3) * 16, 8
    )  # PT_DYNAMIC
    strtab = bytearray(b"\0")
    doff = dyn_off
    for name in sonames:
        struct.pack_into("<qQ", blob, doff, 1, len(strtab))  # DT_NEEDED
        doff += 16
        strtab += name.encode() + b"\0"
    struct.pack_into("<qQ", blob, doff, 5, 0x400000 + str_off)  # DT_STRTAB
    struct.pack_into("<qQ", blob, doff + 16, 10, len(strtab))  # DT_STRSZ
    struct.pack_into("<qQ", blob, doff + 32, 0, 0)  # DT_NULL
    blob[str_off : str_off + len(strtab)] = bytes(strtab)
    return bytes(blob)


_MUSL_SONAME = {"x86-64": "libc.musl-x86_64.so.1", "aarch64": "libc.musl-aarch64.so.1"}
_GNU_SONAME = "libc.so.6"
_GNU_INTERP = {
    "x86-64": "/lib64/ld-linux-x86-64.so.2",
    "aarch64": "/lib/ld-linux-aarch64.so.1",
}
_MUSL_INTERP = {
    "x86-64": "/lib/ld-musl-x86_64.so.1",
    "aarch64": "/lib/ld-musl-aarch64.so.1",
}
_ELF_MACHINE = {"x86-64": 62, "aarch64": 183}


def _elf_executable(
    *,
    arch: str = "x86-64",
    etype: int = 2,
    interp: str | None = None,
    needed: tuple[str, ...] | None = None,
) -> bytes:
    """A minimal ELF64 executable with the given linkage.

    `needed=None` means no PT_DYNAMIC at all (a fully static link);
    `needed=()` means a PT_DYNAMIC with no DT_NEEDED in it (static-pie).
    Either plus no interpreter is static; an interpreter or any DT_NEEDED
    is dynamic. The musl static check reads exactly this, off the bytes,
    so it judges arm64 on an x86 runner.
    """
    import struct

    machine = _ELF_MACHINE[arch]
    blob = bytearray(2048)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2  # 64-bit
    blob[5] = 1  # little-endian
    struct.pack_into("<HHI", blob, 16, etype, machine, 1)
    struct.pack_into("<Q", blob, 32, 64)  # e_phoff
    struct.pack_into("<H", blob, 54, 56)  # e_phentsize
    phnum = 1 + (1 if interp else 0) + (1 if needed is not None else 0)
    struct.pack_into("<H", blob, 56, phnum)  # e_phnum
    struct.pack_into("<IIQQQQQQ", blob, 64, 1, 5, 0, 0x400000, 0, 2048, 2048, 0x1000)
    slot = 120
    if interp:
        raw = interp.encode() + b"\0"
        struct.pack_into(
            "<IIQQQQQQ", blob, slot, 3, 4, 512, 0x400000 + 512, 0,
            len(raw), len(raw), 1,
        )  # PT_INTERP
        blob[512 : 512 + len(raw)] = raw
        slot += 56
    if needed is not None:
        dyn_off, str_off = 640, 768
        struct.pack_into(
            "<IIQQQQQQ", blob, slot, 2, 4, dyn_off, 0x400000 + dyn_off, 0,
            (len(needed) + 3) * 16, (len(needed) + 3) * 16, 8,
        )  # PT_DYNAMIC
        strtab = bytearray(b"\0")
        doff = dyn_off
        for name in needed:
            struct.pack_into("<qQ", blob, doff, 1, len(strtab))  # DT_NEEDED
            doff += 16
            strtab += name.encode() + b"\0"
        struct.pack_into("<qQ", blob, doff, 5, 0x400000 + str_off)  # DT_STRTAB
        struct.pack_into("<qQ", blob, doff + 16, 10, len(strtab))  # DT_STRSZ
        struct.pack_into("<qQ", blob, doff + 32, 0, 0)  # DT_NULL
        blob[str_off : str_off + len(strtab)] = bytes(strtab)
    return bytes(blob)


def _native_fat(archs: tuple[str, ...] = ("x86-64", "aarch64")) -> bytes:
    """A fat Mach-O whose header lists `archs` (big-endian fat_arch).

    The gate reads only the fat header, so the members are padding: what
    matters is which cputypes the header names.
    """
    import struct

    cputype = {"x86-64": 0x01000007, "aarch64": 0x0100000C}
    head = b"\xca\xfe\xba\xbe" + struct.pack(">I", len(archs))
    off = 8 + 20 * len(archs)
    for arch in archs:
        head += struct.pack(">IIIII", cputype[arch], 3, off, 512, 12)
        off += 512
    return head.ljust(2048, b"\0")


def _native(kind: str, arch: str, libc: str | None) -> bytes:
    """Minimal bytes the identity check reads as (kind, arch, libc)."""
    import struct

    if kind == "elf":
        machine = {"x86-64": 62, "aarch64": 183}[arch]
        soname = _MUSL_SONAME[arch] if libc == "musl" else "libc.so.6"
        return _elf_shared(machine, soname)
    if kind == "macho":
        if arch == "universal":
            return _native_fat()
        cputype = {"x86-64": 0x01000007, "aarch64": 0x0100000C}[arch]
        return (b"\xcf\xfa\xed\xfe" + struct.pack("<I", cputype)).ljust(2048, b"\0")
    if kind == "pe":
        machine = {"x86-64": 0x8664, "aarch64": 0xAA64}[arch]
        head = b"MZ" + b"\0" * 58 + struct.pack("<I", 0x40)
        return (head + b"PE\0\0" + struct.pack("<H", machine)).ljust(2048, b"\0")
    raise AssertionError(kind)


def _native_for(column: int, slot: str) -> bytes:
    kind, arch, libc = gate._want(column, slot)
    return _native(kind, arch, libc)


def _write_zip(path: pathlib.Path, entries: dict[str, bytes]) -> pathlib.Path:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_STORED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return path


def _write_sdist(root: pathlib.Path, name: str = "pdfluent-1.0.1.tar.gz") -> pathlib.Path:
    """A minimal sdist beside the wheels: the gate judges the name, which
    is what the release uploads, not the vendored tree inside."""
    path = root / name
    stem = name.removesuffix(".tar.gz")
    payload = f"{stem}\n".encode()
    with tarfile.open(path, "w:gz") as tf:
        info = tarfile.TarInfo(f"{stem}/README.md")
        info.size = len(payload)
        tf.addfile(info, io.BytesIO(payload))
    return path


def _jar_entries(drop: str | None = None) -> dict[str, bytes]:
    entries = {
        e: _native_for(gate._J, e.split("/")[1]) for e in gate.java_expected()
    }
    entries["META-INF/MANIFEST.MF"] = b"Manifest-Version: 1.0\n"
    entries["META-INF/LICENSE"] = _AGPL_TEXT
    entries["META-INF/LICENSE-COMMERCIAL"] = _COMMERCIAL_TEXT
    entries["com/pdfluent/Document.class"] = b"\xca\xfe\xba\xbe"
    if drop is not None:
        entries = {k: v for k, v in entries.items() if f"/{drop}/" not in f"/{k}/"}
    return entries


WHEEL_NAMES = [
    "pdfluent-1.0.1-cp38-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    "pdfluent-1.0.1-cp38-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
    "pdfluent-1.0.1-cp38-abi3-musllinux_1_2_x86_64.whl",
    "pdfluent-1.0.1-cp38-abi3-musllinux_1_2_aarch64.whl",
    "pdfluent-1.0.1-cp38-abi3-macosx_10_12_universal2.whl",
    "pdfluent-1.0.1-cp38-abi3-win_amd64.whl",
]

_WHEEL_NATIVE = {
    "manylinux_2_17_x86_64": ("elf", "x86-64", "gnu"),
    "manylinux_2_17_aarch64": ("elf", "aarch64", "gnu"),
    "musllinux_1_2_x86_64": ("elf", "x86-64", "musl"),
    "musllinux_1_2_aarch64": ("elf", "aarch64", "musl"),
    "macosx_10_12_universal2": ("macho", "universal", None),
    "win_amd64": ("pe", "x86-64", None),
}


def _wheel_entries(name: str) -> dict[str, bytes]:
    tag = name.removesuffix(".whl").split("-")[-1]
    kind, arch, libc = _WHEEL_NATIVE[tag.split(".")[0]]
    ext = "pyd" if kind == "pe" else "so"
    return {
        "pdfluent/__init__.py": b"# pdfluent\n",
        f"pdfluent/_native.abi3.{ext}": _native(kind, arch, libc),
        "pdfluent-1.0.1.dist-info/METADATA": (
            b"Metadata-Version: 2.4\nName: pdfluent\nVersion: 1.0.1\n"
            b"License-Expression: AGPL-3.0-only OR LicenseRef-PDFluent-Commercial\n"
        ),
        "pdfluent-1.0.1.dist-info/licenses/LICENSE": _AGPL_TEXT,
        "pdfluent-1.0.1.dist-info/licenses/LICENSE-COMMERCIAL": _COMMERCIAL_TEXT,
    }


def _nupkg_entries(drop: str | None = None) -> dict[str, bytes]:
    entries = {
        e: _native_for(gate._R, e.split("/")[1]) for e in gate.nuget_expected()
    }
    entries["PDFluent.nuspec"] = b"<package/>"
    entries["LICENSE"] = _AGPL_TEXT
    entries["LICENSE-COMMERCIAL"] = _COMMERCIAL_TEXT
    entries["lib/net8.0/PDFluent.dll"] = b"MZ"
    if drop is not None:
        entries = {k: v for k, v in entries.items() if f"/{drop}/" not in f"/{k}/"}
    return entries


class GateTests(unittest.TestCase):
    def test_maven_complete_jar_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", _jar_entries())
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])

    def test_maven_jar_missing_one_platform_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            jar = _write_zip(
                pathlib.Path(tmp) / "pdfluent.jar",
                _jar_entries(drop="linux-arm64-musl"),
            )
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(
                any("linux-arm64-musl" in f for f in failures), failures
            )

    def test_maven_empty_jar_fails_on_floor(self):
        # The 1.0.0 jar: classes only, 45,174 bytes, no natives at all.
        with tempfile.TemporaryDirectory() as tmp:
            jar = _write_zip(
                pathlib.Path(tmp) / "pdfluent.jar",
                {"com/pdfluent/Document.class": b"\xca\xfe\xba\xbe"},
            )
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(any("linux-x64" in f for f in failures), failures)

    def test_python_complete_wheel_set_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                _write_zip(root / name, _wheel_entries(name))
            _write_sdist(root)
            self.assertEqual(
                gate.check_python_wheels(root, MIN_NATIVE, MARGIN), []
            )

    def test_python_wheel_set_missing_musl_arm64_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                if "musllinux_1_2_aarch64" in name:
                    continue
                _write_zip(root / name, _wheel_entries(name))
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(
                any("linux-arm64-musl" in f for f in failures), failures
            )

    def test_nuget_complete_nupkg_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            nupkg = _write_zip(
                pathlib.Path(tmp) / "PDFluent.1.0.1.nupkg", _nupkg_entries()
            )
            self.assertEqual(gate.check_nuget(nupkg, MIN_NATIVE, MARGIN), [])

    def test_nuget_missing_one_rid_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            nupkg = _write_zip(
                pathlib.Path(tmp) / "PDFluent.1.0.1.nupkg",
                _nupkg_entries(drop="linux-musl-arm64"),
            )
            failures = gate.check_nuget(nupkg, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(
                any("linux-musl-arm64" in f for f in failures), failures
            )

    def test_npm_complete_tree_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "package.json").write_text(
                json.dumps(
                    {
                        "name": "@pdfluent/node",
                        "optionalDependencies": {
                            n: "1.0.1" for n in gate.npm_package_names()
                        },
                    }
                )
            )
            for tag in gate.NPM_TAGS:
                d = root / "npm" / tag
                d.mkdir(parents=True)
                (d / f"index.{tag}.node").write_bytes(_native_for(gate._N, tag))
            self.assertEqual(gate.check_node_dir(root, MIN_NATIVE, MARGIN), [])

    def test_npm_missing_arm64_musl_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            optional = {n: "1.0.1" for n in gate.npm_package_names()}
            del optional["@pdfluent/node-linux-arm64-musl"]
            (root / "package.json").write_text(
                json.dumps({"name": "@pdfluent/node", "optionalDependencies": optional})
            )
            for tag in gate.NPM_TAGS:
                if tag == "linux-arm64-musl":
                    continue
                d = root / "npm" / tag
                d.mkdir(parents=True)
                (d / f"index.{tag}.node").write_bytes(_native_for(gate._N, tag))
            failures = gate.check_node_dir(root, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(
                any("linux-arm64-musl" in f for f in failures), failures
            )

    def test_capi_complete_asset_dir_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for asset in gate.CAPI_ASSETS:
                (root / asset).write_bytes(_native_for(gate._C, asset))
            (root / gate.CAPI_HEADER).write_text("/* pdfluent.h */\n")
            self.assertEqual(gate.check_capi_dir(root, MIN_NATIVE, MARGIN), [])

    def test_capi_missing_one_asset_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for asset in gate.CAPI_ASSETS:
                if "arm64-musl" in asset:
                    continue
                (root / asset).write_bytes(_native_for(gate._C, asset))
            (root / gate.CAPI_HEADER).write_text("/* pdfluent.h */\n")
            failures = gate.check_capi_dir(root, MIN_NATIVE, MARGIN)
            self.assertTrue(failures)
            self.assertTrue(any("arm64-musl" in f for f in failures), failures)

    def test_small_natives_fail_individually_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for asset in gate.CAPI_ASSETS:
                (root / asset).write_bytes(b"\0" * 10)
            (root / gate.CAPI_HEADER).write_text("/* pdfluent.h */\n")
            failures = gate.check_capi_dir(root, MIN_NATIVE, MARGIN)
            small = [f for f in failures if "below the per-native floor" in f]
            self.assertEqual(len(small), len(gate.CAPI_ASSETS), failures)
            for asset in gate.CAPI_ASSETS:
                self.assertTrue(
                    any(asset in f for f in small), (asset, failures)
                )

    def test_one_zero_byte_native_fails_while_six_pass(self):
        # The sum floor passed this: six good natives carried one empty one.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            victim = "native/osx-arm64/libpdfluent_java.dylib"
            entries[victim] = b""
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("osx-arm64" in f and "0 bytes" in f for f in failures),
                failures,
            )
            self.assertFalse(
                [f for f in failures if "linux-x64" in f and "0 bytes" in f],
                failures,
            )

    def test_x86_dylib_in_the_arm64_slot_fails_with_both_names(self):
        # The host fallback staged exactly this: a host-built dylib under a
        # foreign platform. The gate counted files and passed it.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/osx-arm64/libpdfluent_java.dylib"] = _native(
                "macho", "x86-64", None
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "osx-arm64" in f and "x86-64" in f and "aarch64" in f
                    for f in failures
                ),
                failures,
            )

    def test_gnu_so_in_a_musl_slot_fails_with_the_libc(self):
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/linux-arm64-musl/libpdf_capi.so"] = _native(
                "elf", "aarch64", "gnu"
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "linux-arm64-musl" in f
                    and "gnu" in f
                    and "musl" in f
                    for f in failures
                ),
                failures,
            )

    def test_pt_interp_decides_the_libc(self):
        import struct

        def executable(interp: str) -> bytes:
            blob = bytearray(2048)
            blob[0:4] = b"\x7fELF"
            blob[4] = 2
            struct.pack_into("<HH", blob, 16, 2, 62)
            struct.pack_into("<Q", blob, 32, 64)
            struct.pack_into("<H", blob, 54, 56)
            struct.pack_into("<H", blob, 56, 2)
            struct.pack_into("<I", blob, 64, 1)
            struct.pack_into("<I", blob, 120, 3)
            struct.pack_into("<Q", blob, 128, 256)
            raw = interp.encode() + b"\0"
            blob[256 : 256 + len(raw)] = raw
            return bytes(blob)

        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/linux-x64-musl/libpdf_capi.so"] = executable(
                "/lib/ld-musl-x86_64.so.1"
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])
            entries["native/linux-x64-musl/libpdf_capi.so"] = executable(
                "/lib64/ld-linux-x86-64.so.2"
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "linux-x64-musl" in f and "gnu" in f
                    for f in failures
                ),
                failures,
            )

    def test_elf_without_a_libc_marker_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/linux-x64/libpdf_capi.so"] = (
                b"\x7fELF" + bytes((2, 1, 1, 0)) + b"\0" * 8
                + b"\x03\x00\x3e\x00"
            ).ljust(2048, b"\0")
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("linux-x64" in f and "libc marker" in f for f in failures),
                failures,
            )

    def test_garbage_bytes_in_a_native_slot_fail_on_magic(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for asset in gate.CAPI_ASSETS:
                (root / asset).write_bytes(_native_for(gate._C, asset))
            (root / gate.CAPI_HEADER).write_text("/* pdfluent.h */\n")
            victim = "libpdf_capi.linux-x64.so"
            (root / victim).write_bytes(b"not a native at all".ljust(2048, b"\0"))
            failures = gate.check_capi_dir(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(victim in f and "bad magic" in f for f in failures),
                failures,
            )

    def test_jar_without_licence_texts_fails_on_the_audit(self):
        with tempfile.TemporaryDirectory() as tmp:
            entries = {
                k: v
                for k, v in _jar_entries().items()
                if "LICENSE" not in k
            }
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(any("LICENSE" in f for f in failures), failures)

    def test_jar_with_only_the_commercial_half_fails_on_the_agpl_half(self):
        with tempfile.TemporaryDirectory() as tmp:
            entries = {
                k: v
                for k, v in _jar_entries().items()
                if k != "META-INF/LICENSE"
            }
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("AGPL" in f for f in failures), failures
            )

    def test_single_x86_mac_wheel_does_not_cover_macos(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            names = [
                n for n in WHEEL_NAMES if "macosx" not in n
            ] + ["pdfluent-1.0.1-cp38-abi3-macosx_10_12_x86_64.whl"]
            for name in names:
                tag = name.removesuffix(".whl").split("-")[-1]
                kind, arch, libc = _WHEEL_NATIVE.get(
                    tag.split(".")[0], ("macho", "x86-64", None)
                )
                entries = _wheel_entries(WHEEL_NAMES[0])
                entries["pdfluent/_native.abi3.so"] = _native(kind, arch, libc)
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("macos" in f and "universal2" in f for f in failures),
                failures,
            )

    def test_per_arch_mac_wheels_cover_macos_without_universal2(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            names = [n for n in WHEEL_NAMES if "macosx" not in n]
            for name in names:
                _write_zip(root / name, _wheel_entries(name))
            for arch_wheel, arch in (
                ("pdfluent-1.0.1-cp38-abi3-macosx_10_12_x86_64.whl", "x86-64"),
                ("pdfluent-1.0.1-cp38-abi3-macosx_11_0_arm64.whl", "aarch64"),
            ):
                entries = _wheel_entries(WHEEL_NAMES[0])
                entries["pdfluent/_native.abi3.so"] = _native("macho", arch, None)
                _write_zip(root / arch_wheel, entries)
            _write_sdist(root)
            self.assertEqual(
                gate.check_python_wheels(root, MIN_NATIVE, MARGIN), []
            )

    def test_universal2_wheel_with_a_thin_extension_fails(self):
        # The gap: a thin x86_64 .so under a universal2 tag passed, and
        # installs on arm64 only to die at import. universal2 means FAT.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "universal2" in name:
                    entries["pdfluent/_native.abi3.so"] = _native(
                        "macho", "x86-64", None
                    )
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "universal2" in f and "universal" in f
                    for f in failures
                ),
                failures,
            )

    def test_fat_header_missing_an_arch_fails_the_wheel(self):
        # A fat file listing only x86_64 is not universal2 either: the old
        # magic-only read called every fat file universal and passed it.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "universal2" in name:
                    entries["pdfluent/_native.abi3.so"] = _native_fat(("x86-64",))
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("universal2" in f and "fat-" in f for f in failures),
                failures,
            )

    def test_verified_universal_dylib_passes_either_mac_slot(self):
        # A fat dylib listing both archs holds both slots' bytes.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/osx-arm64/libpdfluent_java.dylib"] = _native_fat()
            entries["native/osx-x64/libpdfluent_java.dylib"] = _native_fat()
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])

    def test_fat_dylib_missing_an_arch_fails_the_arm64_slot(self):
        # The old slot check let any fat file pass any mac slot without
        # reading which archs its header lists.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/osx-arm64/libpdfluent_java.dylib"] = _native_fat(
                ("x86-64",)
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("osx-arm64" in f and "fat-" in f for f in failures),
                failures,
            )

    def test_fat_header_archs_are_read_off_the_bytes(self):
        self.assertEqual(
            gate.native_identity(_native_fat(("x86-64", "aarch64"))),
            ("macho", "universal", None),
        )
        kind, arch, _ = gate.native_identity(_native_fat(("x86-64",)))
        self.assertEqual(kind, "macho")
        self.assertIn("x86-64", arch)
        self.assertNotEqual(arch, "universal")
        self.assertEqual(
            gate.native_identity(b"\xca\xfe\xba\xbe\x00"),
            ("macho", "fat-unreadable", None),
        )
        self.assertEqual(
            gate.native_identity(b"\xca\xfe\xba\xbe" + b"\0" * 4),
            ("macho", "fat-empty", None),
        )

    def test_dt_needed_overrules_stray_version_bytes(self):
        # A musl-linked object mentioning GLIBC_ in a comment used to read
        # as gnu: the verdict grepped the whole file. Now DT_NEEDED alone
        # decides, and stray bytes elsewhere move nothing.
        with tempfile.TemporaryDirectory() as tmp:
            blob = bytearray(_native("elf", "x86-64", "musl"))
            blob[1500 : 1500 + 24] = b"needs GLIBC_2.17 or later\x00"
            entries = _jar_entries()
            entries["native/linux-x64-musl/libpdf_capi.so"] = bytes(blob)
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])

    def test_stray_musl_bytes_do_not_move_a_gnu_verdict(self):
        with tempfile.TemporaryDirectory() as tmp:
            blob = bytearray(_native("elf", "x86-64", "gnu"))
            blob[1500 : 1500 + 20] = b"see ld-musl someday\x00\x00\x00"
            entries = _jar_entries()
            entries["native/linux-x64/libpdf_capi.so"] = bytes(blob)
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])

    def test_dt_needed_lists_the_libc_sonames(self):
        self.assertEqual(
            gate._elf_dt_needed(_native("elf", "x86-64", "gnu")),
            ["libc.so.6"],
        )
        self.assertEqual(
            gate._elf_dt_needed(_native("elf", "aarch64", "musl")),
            ["libc.musl-aarch64.so.1"],
        )
        # No dynamic section, no names: a bare header and garbage read None.
        self.assertIsNone(gate._elf_dt_needed(b"\x7fELF" + b"\0" * 100))
        self.assertIsNone(gate._elf_dt_needed(b"not an elf file"))

    def test_unexpected_dt_needed_fails_with_the_soname(self):
        # The libc verdict reads DT_NEEDED but judged only the libc entry:
        # a native linking anything else passed without a word. Now every
        # DT_NEEDED entry must be on the allowlist, and this one is not.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/linux-x64/libpdf_capi.so"] = _elf_shared(
                62, ("libc.so.6", "libevil.so.1")
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            failures = gate.check_maven_jar(jar, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "libevil.so.1" in f and "linux-x64" in f
                    for f in failures
                ),
                failures,
            )

    def test_allowed_companion_libraries_pass(self):
        # The companion set a shipped native may link beside its libc.
        with tempfile.TemporaryDirectory() as tmp:
            entries = _jar_entries()
            entries["native/linux-x64/libpdf_capi.so"] = _elf_shared(
                62,
                (
                    "libc.so.6",
                    "libm.so.6",
                    "libpthread.so.0",
                    "libdl.so.2",
                    "libgcc_s.so.1",
                ),
            )
            jar = _write_zip(pathlib.Path(tmp) / "pdfluent.jar", entries)
            self.assertEqual(gate.check_maven_jar(jar, MIN_NATIVE, MARGIN), [])

    def test_wheel_with_unexpected_dt_needed_fails(self):
        # The wheel check reads DT_NEEDED through its own loop, not through
        # _identity_failures: the allowlist must hold there too.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "manylinux_2_17_x86_64" in name:
                    entries["pdfluent/_native.abi3.so"] = _elf_shared(
                        62, ("libc.so.6", "libevil.so.1")
                    )
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("libevil.so.1" in f for f in failures), failures
            )

    def test_allow_missing_reports_unbuilt_platforms_by_name(self):
        # The lane builds some platforms, not all: what is built is judged,
        # the rest is named, and nothing fails for being absent.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_zip(root / WHEEL_NAMES[0], _wheel_entries(WHEEL_NAMES[0]))
            _write_sdist(root)
            infos: list[str] = []
            failures = gate.check_python_wheels(
                root, MIN_NATIVE, MARGIN, allow_missing=True, not_built=infos
            )
            self.assertEqual(failures, [])
            self.assertIn("python: not built: linux-arm64-musl", infos)
            self.assertIn("python: not built: macos", infos)
            self.assertNotIn("python: not built: linux-x64", infos)

    def test_allow_missing_still_judges_what_is_built(self):
        # Lane mode forgives absence, never a wrong native: the thin
        # universal2 wheel fails here exactly as it does strictly.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "universal2" in name:
                    entries["pdfluent/_native.abi3.so"] = _native(
                        "macho", "x86-64", None
                    )
                _write_zip(root / name, entries)
            infos: list[str] = []
            failures = gate.check_python_wheels(
                root, MIN_NATIVE, MARGIN, allow_missing=True, not_built=infos
            )
            self.assertTrue(
                any("universal2" in f and "universal" in f for f in failures),
                failures,
            )
            self.assertEqual(infos, [])

    def test_allow_missing_with_nothing_built_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            root.mkdir(exist_ok=True)
            infos: list[str] = []
            failures = gate.check_python_wheels(
                root, MIN_NATIVE, MARGIN, allow_missing=True, not_built=infos
            )
            self.assertTrue(
                any("no .whl files" in f for f in failures), failures
            )
            entries = {
                "LICENSE": _AGPL_TEXT,
                "LICENSE-COMMERCIAL": _COMMERCIAL_TEXT,
            }
            jar = _write_zip(pathlib.Path(tmp) / "empty.jar", entries)
            infos = []
            failures = gate.check_maven_jar(
                jar, MIN_NATIVE, MARGIN, allow_missing=True, not_built=infos
            )
            self.assertTrue(
                any("nothing built" in f for f in failures), failures
            )

    def test_allow_missing_verdict_names_the_count(self):
        import contextlib
        import io

        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_zip(root / WHEEL_NAMES[0], _wheel_entries(WHEEL_NAMES[0]))
            _write_sdist(root)
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gate.main(
                    [
                        "--wheels", str(root), "--allow-missing",
                        "--min-native-bytes", str(MIN_NATIVE),
                        "--margin", str(MARGIN),
                    ]
                )
            self.assertEqual(rc, 0, buf.getvalue())
            self.assertIn("PASS (partial:", buf.getvalue())
            self.assertIn("not built: linux-arm64-musl", buf.getvalue())

    def test_non_abi3_wheel_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                _write_zip(root / name, _wheel_entries(name))
            intruder = "pdfluent-1.0.1-cp311-cp311-manylinux_2_17_x86_64.whl"
            _write_zip(root / intruder, _wheel_entries(WHEEL_NAMES[0]))
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("cp311" in f and "abi3" in f for f in failures), failures
            )

    def test_musl_wheel_holding_a_gnu_extension_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "musllinux_1_2_x86_64" in name:
                    entries["pdfluent/_native.abi3.so"] = _native(
                        "elf", "x86-64", "gnu"
                    )
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any(
                    "musllinux_1_2_x86_64" in f
                    and "gnu" in f
                    and "musl" in f
                    for f in failures
                ),
                failures,
            )

    def test_wheel_without_a_native_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                entries = _wheel_entries(name)
                if "win_amd64" in name:
                    entries = {
                        k: v
                        for k, v in entries.items()
                        if not k.endswith((".so", ".pyd"))
                    }
                _write_zip(root / name, entries)
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("win_amd64" in f and "no native" in f for f in failures),
                failures,
            )


CLI_VERSION = "1.0.0"

CLI_FILE_LINES = {
    "macos-arm64": "Mach-O 64-bit executable arm64",
    "macos-x64": "Mach-O 64-bit executable x86_64",
    "linux-x64": "ELF 64-bit LSB pie executable, x86-64",
    "linux-arm64": "ELF 64-bit LSB pie executable, ARM aarch64",
    "linux-x64-musl": "ELF 64-bit LSB executable, x86-64, statically linked",
    "linux-arm64-musl": "ELF 64-bit LSB executable, ARM aarch64, statically linked",
    "win-x64": "PE32+ executable (console) x86-64, for MS Windows",
}

_CLI_MUSL_ARCH = {"linux-x64-musl": "x86-64", "linux-arm64-musl": "aarch64"}


def _write_cli_archive(
    directory: pathlib.Path,
    plat: str,
    triple: str,
    ext: str,
    drop_member: str | None = None,
    binary: bytes | None = None,
) -> pathlib.Path:
    if binary is None and plat in _CLI_MUSL_ARCH:
        # The static check reads the archived binary's own headers, so the
        # default musl binary is a static executable of the slot's arch.
        binary = _elf_executable(arch=_CLI_MUSL_ARCH[plat])
    members = {gate.cli_binary_name(plat): binary if binary is not None else b"\0" * 2048}
    for name in gate.CLI_ARCHIVE_MEMBERS:
        if name == "LICENSE":
            members[name] = _AGPL_TEXT
        elif name == "LICENSE-COMMERCIAL":
            members[name] = _COMMERCIAL_TEXT
        else:
            members[name] = f"{name} for {plat}\n".encode()
    if drop_member is not None:
        members.pop(drop_member, None)
    archive = directory / gate.cli_archive_name(CLI_VERSION, triple, ext)
    if ext == "zip":
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_STORED) as zf:
            for name, data in members.items():
                zf.writestr(name, data)
    else:
        with tarfile.open(archive, "w:gz") as tf:
            for name, data in members.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                tf.addfile(info, io.BytesIO(data))
    return archive


def _write_cli_dir(
    root: pathlib.Path,
    drop_platform: str | None = None,
    drop_member: tuple[str, str] | None = None,
    binaries: dict[str, bytes] | None = None,
) -> pathlib.Path:
    for plat, triple, ext in gate.CLI_PLATFORMS:
        if plat == drop_platform:
            continue
        drop = drop_member[1] if drop_member and drop_member[0] == plat else None
        blob = binaries.get(plat) if binaries else None
        _write_cli_archive(root, plat, triple, ext, drop_member=drop, binary=blob)
    sums = []
    for plat, triple, ext in gate.CLI_PLATFORMS:
        if plat == drop_platform:
            continue
        name = gate.cli_archive_name(CLI_VERSION, triple, ext)
        digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
        sums.append(f"{digest}  {name}\n")
    (root / "SHA256SUMS").write_text("".join(sorted(sums)))
    return root


def _formula_text(entries: dict[str, str], version: str = CLI_VERSION) -> str:
    lines = ['class Pdfluent < Formula', '  test do: system bin/"pdfluent", "--version"']
    for name, digest in sorted(entries.items()):
        lines.append(f"  # {name}")
        lines.append(f"  sha256 \"{digest}\"")
    return "\n".join(lines) + "\n"


def _sums_text(entries: dict[str, str]) -> str:
    return "".join(f"{d}  {n}\n" for n, d in sorted(entries.items()))


def _brew_entries(
    drop: str | None = None,
) -> tuple[dict[str, str], dict[str, str]]:
    """(sums entries, formula entries) for the four Homebrew archives."""
    triple_of = {plat: triple for plat, triple, _ in gate.CLI_PLATFORMS}
    sums = {}
    for plat in gate.HOMEBREW_PLATFORMS:
        if plat == drop:
            continue
        name = gate.cli_archive_name(CLI_VERSION, triple_of[plat], "tar.gz")
        sums[name] = hashlib.sha256(name.encode()).hexdigest()
    return sums, dict(sums)


def _manifest_text(platforms: list[tuple[str, str]]) -> str:
    return json.dumps(
        {
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "platform": {"architecture": arch, "os": os},
                }
                for os, arch in platforms
            ],
        }
    )


class CliChannelTests(unittest.TestCase):
    def test_platform_table_is_the_seven_with_identity(self):
        # The one table, pinned: a platform removed here must turn this red,
        # not silently shrink the gate. Row order is the canonical order every
        # channel derives.
        self.assertEqual(
            [r[0] for r in gate.PLATFORMS],
            [
                "linux-x64",
                "linux-arm64",
                "linux-x64-musl",
                "linux-arm64-musl",
                "osx-x64",
                "osx-arm64",
                "win-x64",
            ],
        )
        self.assertEqual(
            [(r[7], r[8], r[9]) for r in gate.PLATFORMS],
            [
                ("elf", "x86-64", "gnu"),
                ("elf", "aarch64", "gnu"),
                ("elf", "x86-64", "musl"),
                ("elf", "aarch64", "musl"),
                ("macho", "x86-64", None),
                ("macho", "aarch64", None),
                ("pe", "x86-64", None),
            ],
        )

    def test_every_channel_table_derives_from_the_one(self):
        self.assertEqual(
            tuple(gate.JAVA_PLATFORMS), tuple(r[1] for r in gate.PLATFORMS)
        )
        self.assertEqual(tuple(gate.NUGET_RIDS), tuple(r[2] for r in gate.PLATFORMS))
        self.assertEqual(tuple(gate.NPM_TAGS), tuple(r[3] for r in gate.PLATFORMS))
        self.assertEqual(tuple(gate.CAPI_ASSETS), tuple(r[5] for r in gate.PLATFORMS))
        self.assertEqual(
            tuple(gate.CLI_PLATFORMS), tuple(r[6] for r in gate.PLATFORMS)
        )
        # The two mac rows collapse to one python entry; nothing else does.
        self.assertEqual(len(gate.PYTHON_PLATFORMS), 6)
        self.assertEqual(len(gate.PYTHON_PLATFORM_NAMES), 6)

    def test_cli_platform_table_is_the_seven(self):
        # The addendum table, pinned: a platform removed here must turn this
        # red, not silently shrink the gate.
        self.assertEqual(
            list(gate.CLI_PLATFORMS),
            [
                ("linux-x64", "x86_64-unknown-linux-gnu", "tar.gz"),
                ("linux-arm64", "aarch64-unknown-linux-gnu", "tar.gz"),
                ("linux-x64-musl", "x86_64-unknown-linux-musl", "tar.gz"),
                ("linux-arm64-musl", "aarch64-unknown-linux-musl", "tar.gz"),
                ("macos-x64", "x86_64-apple-darwin", "tar.gz"),
                ("macos-arm64", "aarch64-apple-darwin", "tar.gz"),
                ("win-x64", "x86_64-pc-windows-msvc", "zip"),
            ],
        )

    def test_homebrew_platforms_are_the_four_glibc_and_macos(self):
        self.assertEqual(
            list(gate.HOMEBREW_PLATFORMS),
            ["macos-arm64", "macos-x64", "linux-x64", "linux-arm64"],
        )

    def test_cli_complete_dir_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            self.assertEqual(
                gate.check_cli_dir(
                    root,
                    CLI_VERSION,
                    10,
                    file_lines=dict(CLI_FILE_LINES),
                ),
                [],
            )

    def test_cli_missing_one_archive_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp), drop_platform="linux-arm64-musl")
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(any("linux-arm64-musl" in f for f in failures), failures)

    def test_cli_archive_missing_member_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(
                pathlib.Path(tmp), drop_member=("linux-x64", "NOTICE")
            )
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(
                any("linux-x64" in f and "NOTICE" in f for f in failures), failures
            )

    def test_cli_archive_without_the_man_page_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(
                pathlib.Path(tmp),
                drop_member=("linux-x64", "man/man1/pdfluent.1"),
            )
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(
                any(
                    "linux-x64" in f and "man/man1/pdfluent.1" in f
                    for f in failures
                ),
                failures,
            )

    def test_cli_zip_with_backslash_entries_fails_naming_them(self):
        # Build 2: Compress-Archive wrote man\man1\pdfluent.1 into the
        # win-x64 zip, and the gate only said it "misses man/man1/…".
        # The backslash entries are named outright now, so the cause can
        # never be silent again.
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            win = root / gate.cli_archive_name(
                CLI_VERSION, "x86_64-pc-windows-msvc", "zip"
            )
            with zipfile.ZipFile(win) as zf:
                entries = {
                    i.filename: zf.read(i.filename)
                    for i in zf.infolist()
                    if not i.is_dir()
                }
            win.unlink()
            with zipfile.ZipFile(win, "w", zipfile.ZIP_STORED) as zf:
                for name, data in entries.items():
                    zf.writestr(name.replace("/", "\\"), data)
            sums = []
            for plat, triple, ext in gate.CLI_PLATFORMS:
                name = gate.cli_archive_name(CLI_VERSION, triple, ext)
                digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
                sums.append(f"{digest}  {name}\n")
            (root / "SHA256SUMS").write_text("".join(sorted(sums)))
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(
                any(
                    "win-x64" in f and "entry uses backslashes" in f
                    for f in failures
                ),
                failures,
            )

    def test_cli_digest_mismatch_fails_with_platform_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            victim = root / gate.cli_archive_name(
                CLI_VERSION, "x86_64-apple-darwin", "tar.gz"
            )
            victim.write_bytes(victim.read_bytes() + b"tampered")
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(any("macos-x64" in f for f in failures), failures)

    def test_cli_wrong_architecture_fails_with_platform_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            lines = dict(CLI_FILE_LINES)
            lines["linux-arm64"] = "ELF 64-bit LSB pie executable, x86-64"
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=lines,
            )
            self.assertTrue(failures)
            self.assertTrue(any("linux-arm64" in f for f in failures), failures)

    def test_static_executable_reads_static_on_both_arches(self):
        for arch in ("x86-64", "aarch64"):
            with self.subTest(arch=arch):
                self.assertTrue(gate.elf_is_static(_elf_executable(arch=arch)))

    def test_static_pie_reads_static_on_both_arches(self):
        # Rust musl binaries are static-pie: a PT_DYNAMIC with no DT_NEEDED
        # and no interpreter. Modern ldd reports "statically linked" for
        # them, which the old string match read as dynamic.
        for arch in ("x86-64", "aarch64"):
            with self.subTest(arch=arch):
                self.assertTrue(
                    gate.elf_is_static(
                        _elf_executable(arch=arch, etype=3, needed=())
                    )
                )

    def test_dynamic_glibc_executable_reads_dynamic_on_both_arches(self):
        for arch in ("x86-64", "aarch64"):
            with self.subTest(arch=arch):
                self.assertFalse(
                    gate.elf_is_static(
                        _elf_executable(
                            arch=arch,
                            interp=_GNU_INTERP[arch],
                            needed=(_GNU_SONAME,),
                        )
                    )
                )

    def test_dynamic_musl_executable_reads_dynamic_on_both_arches(self):
        for arch in ("x86-64", "aarch64"):
            with self.subTest(arch=arch):
                self.assertFalse(
                    gate.elf_is_static(
                        _elf_executable(
                            arch=arch,
                            interp=_MUSL_INTERP[arch],
                            needed=(_MUSL_SONAME[arch],),
                        )
                    )
                )

    def test_non_elf_bytes_read_dynamic(self):
        # Not an ELF file at all: nothing to vouch for, so never static.
        self.assertFalse(gate.elf_is_static(b"\0" * 2048))
        self.assertFalse(gate.elf_is_static(b"not an elf file"))

    def test_cli_dynamic_musl_fails_with_platform_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(
                pathlib.Path(tmp),
                binaries={
                    "linux-x64-musl": _elf_executable(
                        arch="x86-64",
                        interp=_MUSL_INTERP["x86-64"],
                        needed=(_MUSL_SONAME["x86-64"],),
                    ),
                },
            )
            failures = gate.check_cli_dir(
                root, CLI_VERSION, 10, file_lines=dict(CLI_FILE_LINES)
            )
            self.assertTrue(failures)
            self.assertTrue(
                any(
                    "linux-x64-musl" in f and "statically linked" in f
                    for f in failures
                ),
                failures,
            )

    def test_cli_dynamic_arm64_musl_fails_on_any_host(self):
        # ldd on the x86 release-gate runner calls every arm64 binary "not a
        # dynamic executable", so the old check passed a dynamic musl binary
        # there. The gate reads the binary's own headers now: a dynamic
        # arm64 binary fails naming the platform, on any host, with no ldd.
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(
                pathlib.Path(tmp),
                binaries={
                    "linux-arm64-musl": _elf_executable(
                        arch="aarch64",
                        interp=_MUSL_INTERP["aarch64"],
                        needed=(_MUSL_SONAME["aarch64"],),
                    ),
                },
            )
            failures = gate.check_cli_dir(
                root, CLI_VERSION, 10, file_lines=dict(CLI_FILE_LINES)
            )
            self.assertTrue(
                any(
                    "linux-arm64-musl" in f and "not statically linked" in f
                    for f in failures
                ),
                failures,
            )

    def test_cli_static_pie_musl_binaries_pass(self):
        # Rust musl binaries are static-pie, and modern ldd says "statically
        # linked" for them, which the old match read as dynamic. Both musl
        # slots hold a static-pie here and the gate passes them.
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(
                pathlib.Path(tmp),
                binaries={
                    plat: _elf_executable(arch=arch, etype=3, needed=())
                    for plat, arch in _CLI_MUSL_ARCH.items()
                },
            )
            self.assertEqual(
                gate.check_cli_dir(
                    root, CLI_VERSION, 10, file_lines=dict(CLI_FILE_LINES)
                ),
                [],
            )

    def test_cli_archive_below_floor_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10**9,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(any("below floor" in f for f in failures), failures)

    def test_cli_missing_sums_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = _write_cli_dir(pathlib.Path(tmp))
            (root / "SHA256SUMS").unlink()
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(any("SHA256SUMS" in f for f in failures), failures)

    def test_cli_archive_with_placeholder_licence_fails_on_text(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for plat, triple, ext in gate.CLI_PLATFORMS:
                archive = _write_cli_archive(root, plat, triple, ext)
                if plat == "linux-x64":
                    # Rewrite the one archive with a placeholder LICENSE.
                    members = {gate.cli_binary_name(plat): b"\0" * 2048}
                    for name in gate.CLI_ARCHIVE_MEMBERS:
                        members[name] = f"{name} for {plat}\n".encode()
                    archive.unlink()
                    with tarfile.open(archive, "w:gz") as tf:
                        for name, data in members.items():
                            info = tarfile.TarInfo(name)
                            info.size = len(data)
                            tf.addfile(info, io.BytesIO(data))
            sums = []
            for plat, triple, ext in gate.CLI_PLATFORMS:
                name = gate.cli_archive_name(CLI_VERSION, triple, ext)
                digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
                sums.append(f"{digest}  {name}\n")
            (root / "SHA256SUMS").write_text("".join(sorted(sums)))
            failures = gate.check_cli_dir(
                root,
                CLI_VERSION,
                10,
                file_lines=dict(CLI_FILE_LINES),
            )
            self.assertTrue(failures)
            self.assertTrue(
                any(
                    "x86_64-unknown-linux-gnu" in f and "AGPL" in f
                    for f in failures
                ),
                failures,
            )

    def test_homebrew_formula_matching_sums_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums, formula_entries = _brew_entries()
            (root / "SHA256SUMS").write_text(_sums_text(sums))
            (root / "pdfluent.rb").write_text(_formula_text(formula_entries))
            self.assertEqual(
                gate.check_homebrew_formula(
                    root / "pdfluent.rb", root / "SHA256SUMS", CLI_VERSION
                ),
                [],
            )

    def test_homebrew_sums_missing_platform_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums, formula_entries = _brew_entries(drop="linux-arm64")
            (root / "SHA256SUMS").write_text(_sums_text(sums))
            (root / "pdfluent.rb").write_text(_formula_text(formula_entries))
            failures = gate.check_homebrew_formula(
                root / "pdfluent.rb", root / "SHA256SUMS", CLI_VERSION
            )
            self.assertTrue(failures)
            self.assertTrue(any("linux-arm64" in f for f in failures), failures)

    def test_homebrew_formula_sha_mismatch_fails_with_platform_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums, formula_entries = _brew_entries()
            victim = next(n for n in formula_entries if "x86_64-apple-darwin" in n)
            formula_entries[victim] = "0" * 64
            (root / "SHA256SUMS").write_text(_sums_text(sums))
            (root / "pdfluent.rb").write_text(_formula_text(formula_entries))
            failures = gate.check_homebrew_formula(
                root / "pdfluent.rb", root / "SHA256SUMS", CLI_VERSION
            )
            self.assertTrue(failures)
            self.assertTrue(any("macos-x64" in f for f in failures), failures)

    def test_homebrew_formula_without_version_test_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            sums, formula_entries = _brew_entries()
            (root / "SHA256SUMS").write_text(_sums_text(sums))
            text = _formula_text(formula_entries).replace('test do: system bin/"pdfluent", "--version"', "test do\n    true\n  end")
            (root / "pdfluent.rb").write_text(text)
            failures = gate.check_homebrew_formula(
                root / "pdfluent.rb", root / "SHA256SUMS", CLI_VERSION
            )
            self.assertTrue(failures)
            self.assertTrue(any("--version" in f for f in failures), failures)

    def test_docker_manifest_exact_pair_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "manifest.json"
            manifest.write_text(_manifest_text([("linux", "amd64"), ("linux", "arm64")]))
            self.assertEqual(gate.check_docker_manifest(manifest), [])

    def test_docker_manifest_missing_arm64_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "manifest.json"
            manifest.write_text(_manifest_text([("linux", "amd64")]))
            failures = gate.check_docker_manifest(manifest)
            self.assertTrue(failures)
            self.assertTrue(any("linux/arm64" in f for f in failures), failures)

    def test_docker_manifest_extra_platform_fails_with_its_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "manifest.json"
            manifest.write_text(
                _manifest_text(
                    [("linux", "amd64"), ("linux", "arm64"), ("windows", "amd64")]
                )
            )
            failures = gate.check_docker_manifest(manifest)
            self.assertTrue(failures)
            self.assertTrue(any("windows/amd64" in f for f in failures), failures)

    def test_docker_manifest_with_attestation_entries_passes(self):
        # buildx appends attestation manifests to the index, with
        # unknown/unknown where a platform would be. They describe the
        # build, not a platform, so the gate skips them instead of failing
        # on them; the provenance they carry stays in the image.
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "manifest.json"
            manifest.write_text(
                _manifest_text(
                    [("linux", "amd64"), ("linux", "arm64"),
                     ("unknown", "unknown")]
                )
            )
            self.assertEqual(gate.check_docker_manifest(manifest), [])

    def test_docker_manifest_of_only_attestations_still_misses_the_pair(self):
        # Skipping attestation entries must not become counting them: an
        # index with no real platform still fails naming what is missing.
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "manifest.json"
            manifest.write_text(_manifest_text([("unknown", "unknown")]))
            failures = gate.check_docker_manifest(manifest)
            self.assertTrue(any("linux/arm64" in f for f in failures), failures)


class RequireAllTests(unittest.TestCase):
    def test_require_all_with_only_node_dir_fails_naming_the_other_five(
        self,
    ):
        # The release mode: an omitted channel flag is a failure naming
        # the channel, not a silently skipped channel.
        import contextlib

        with tempfile.TemporaryDirectory() as tmp:
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gate.main(["--require-all", "--node-dir", tmp])
            out = buf.getvalue()
            self.assertEqual(rc, 1, out)
            for channel in ("jar", "wheels", "nupkg", "capi", "cli"):
                self.assertIn(f"the {channel} channel is missing", out)
            self.assertNotIn("the node channel is missing", out)

    def test_require_all_alone_names_all_six(self):
        import contextlib

        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = gate.main(["--require-all"])
        out = buf.getvalue()
        self.assertEqual(rc, 1, out)
        for channel in ("jar", "wheels", "nupkg", "node", "capi", "cli"):
            self.assertIn(f"the {channel} channel is missing", out)

    def test_require_all_scoped_to_a_passed_channel_says_nothing_else(
        self,
    ):
        # The one-channel lanes (npm, binaries) demand their own channel
        # only: absent others are not their run's business.
        import contextlib

        with tempfile.TemporaryDirectory() as tmp:
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gate.main(["--require-all", "node", "--node-dir", tmp])
            out = buf.getvalue()
            self.assertEqual(rc, 1, out)  # the empty dir itself still fails
            self.assertNotIn("the jar channel is missing", out)
            self.assertNotIn("the wheels channel is missing", out)
            self.assertNotIn("the nupkg channel is missing", out)
            self.assertNotIn("the capi channel is missing", out)
            self.assertNotIn("the cli channel is missing", out)
            self.assertNotIn("the node channel is missing", out)

    def test_require_all_scoped_to_an_absent_channel_fails_by_name(self):
        import contextlib

        with tempfile.TemporaryDirectory() as tmp:
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gate.main(["--require-all", "cli", "--node-dir", tmp])
            out = buf.getvalue()
            self.assertEqual(rc, 1, out)
            self.assertIn("the cli channel is missing", out)
            self.assertNotIn("the jar channel is missing", out)

    def test_require_all_unknown_channel_is_a_usage_error(self):
        import contextlib

        with tempfile.TemporaryDirectory() as tmp:
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                with self.assertRaises(SystemExit) as ctx:
                    gate.main(["--require-all", "wasm", "--node-dir", tmp])
            self.assertEqual(ctx.exception.code, 2)
            self.assertIn("unknown channel 'wasm'", err.getvalue())


class SdistTests(unittest.TestCase):
    def test_wheels_dir_without_an_sdist_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                _write_zip(root / name, _wheel_entries(name))
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(any("no sdist" in f for f in failures), failures)

    def test_wheels_dir_with_two_sdists_fails_as_ambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name in WHEEL_NAMES:
                _write_zip(root / name, _wheel_entries(name))
            _write_sdist(root, "pdfluent-1.0.1.tar.gz")
            _write_sdist(root, "pdfluent-1.0.0.tar.gz")
            failures = gate.check_python_wheels(root, MIN_NATIVE, MARGIN)
            self.assertTrue(
                any("more than one sdist" in f for f in failures), failures
            )

    def test_single_wheel_without_a_beside_sdist_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            wheel = _write_zip(
                root / WHEEL_NAMES[0], _wheel_entries(WHEEL_NAMES[0])
            )
            failures = gate.check_python_wheels(wheel, MIN_NATIVE, MARGIN)
            self.assertTrue(any("no sdist" in f for f in failures), failures)

    def test_single_wheel_with_a_beside_sdist_says_nothing_about_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            wheel = _write_zip(
                root / WHEEL_NAMES[0], _wheel_entries(WHEEL_NAMES[0])
            )
            _write_sdist(root)
            failures = gate.check_python_wheels(wheel, MIN_NATIVE, MARGIN)
            self.assertTrue(failures, "the lone wheel is still judged")
            self.assertFalse(any("sdist" in f for f in failures), failures)

    def test_lane_mode_still_demands_the_sdist(self):
        # Lane mode forgives unbuilt platforms, never the missing sdist:
        # the lane builds it beside the wheels it did build.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_zip(root / WHEEL_NAMES[0], _wheel_entries(WHEEL_NAMES[0]))
            infos: list[str] = []
            failures = gate.check_python_wheels(
                root, MIN_NATIVE, MARGIN, allow_missing=True, not_built=infos
            )
            self.assertTrue(any("no sdist" in f for f in failures), failures)

    def test_extract_pair_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_sdist(root, "pdfluent_extract-0.1.0.tar.gz")
            _write_zip(
                root / "pdfluent_extract-0.1.0-py3-none-any.whl",
                {"pdfluent_extract/__init__.py": b"# extract\n"},
            )
            self.assertEqual(gate.check_extract_dir(root), [])

    def test_extract_missing_sdist_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_zip(
                root / "pdfluent_extract-0.1.0-py3-none-any.whl",
                {"pdfluent_extract/__init__.py": b"# extract\n"},
            )
            failures = gate.check_extract_dir(root)
            self.assertTrue(any("no sdist" in f for f in failures), failures)

    def test_extract_missing_wheel_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_sdist(root, "pdfluent_extract-0.1.0.tar.gz")
            failures = gate.check_extract_dir(root)
            self.assertTrue(
                any("no pure wheel" in f for f in failures), failures
            )

    def test_extract_two_wheels_fail_as_ambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_sdist(root, "pdfluent_extract-0.1.0.tar.gz")
            for version in ("0.1.0", "0.0.9"):
                _write_zip(
                    root / f"pdfluent_extract-{version}-py3-none-any.whl",
                    {"pdfluent_extract/__init__.py": b"# extract\n"},
                )
            failures = gate.check_extract_dir(root)
            self.assertTrue(
                any("more than one pure wheel" in f for f in failures),
                failures,
            )

    def test_extract_version_mismatch_fails_by_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            _write_sdist(root, "pdfluent_extract-0.1.0.tar.gz")
            _write_zip(
                root / "pdfluent_extract-0.0.9-py3-none-any.whl",
                {"pdfluent_extract/__init__.py": b"# extract\n"},
            )
            failures = gate.check_extract_dir(root)
            self.assertTrue(
                any("not one release" in f for f in failures), failures
            )

    def test_extract_missing_dir_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            failures = gate.check_extract_dir(pathlib.Path(tmp) / "absent")
            self.assertTrue(
                any("not found" in f for f in failures), failures
            )

    def test_extract_dir_flag_is_wired(self):
        import contextlib

        with tempfile.TemporaryDirectory() as tmp:
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gate.main(["--extract-dir", tmp])
            out = buf.getvalue()
            self.assertEqual(rc, 1, out)
            self.assertIn("extract: no sdist", out)
            self.assertIn("extract: no pure wheel", out)


if __name__ == "__main__":
    unittest.main()
