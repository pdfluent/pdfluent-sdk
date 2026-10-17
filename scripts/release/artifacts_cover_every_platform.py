#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""artifacts_cover_every_platform.py — the artefact gate (m30 requirement 1).

Every channel ships natives, and every layer that used to check them was green
for the wrong reason: the Java build never staged any, the Maven guard read the
POM instead of the jar, the consumer smoke never loaded one, the runbook
recorded 45,174 bytes without judging it, and the post-publish check read the
registry page instead of the package. This gate opens the built artefacts and
requires every platform's native, in one script with one platform table per
channel, so a new platform in the matrix is required everywhere at once.

The sixth channel is the `pdfluent` CLI (T6): seven release archives, each
holding the binary plus the licence/notice/readme set, with SHA256SUMS beside
them. The same script also checks the two derived artefacts that must agree
with those archives: the Homebrew formula (sha256 equal to SHA256SUMS) and the
Docker manifest list (exactly linux/amd64 + linux/arm64).

Usage (run in CI and before any publish job):
  python3 scripts/release/artifacts_cover_every_platform.py \
      --jar bindings/java/target/pdfluent-1.1.0.jar \
      --wheels dist/ \
      --nupkg dist/PDFluent.1.1.0.nupkg \
      --node-dir crates/pdf-node \
      --capi-dir dist/capi \
      --cli-dir dist/cli --cli-version 1.0.0 \
      --extract-dir dist/extract \
      --formula Formula/pdfluent.rb --sums dist/cli/SHA256SUMS \
      --docker-manifest /tmp/pdfluent-manifest.json

Omit a flag to skip that channel. Each checked channel fails (exit 1) naming
the channel and the missing platform. A channel passes only when every native
is present AND each native clears the per-native size floor
(--min-native-bytes x (1 - --margin)), IS its slot (kind, arch and libc read
off the bytes), and the archives carry both licence texts.

`--require-all` is the release mode: a channel whose flag is absent fails
the run (exit 1) naming the channel, so a dropped flag is red instead of a
silently skipped channel. Bare it demands all six native channels; with
names (e.g. `--require-all node`) it demands only those, for the lanes
that judge one channel.

`--allow-missing` is the build lane's mode (scripts/release/
build_all_platforms.sh builds some platforms, not all): platforms with no
artefact are reported as `not built: ...` lines instead of failures, and the
gate judges what IS there. A channel with nothing built at all still fails
— "nothing to judge" is not a pass — and the release needs the full set,
so the runbook never passes this flag.

Tests: scripts/release/test_artifacts_cover_every_platform.py. For the
post-publish twin that re-reads the live registries, see
scripts/release/post_publish_contents_check.sh (m30 requirement 2).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

try:
    import license_audit
except ImportError:  # loaded by path (see the tests) rather than run as a script
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import license_audit

# ---------------------------------------------------------------------------
# One platform table for every channel. A new platform is added HERE and
# required everywhere at once; no channel keeps a private list.
#
# Each row: the per-channel names plus what the native in that slot must BE:
# kind (elf/macho/pe), arch (x86-64/aarch64) and, on Linux, the libc
# (gnu/musl). The identity check reads those three off the bytes, so a host
# fallback that stages an x86 dylib into the osx-arm64 slot fails here with
# both names on it.
# ---------------------------------------------------------------------------

# key, java dir, nuget rid, npm tag, python tag fragments, capi asset,
# (cli platform, cli triple, cli ext), kind, arch, libc.
PLATFORMS: tuple[tuple, ...] = (
    (
        "linux-x64", "linux-x64", "linux-x64", "linux-x64-gnu",
        ("manylinux2014_x86_64", "manylinux_2_17_x86_64"),
        "libpdf_capi.linux-x64.so",
        ("linux-x64", "x86_64-unknown-linux-gnu", "tar.gz"),
        "elf", "x86-64", "gnu",
    ),
    (
        "linux-arm64", "linux-arm64", "linux-arm64", "linux-arm64-gnu",
        ("manylinux2014_aarch64", "manylinux_2_17_aarch64"),
        "libpdf_capi.linux-arm64-gnu.so",
        ("linux-arm64", "aarch64-unknown-linux-gnu", "tar.gz"),
        "elf", "aarch64", "gnu",
    ),
    (
        "linux-x64-musl", "linux-x64-musl", "linux-musl-x64", "linux-x64-musl",
        ("musllinux_1_2_x86_64",),
        "libpdf_capi.linux-x64-musl.so",
        ("linux-x64-musl", "x86_64-unknown-linux-musl", "tar.gz"),
        "elf", "x86-64", "musl",
    ),
    (
        "linux-arm64-musl", "linux-arm64-musl", "linux-musl-arm64",
        "linux-arm64-musl",
        ("musllinux_1_2_aarch64",),
        "libpdf_capi.linux-arm64-musl.so",
        ("linux-arm64-musl", "aarch64-unknown-linux-musl", "tar.gz"),
        "elf", "aarch64", "musl",
    ),
    (
        "osx-x64", "osx-x64", "osx-x64", "darwin-x64",
        ("macosx",),
        "libpdf_capi.osx-x64.dylib",
        ("macos-x64", "x86_64-apple-darwin", "tar.gz"),
        "macho", "x86-64", None,
    ),
    (
        "osx-arm64", "osx-arm64", "osx-arm64", "darwin-arm64",
        ("macosx",),
        "libpdf_capi.osx-arm64.dylib",
        ("macos-arm64", "aarch64-apple-darwin", "tar.gz"),
        "macho", "aarch64", None,
    ),
    (
        "win-x64", "win-x64", "win-x64", "win32-x64-msvc",
        ("win_amd64",),
        "pdf_capi.win-x64.dll",
        ("win-x64", "x86_64-pc-windows-msvc", "zip"),
        "pe", "x86-64", None,
    ),
)

_K, _J, _R, _N, _P, _C, _T, _KIND, _ARCH, _LIBC = range(10)


def _dedupe(rows: list) -> tuple:
    seen: set = set()
    out: list = []
    for row in rows:
        if row not in seen:
            seen.add(row)
            out.append(row)
    return tuple(out)


# Maven jar layout: native/<platform>/lib{pdfluent_java,pdf_capi}.<ext>.
# Seven platforms: the four 1.0.0 RIDs never shipped (the jar held no native
# at all) plus the three new Linux targets from the 30-09 matrix.
JAVA_PLATFORMS = tuple(r[_J] for r in PLATFORMS)
JAVA_LIBS = ("pdfluent_java", "pdf_capi")


def _java_lib_file(platform: str, stem: str) -> str:
    if platform.startswith("win"):
        return f"{stem}.dll"
    if platform.startswith("osx"):
        return f"lib{stem}.dylib"
    return f"lib{stem}.so"


def java_expected() -> list[str]:
    return [
        f"native/{p}/{_java_lib_file(p, s)}" for p in JAVA_PLATFORMS for s in JAVA_LIBS
    ]


# Python wheels: one abi3 wheel per platform tag. The platform tag is the last
# `-`-separated field of the wheel filename (dots join compatible tags, e.g.
# manylinux_2_17_x86_64.manylinux2014_x86_64). Each entry is the set of
# alternative tag fragments of which at least one wheel must carry one. The
# two mac rows collapse to one `macos` entry — but one x86 mac wheel does NOT
# satisfy it: macOS needs universal2, or both archs (see check_python_wheels).
PYTHON_PLATFORMS: tuple[tuple[str, ...], ...] = _dedupe([r[_P] for r in PLATFORMS])
PYTHON_PLATFORM_NAMES = _dedupe(
    ["macos" if r[_K].startswith("osx") else r[_K] for r in PLATFORMS]
)

# NuGet: runtimes/<rid>/native/<lib> for every RID.
NUGET_RIDS = tuple(r[_R] for r in PLATFORMS)


def _nuget_lib_file(rid: str) -> str:
    if rid.startswith("win"):
        return "pdf_capi.dll"
    if rid.startswith("osx"):
        return "libpdf_capi.dylib"
    return "libpdf_capi.so"


def nuget_expected() -> list[str]:
    return [f"runtimes/{r}/native/{_nuget_lib_file(r)}" for r in NUGET_RIDS]


# npm: the seven optional platform packages, each with its .node.
NPM_TAGS = tuple(r[_N] for r in PLATFORMS)
NPM_SCOPE = "@pdfluent/node"


def npm_package_names() -> list[str]:
    return [f"{NPM_SCOPE}-{t}" for t in NPM_TAGS]


# C API release assets: one library per target, plus the header (unchanged).
CAPI_ASSETS = tuple(r[_C] for r in PLATFORMS)
CAPI_HEADER = "pdfluent.h"


# CLI release archives (T6): (platform name, target triple, archive ext).
# The same seven platforms as the other channels — one table, so a platform
# that is added is required everywhere. Archive layout per platform:
#   pdfluent-<version>-<triple>.tar.gz   (all but Windows)
#   pdfluent-<version>-<triple>.zip      (Windows)
# each holding the `pdfluent[.exe]` binary plus the licence/notice/readme set
# and the man pages under man/man1/.
CLI_PLATFORMS: tuple[tuple[str, str, str], ...] = tuple(r[_T] for r in PLATFORMS)


def _row_by(column: int, name: str) -> tuple:
    for r in PLATFORMS:
        if r[column] == name:
            return r
    raise KeyError(name)


def _want(column: int, name: str) -> tuple[str, str, str | None]:
    """(kind, arch, libc) the native in the `name` slot must be."""
    row = _row_by(column, name)
    return (row[_KIND], row[_ARCH], row[_LIBC])
CLI_ARCHIVE_MEMBERS = (
    "LICENSE",
    "LICENSE-COMMERCIAL",
    "NOTICE",
    "THIRD_PARTY_LICENSES.txt",
    "README.md",
    "man/man1/pdfluent.1",
)
CLI_MUSL_PLATFORMS = ("linux-x64-musl", "linux-arm64-musl")


def cli_archive_name(version: str, triple: str, ext: str) -> str:
    return f"pdfluent-{version}-{triple}.{ext}"


def cli_binary_name(platform: str) -> str:
    return "pdfluent.exe" if platform == "win-x64" else "pdfluent"


# Homebrew serves the four macOS + glibc archives (no musl, no Windows).
HOMEBREW_PLATFORMS = ("macos-arm64", "macos-x64", "linux-x64", "linux-arm64")

# `file -b` must contain every token (lowercased) for the platform.
CLI_FILE_TOKENS: dict[str, tuple[str, ...]] = {
    "macos-arm64": ("mach-o", "arm64"),
    "macos-x64": ("mach-o", "x86_64"),
    "linux-x64": ("elf", "x86-64"),
    "linux-arm64": ("elf", "aarch64"),
    "linux-x64-musl": ("elf", "x86-64"),
    "linux-arm64-musl": ("elf", "aarch64"),
    "win-x64": ("pe32+", "x86-64"),
}

DOCKER_PLATFORMS = (("linux", "amd64"), ("linux", "arm64"))


# ---------------------------------------------------------------------------
# Checks. Each returns a list of failure strings (empty means pass).
# ---------------------------------------------------------------------------

def _size_floor_failures(
    channel: str, native_sizes: dict[str, int], min_native_bytes: int, margin: float
) -> list[str]:
    """Every native is judged on its own, not as a sum.

    The floor used to be one sum over the set, so a single 0-byte native
    passed on the back of its six siblings. Now each native must clear
    smallest-plausible-native minus a margin, and a 0-byte file fails by name.
    """
    floor = int(min_native_bytes * (1.0 - margin))
    failures = []
    for name in sorted(native_sizes):
        size = native_sizes[name]
        if size == 0:
            failures.append(f"{channel}: {name} is empty (0 bytes)")
        elif size < floor:
            failures.append(
                f"{channel}: {name} is {size} bytes, below the per-native "
                f"floor of {floor} ({min_native_bytes} x (1 - {margin}))"
            )
    return failures


# ---------------------------------------------------------------------------
# Native identity: kind, arch and libc read off the bytes.
# ---------------------------------------------------------------------------

_ELF_MACHINES = {62: "x86-64", 183: "aarch64", 40: "arm", 3: "x86"}
_MACHO_CPUS = {0x01000007: "x86-64", 0x0100000C: "aarch64"}
_PE_MACHINES = {0x8664: "x86-64", 0xAA64: "aarch64", 0x14C: "x86", 0x1C0: "arm"}

# Dynamic-section tags the libc verdict reads.
_DT_NULL = 0
_DT_NEEDED = 1
_DT_STRTAB = 5
_DT_STRSZ = 10
_PT_LOAD = 1
_PT_DYNAMIC = 2


def _elf_interp(data: bytes) -> str | None:
    """The PT_INTERP of an ELF file, or None when it has none to read.

    Authoritative for executables; shared libraries carry no interpreter and
    fall through to DT_NEEDED in _elf_libc.
    """
    try:
        ei_class = data[4]
        if ei_class == 1:  # 32-bit: e_phoff @28, e_phentsize @42, e_phnum @44
            phoff = struct.unpack_from("<I", data, 28)[0]
            phentsize = struct.unpack_from("<H", data, 42)[0]
            phnum = struct.unpack_from("<H", data, 44)[0]
            dyn = False
        elif ei_class == 2:  # 64-bit: e_phoff @32, e_phentsize @54, e_phnum @56
            phoff = struct.unpack_from("<Q", data, 32)[0]
            phentsize = struct.unpack_from("<H", data, 54)[0]
            phnum = struct.unpack_from("<H", data, 56)[0]
            dyn = True
        else:
            return None
        for i in range(min(phnum, 32)):
            off = phoff + i * phentsize
            p_type = struct.unpack_from("<I", data, off)[0]
            if p_type != 3:  # PT_INTERP
                continue
            p_offset = struct.unpack_from("<Q" if dyn else "<I", data, off + 8)[0]
            raw = data[p_offset : p_offset + 256].split(b"\0", 1)[0]
            return raw.decode("latin1")
    except (struct.error, IndexError):
        return None
    return None


def _elf_dt_needed(data: bytes) -> list[str] | None:
    """The DT_NEEDED sonames of an ELF file, or None when it has no dynamic
    section to read.

    The libc verdict reads these names -- "libc.so.6" is gnu,
    "libc.musl-<arch>.so.1" (or the bare "libc.so") is musl -- instead of
    grepping the whole file: a comment or a version string mentioning
    GLIBC_ inside a musl-linked object used to flip the verdict to gnu.
    Static binaries have no PT_DYNAMIC and read as None, as do
    big-endian objects and truncated headers.
    """
    try:
        if len(data) < 64 or data[:4] != b"\x7fELF":
            return None
        ei_class = data[4]
        if data[5] != 1:  # little-endian only
            return None
        if ei_class == 1:  # 32-bit
            phoff = struct.unpack_from("<I", data, 28)[0]
            phentsize = struct.unpack_from("<H", data, 42)[0]
            phnum = struct.unpack_from("<H", data, 44)[0]
            is64 = False
        elif ei_class == 2:  # 64-bit
            phoff = struct.unpack_from("<Q", data, 32)[0]
            phentsize = struct.unpack_from("<H", data, 54)[0]
            phnum = struct.unpack_from("<H", data, 56)[0]
            is64 = True
        else:
            return None
        loads: list[tuple[int, int, int]] = []  # (vaddr, offset, filesz)
        dynamic: tuple[int, int] | None = None  # (offset, filesz)
        for i in range(min(phnum, 32)):
            off = phoff + i * phentsize
            p_type = struct.unpack_from("<I", data, off)[0]
            if is64:
                p_offset = struct.unpack_from("<Q", data, off + 8)[0]
                p_vaddr = struct.unpack_from("<Q", data, off + 16)[0]
                p_filesz = struct.unpack_from("<Q", data, off + 32)[0]
            else:
                p_offset = struct.unpack_from("<I", data, off + 4)[0]
                p_vaddr = struct.unpack_from("<I", data, off + 8)[0]
                p_filesz = struct.unpack_from("<I", data, off + 16)[0]
            if p_type == _PT_LOAD:
                loads.append((p_vaddr, p_offset, p_filesz))
            elif p_type == _PT_DYNAMIC and dynamic is None:
                dynamic = (p_offset, p_filesz)
        if dynamic is None:
            return None
        dyn_off, dyn_size = dynamic
        needed_idx: list[int] = []
        strtab: int | None = None
        strsz: int | None = None
        entsize = 16 if is64 else 8
        for off in range(dyn_off, dyn_off + dyn_size, entsize):
            if is64:
                tag, val = struct.unpack_from("<qQ", data, off)
            else:
                tag, val = struct.unpack_from("<iI", data, off)
            if tag == _DT_NULL:
                break
            if tag == _DT_NEEDED:
                needed_idx.append(val)
            elif tag == _DT_STRTAB:
                strtab = val
            elif tag == _DT_STRSZ:
                strsz = val
        if strtab is None:
            return None
        str_off: int | None = None
        for vaddr, offset, filesz in loads:
            if vaddr <= strtab < vaddr + filesz:
                str_off = strtab - vaddr + offset
                break
        if str_off is None:
            return None
        end = str_off + (strsz if strsz else 512)
        names = []
        for idx in needed_idx:
            if str_off + idx >= len(data):
                return None
            raw = data[str_off + idx : end].split(b"\0", 1)[0]
            names.append(raw.decode("latin1"))
        return names
    except (struct.error, IndexError):
        return None


def _elf_libc(data: bytes) -> str | None:
    interp = _elf_interp(data)
    if interp is not None:
        if "ld-musl" in interp:
            return "musl"
        if "ld-linux" in interp:
            return "gnu"
    for name in _elf_dt_needed(data) or []:
        lowered = name.lower()
        if "musl" in lowered or lowered == "libc.so":
            return "musl"
        if lowered == "libc.so.6" or lowered.startswith("ld-linux"):
            return "gnu"
    return None


def elf_is_static(data: bytes) -> bool:
    """True when an ELF binary links no shared library at all.

    No PT_INTERP and no DT_NEEDED. A fully static link has neither a
    dynamic section nor an interpreter; a static-pie (what Rust musl
    builds) has a dynamic section with no DT_NEEDED in it and no
    interpreter. Both read static here; an interpreter or any DT_NEEDED
    reads dynamic, as does anything that is not ELF at all.

    This is the musl static check, and it reads the binary's own headers
    because `ldd` answers about the HOST: on an x86 runner it calls every
    arm64 binary "not a dynamic executable" (so the old check passed a
    dynamic musl binary there), while modern ldd calls a static-pie
    "statically linked" (which the old string match read as dynamic).
    """
    if len(data) < 4 or data[:4] != b"\x7fELF":
        return False
    if _elf_interp(data) is not None:
        return False
    return not _elf_dt_needed(data)


# DT_NEEDED sonames a shipped native may link beside its libc. The libc
# verdict decides gnu-vs-musl off the libc entries; this list judges the
# REST: a dependency the release did not declare fails the slot naming
# the soname. Extend here — with a companion case in
# test_artifacts_cover_every_platform.py — when a real artefact needs a
# library this list does not know.
_DT_NEEDED_ALLOWLIST = frozenset(
    {
        "libc.so.6",
        "libc.so",
        "libm.so.6",
        "libpthread.so.0",
        "libdl.so.2",
        "libgcc_s.so.1",
    }
)
_DT_NEEDED_ALLOWLIST_PREFIXES = ("libc.musl-", "ld-linux")


def _dt_needed_allowed(soname: str) -> bool:
    lowered = soname.lower()
    if lowered in _DT_NEEDED_ALLOWLIST:
        return True
    return lowered.startswith(_DT_NEEDED_ALLOWLIST_PREFIXES)


def _dt_needed_allowlist_failures(
    channel: str, where: str, data: bytes
) -> list[str]:
    """Every DT_NEEDED entry must be a library the release declared."""
    failures = []
    for soname in _elf_dt_needed(data) or []:
        if not _dt_needed_allowed(soname):
            failures.append(
                f"{channel}: {where} links unexpected {soname} "
                "(not on the DT_NEEDED allowlist; extend "
                "_DT_NEEDED_ALLOWLIST when a real artefact needs it)"
            )
    return failures


def _macho_fat_archs(data: bytes) -> set[str] | None:
    """The archs a fat Mach-O's header lists, or None when unreadable.

    A universal2 wheel must hold a FAT binary listing BOTH x86_64 and
    arm64: a thin x86_64 .so under a universal2 tag installs on arm64
    and dies at import. The fat header is big-endian in both its forms
    (fat_arch and fat_arch_64); a truncated header reads as None, never
    as an arch that is not there.
    """
    if len(data) < 8:
        return None
    if data[:4] == b"\xca\xfe\xba\xbe":
        entsize = 20
    elif data[:4] == b"\xca\xfe\xba\xbf":
        entsize = 32
    else:
        return None
    try:
        nfat = struct.unpack_from(">I", data, 4)[0]
        if nfat > 16 or len(data) < 8 + nfat * entsize:
            return None
        archs = set()
        for i in range(nfat):
            cputype = struct.unpack_from(">I", data, 8 + i * entsize)[0]
            archs.add(_MACHO_CPUS.get(cputype, f"cputype={cputype:#x}"))
        return archs
    except struct.error:
        return None


def native_identity(data: bytes) -> tuple[str | None, str | None, str | None]:
    """(kind, arch, libc) read off a native's bytes.

    kind is elf/macho/pe (None when the magic matches none of them), arch is
    x86-64/aarch64 (or the raw machine id when it is valid but unshipped, so
    the message names a number rather than a shrug), and libc is gnu/musl on
    ELF (None when neither PT_INTERP nor DT_NEEDED answers, and always None
    off ELF). A fat Mach-O reports the arch "universal" only when its fat
    header lists BOTH x86-64 and aarch64: it holds both mac slots' bytes. A
    fat file missing one reports what it holds (fat-<archs>-only) and an
    unreadable fat header reports fat-unreadable, so either fails its slot
    naming what is missing.
    """
    if len(data) >= 4 and data[:4] == b"\x7fELF":
        if len(data) < 20:
            return ("elf", None, None)
        machine = struct.unpack_from("<H", data, 18)[0]
        return ("elf", _ELF_MACHINES.get(machine, f"e_machine={machine}"), _elf_libc(data))
    if len(data) >= 8 and data[:4] in (
        b"\xcf\xfa\xed\xfe",
        b"\xce\xfa\xed\xfe",
    ):
        cputype = struct.unpack_from("<I", data, 4)[0]
        return ("macho", _MACHO_CPUS.get(cputype, f"cputype={cputype:#x}"), None)
    if len(data) >= 4 and data[:4] in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        archs = _macho_fat_archs(data)
        if archs is not None and {"x86-64", "aarch64"} <= archs:
            return ("macho", "universal", None)
        if archs is None:
            return ("macho", "fat-unreadable", None)
        if not archs:
            return ("macho", "fat-empty", None)
        return ("macho", f"fat-{'-'.join(sorted(archs))}-only", None)
    if len(data) >= 64 and data[:2] == b"MZ":
        try:
            pe_off = struct.unpack_from("<I", data, 0x3C)[0]
            magic = data[pe_off : pe_off + 4]
            machine = struct.unpack_from("<H", data, pe_off + 4)[0]
        except struct.error:
            return ("pe", None, None)
        if magic != b"PE\0\0":
            return ("pe", None, None)
        return ("pe", _PE_MACHINES.get(machine, f"Machine={machine:#x}"), None)
    return (None, None, None)


def _identity_failures(
    channel: str,
    expected: dict[str, tuple[str, str, str | None]],
    blobs: dict[str, bytes],
) -> list[str]:
    """Each present native must BE its slot: kind, arch and libc.

    A host fallback that stages an x86 dylib into the osx-arm64 slot used to
    pass — the gate counted files. Now the mismatch fails naming what the
    bytes are and what the slot wants.
    """
    failures = []
    for name in sorted(expected):
        if name not in blobs:
            continue  # missing is reported where presence is checked
        want_kind, want_arch, want_libc = expected[name]
        got_kind, got_arch, got_libc = native_identity(blobs[name])
        if got_kind is None:
            failures.append(
                f"{channel}: {name} is not a native at all "
                f"(bad magic; want {want_kind} {want_arch}"
                + (f" {want_libc}" if want_libc else "")
                + ")"
            )
        elif got_kind != want_kind:
            failures.append(
                f"{channel}: {name} is {got_kind}, want {want_kind} "
                f"for this slot ({got_arch}/{want_arch})"
            )
        elif got_arch != want_arch and got_arch != "universal":
            # A verified universal holds both mac slots' bytes, so it passes
            # either one; anything else fat names what it is missing above.
            failures.append(
                f"{channel}: {name} is {want_kind} {got_arch}, "
                f"want {want_kind} {want_arch} for this slot"
            )
        elif want_libc is not None:
            if got_libc is None:
                failures.append(
                    f"{channel}: {name} carries no libc marker (no PT_INTERP "
                    "and no DT_NEEDED naming a libc); cannot tell gnu "
                    f"from musl for this {want_libc} slot"
                )
            elif got_libc != want_libc:
                failures.append(
                    f"{channel}: {name} links {got_libc} libc, "
                    f"want {want_libc} for this slot"
                )
        if got_kind == "elf":
            failures.extend(
                _dt_needed_allowlist_failures(channel, name, blobs[name])
            )
    return failures


def _absent(failures: list[str], not_built: list[str] | None,
            allow_missing: bool, failure: str, info: str) -> None:
    """A platform with no artefact: a failure, or a not-built line.

    In lane mode (--allow-missing, with a list to collect into) the missing
    platform is reported, not failed: the lane gates what it built and names
    the rest. Without the list even lane mode fails — a missing platform
    that is neither failed nor named is the silent pass this gate exists
    to kill.
    """
    if allow_missing and not_built is not None:
        not_built.append(info)
    else:
        failures.append(failure)


def _wheel_identity(
    platform_tag: str,
) -> tuple[str, tuple[str, ...], str | None] | None:
    """(kind, allowed archs, libc) for a wheel platform tag, or None."""
    tag = platform_tag.lower()
    if "musllinux" in tag or "manylinux" in tag:
        libc = "musl" if "musllinux" in tag else "gnu"
        if "x86_64" in tag:
            return ("elf", ("x86-64",), libc)
        if "aarch64" in tag:
            return ("elf", ("aarch64",), libc)
        return None
    if "macosx" in tag:
        if "universal2" in tag:
            # universal2 is a promise of a FAT binary: a thin x86_64 .so
            # installs on arm64 and dies at import, so only a verified
            # universal clears this tag.
            return ("macho", ("universal",), None)
        archs = tuple(
            a
            for frag, a in (("x86_64", "x86-64"), ("arm64", "aarch64"))
            if frag in tag
        )
        return ("macho", archs, None) if archs else None
    if "win" in tag:
        if "amd64" in tag or "x64" in tag:
            return ("pe", ("x86-64",), None)
        if "arm64" in tag:
            return ("pe", ("aarch64",), None)
        return None
    return None


def check_maven_jar(
    jar: Path,
    min_native_bytes: int,
    margin: float,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    failures = []
    if not jar.is_file():
        return [f"maven: jar not found: {jar}"]
    try:
        with zipfile.ZipFile(jar) as zf:
            entries = {i.filename: i.file_size for i in zf.infolist()}
            blobs = {
                e: zf.read(e) for e in java_expected() if e in entries
            }
    except zipfile.BadZipFile as e:
        return [f"maven: {jar} is not a readable jar: {e}"]
    missing = [e for e in java_expected() if e not in entries]
    for e in missing:
        platform = e.split("/")[1]
        _absent(
            failures, not_built, allow_missing,
            f"maven: jar {jar} misses platform {platform} ({e})",
            f"maven: not built: {platform} ({e})",
        )
    if allow_missing and not_built is not None and not blobs:
        failures.append(f"maven: jar {jar}: nothing built (no native for any platform)")
    present = {e: entries[e] for e in java_expected() if e in entries}
    failures.extend(_size_floor_failures("maven", present, min_native_bytes, margin))
    judged = (
        [e for e in java_expected() if e in blobs]
        if allow_missing and not_built is not None
        else java_expected()
    )
    expected = {e: _want(_J, e.split("/")[1]) for e in judged}
    failures.extend(_identity_failures("maven", expected, blobs))
    failures.extend(license_audit.check_archive(jar))
    return failures


def _exactly_one_sdist(
    channel: str, where: Path, pattern: str, want: str
) -> list[str]:
    """Exactly one sdist beside the artefacts: none is a missing release
    file, two is an ambiguous set (the upload would carry the wrong one)."""
    hits = sorted(where.glob(pattern))
    if not hits:
        return [
            f"{channel}: no sdist beside the artefacts in {where} "
            f"(want exactly one {want})"
        ]
    if len(hits) > 1:
        names = ", ".join(h.name for h in hits)
        return [
            f"{channel}: more than one sdist in {where}: {names} "
            "(want exactly one)"
        ]
    return []


def check_python_wheels(
    wheels: Path,
    min_native_bytes: int,
    margin: float,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    failures = []
    if wheels.is_file() and wheels.suffix == ".whl":
        files = [wheels]
    elif wheels.is_dir():
        files = sorted(wheels.glob("*.whl"))
    else:
        return [f"python: no wheels at {wheels}"]
    if not files:
        return [f"python: no .whl files in {wheels}"]
    sdist_dir = wheels if wheels.is_dir() else wheels.parent
    failures.extend(
        _exactly_one_sdist(
            "python", sdist_dir, "pdfluent-*.tar.gz",
            "pdfluent-<version>.tar.gz",
        )
    )
    tags = {f.name: f.name.removesuffix(".whl").split("-")[-1] for f in files}
    for name, fragments in zip(PYTHON_PLATFORM_NAMES, PYTHON_PLATFORMS):
        if name == "macos":
            # One x86 mac wheel is not macOS: universal2, or both archs.
            mac_tags = [t for t in tags.values() if "macosx" in t]
            universal = any("universal2" in t for t in mac_tags)
            has_x64 = any("x86_64" in t for t in mac_tags)
            has_arm64 = any("arm64" in t and "x86_64" not in t for t in mac_tags)
            if not mac_tags:
                _absent(
                    failures, not_built, allow_missing,
                    f"python: no wheel for platform macos "
                    f"(need a universal2 or per-arch macosx wheel; "
                    f"saw {sorted(set(tags.values()))})",
                    "python: not built: macos",
                )
            elif not universal and not (has_x64 and has_arm64):
                failures.append(
                    f"python: macos needs a universal2 wheel or both archs, "
                    f"saw only {sorted(set(mac_tags))}"
                )
            continue
        if not any(
            frag in tag for tag in tags.values() for frag in fragments
        ):
            _absent(
                failures, not_built, allow_missing,
                f"python: no wheel for platform {name} "
                f"(need one of {', '.join(fragments)}; saw {sorted(set(tags.values()))})",
                f"python: not built: {name}",
            )
    for f in files:
        # abi3 is the channel's promise: one wheel per platform, importable
        # from every supported CPython. A cp311-cp311 wheel breaks that.
        parts = f.name.removesuffix(".whl").split("-")
        abi = parts[3] if len(parts) >= 5 else ""
        if abi != "abi3":
            failures.append(
                f"python: {f.name} is built for {abi or 'an unparsable tag'}, "
                "want abi3"
            )
    native_sizes: dict[str, int] = {}
    for f in files:
        try:
            with zipfile.ZipFile(f) as zf:
                members = [
                    i for i in zf.infolist()
                    if i.filename.endswith((".so", ".pyd"))
                ]
                if not members:
                    failures.append(
                        f"python: {f.name} carries no native (.so/.pyd) at all"
                    )
                    continue
                # The extension is the largest native; smaller bundled .so
                # files are judged on identity below, not on size.
                biggest = max(members, key=lambda i: i.file_size)
                native_sizes[f"{f.name}:{biggest.filename}"] = biggest.file_size
                ident = _wheel_identity(tags[f.name])
                if ident is None:
                    failures.append(
                        f"python: {f.name} has an unrecognized platform tag "
                        f"{tags[f.name]!r}; cannot tell what its native must be"
                    )
                    continue
                want_kind, want_archs, want_libc = ident
                for m in members:
                    blob = zf.read(m.filename)
                    got_kind, got_arch, got_libc = native_identity(blob)
                    where = f"{f.name}:{m.filename}"
                    if got_kind != want_kind:
                        failures.append(
                            f"python: {where} is {got_kind or 'not a native'}, "
                            f"want {want_kind} for {tags[f.name]}"
                        )
                    elif got_arch not in want_archs:
                        failures.append(
                            f"python: {where} is {want_kind} {got_arch}, "
                            f"want {want_kind} {'/'.join(want_archs)} "
                            f"for {tags[f.name]}"
                        )
                    elif want_libc is not None:
                        if got_libc is None:
                            failures.append(
                                f"python: {where} carries no libc marker "
                                "(no PT_INTERP and no DT_NEEDED naming a "
                                "libc); "
                                f"cannot tell gnu from musl for {tags[f.name]}"
                            )
                        elif got_libc != want_libc:
                            failures.append(
                                f"python: {where} links {got_libc} libc, "
                                f"want {want_libc} for {tags[f.name]}"
                            )
                    if got_kind == "elf":
                        failures.extend(
                            _dt_needed_allowlist_failures(
                                "python", where, blob
                            )
                        )
        except zipfile.BadZipFile:
            failures.append(f"python: {f.name} is not a readable wheel")
        failures.extend(license_audit.check_archive(f))
    failures.extend(
        _size_floor_failures("python", native_sizes, min_native_bytes, margin)
    )
    return failures


# The pure-Python wrapper: one sdist and one pure wheel, a pair from one
# release. Platform-independent, so no per-platform natives — but the pair
# must still be there, and still be one release.
EXTRACT_SDIST_GLOB = "pdfluent_extract-*.tar.gz"
EXTRACT_WHEEL_GLOB = "pdfluent_extract-*-py3-none-any.whl"


def check_extract_dir(extract_dir: Path) -> list[str]:
    failures = []
    if not extract_dir.is_dir():
        return [f"extract: dist dir not found: {extract_dir}"]
    failures.extend(
        _exactly_one_sdist(
            "extract", extract_dir, EXTRACT_SDIST_GLOB,
            "pdfluent_extract-<version>.tar.gz",
        )
    )
    wheels = sorted(extract_dir.glob(EXTRACT_WHEEL_GLOB))
    if not wheels:
        failures.append(
            f"extract: no pure wheel in {extract_dir} "
            "(want exactly one pdfluent_extract-<version>-py3-none-any.whl)"
        )
    elif len(wheels) > 1:
        names = ", ".join(w.name for w in wheels)
        failures.append(
            f"extract: more than one pure wheel in {extract_dir}: {names} "
            "(want exactly one)"
        )
    sdists = sorted(extract_dir.glob(EXTRACT_SDIST_GLOB))
    if len(sdists) == 1 and len(wheels) == 1:
        sdist_version = sdists[0].name.removeprefix(
            "pdfluent_extract-"
        ).removesuffix(".tar.gz")
        wheel_version = wheels[0].name.removeprefix(
            "pdfluent_extract-"
        ).split("-")[0]
        if sdist_version != wheel_version:
            failures.append(
                f"extract: {sdists[0].name} and {wheels[0].name} are not "
                f"one release ({sdist_version} vs {wheel_version})"
            )
    return failures


def check_nuget(
    nupkg: Path,
    min_native_bytes: int,
    margin: float,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    failures = []
    if not nupkg.is_file():
        return [f"nuget: nupkg not found: {nupkg}"]
    try:
        with zipfile.ZipFile(nupkg) as zf:
            entries = {i.filename: i.file_size for i in zf.infolist()}
    except zipfile.BadZipFile as e:
        return [f"nuget: {nupkg} is not a readable nupkg: {e}"]
    # Zip paths use forward slashes; the csproj writes runtimes/<rid>/native/.
    lowered = {n.lower(): (n, s) for n, s in entries.items()}
    present: dict[str, int] = {}
    blobs: dict[str, bytes] = {}
    with zipfile.ZipFile(nupkg) as zf:
        for e in nuget_expected():
            hit = lowered.get(e.lower())
            if hit is None:
                rid = e.split("/")[1]
                _absent(
                    failures, not_built, allow_missing,
                    f"nuget: {nupkg.name} misses RID {rid} ({e})",
                    f"nuget: not built: {rid} ({e})",
                )
            else:
                present[e] = hit[1]
                blobs[e] = zf.read(hit[0])
    if allow_missing and not_built is not None and not blobs:
        failures.append(f"nuget: {nupkg.name}: nothing built (no native for any RID)")
    failures.extend(_size_floor_failures("nuget", present, min_native_bytes, margin))
    judged = (
        [e for e in nuget_expected() if e in blobs]
        if allow_missing and not_built is not None
        else nuget_expected()
    )
    expected = {e: _want(_R, e.split("/")[1]) for e in judged}
    failures.extend(_identity_failures("nuget", expected, blobs))
    failures.extend(license_audit.check_archive(nupkg))
    return failures


def check_node_dir(
    node_dir: Path,
    min_native_bytes: int,
    margin: float,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    failures = []
    pkg_json = node_dir / "package.json"
    if not pkg_json.is_file():
        return [f"npm: no package.json in {node_dir}"]
    try:
        manifest = json.loads(pkg_json.read_text())
    except (json.JSONDecodeError, OSError) as e:
        return [f"npm: unreadable {pkg_json}: {e}"]
    optional = manifest.get("optionalDependencies", {})
    for name in npm_package_names():
        if name not in optional:
            failures.append(
                f"npm: optionalDependencies of {NPM_SCOPE} misses {name}"
            )
    npm_dir = node_dir / "npm"
    present: dict[str, int] = {}
    blobs: dict[str, bytes] = {}
    for tag in NPM_TAGS:
        node_file = npm_dir / tag / f"index.{tag}.node"
        if not node_file.is_file():
            # Fall back to the napi flat layout (index.<tag>.node at root).
            node_file = node_dir / f"index.{tag}.node"
        if not node_file.is_file():
            _absent(
                failures, not_built, allow_missing,
                f"npm: missing platform package {tag} (.node)",
                f"npm: not built: {tag} (.node)",
            )
        else:
            present[tag] = node_file.stat().st_size
            try:
                blobs[tag] = node_file.read_bytes()
            except OSError as e:
                failures.append(f"npm: unreadable .node for {tag}: {e}")
    if allow_missing and not_built is not None and not blobs:
        failures.append(f"npm: {node_dir}: nothing built (no .node for any tag)")
    failures.extend(_size_floor_failures("npm", present, min_native_bytes, margin))
    judged = (
        [t for t in NPM_TAGS if t in blobs]
        if allow_missing and not_built is not None
        else NPM_TAGS
    )
    expected = {t: _want(_N, t) for t in judged}
    failures.extend(_identity_failures("npm", expected, blobs))
    return failures


def check_capi_dir(
    capi_dir: Path,
    min_native_bytes: int,
    margin: float,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    failures = []
    if not capi_dir.is_dir():
        return [f"capi: asset dir not found: {capi_dir}"]
    names = {p.name for p in capi_dir.iterdir()}
    present: dict[str, int] = {}
    blobs: dict[str, bytes] = {}
    for asset in CAPI_ASSETS:
        if asset not in names:
            _absent(
                failures, not_built, allow_missing,
                f"capi: missing release asset {asset}",
                f"capi: not built: {asset}",
            )
        else:
            present[asset] = (capi_dir / asset).stat().st_size
            try:
                blobs[asset] = (capi_dir / asset).read_bytes()
            except OSError as e:
                failures.append(f"capi: unreadable release asset {asset}: {e}")
    if CAPI_HEADER not in names and "include" not in names:
        failures.append(f"capi: missing header {CAPI_HEADER} (or include/)")
    if allow_missing and not_built is not None and not blobs:
        failures.append(f"capi: {capi_dir}: nothing built (no release asset present)")
    failures.extend(_size_floor_failures("capi", present, min_native_bytes, margin))
    judged = (
        [a for a in CAPI_ASSETS if a in blobs]
        if allow_missing and not_built is not None
        else CAPI_ASSETS
    )
    expected = {a: _want(_C, a) for a in judged}
    failures.extend(_identity_failures("capi", expected, blobs))
    return failures


def parse_sums_file(sums: Path) -> dict[str, str]:
    """Parse a SHA256SUMS file into {filename: hex digest}."""
    entries: dict[str, str] = {}
    for line in sums.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2:
            continue
        digest, name = parts
        entries[name.removeprefix("*")] = digest
    return entries


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _cli_archive_members(archive: Path) -> dict[str, int]:
    """{member name: size} with a single shared top dir stripped."""
    names: dict[str, int] = {}
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zf:
            for i in zf.infolist():
                if not i.is_dir():
                    names[i.filename] = i.file_size
    else:
        with tarfile.open(archive, "r:*") as tf:
            for m in tf.getmembers():
                if m.isfile():
                    names[m.name] = m.size
    # Archives may wrap everything in one top dir (pdfluent-<v>-<triple>/).
    tops = {n.split("/", 1)[0] for n in names if "/" in n}
    if len(tops) == 1 and all("/" in n for n in names):
        (top,) = tops
        names = {n[len(top) + 1:]: s for n, s in names.items()}
    return names


def _cli_licence_texts(archive: Path) -> dict[str, bytes]:
    """{licence member name: content} with the shared top dir stripped."""
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zf:
            raw = {
                i.filename: zf.read(i.filename)
                for i in zf.infolist()
                if not i.is_dir()
            }
    else:
        raw = {}
        with tarfile.open(archive, "r:*") as tf:
            for m in tf.getmembers():
                if not m.isfile():
                    continue
                with tf.extractfile(m) as src:
                    assert src is not None
                    raw[m.name] = src.read()
    tops = {n.split("/", 1)[0] for n in raw if "/" in n}
    if len(tops) == 1 and all("/" in n for n in raw):
        (top,) = tops
        raw = {n[len(top) + 1:]: c for n, c in raw.items()}
    return {
        n: c for n, c in raw.items() if os.path.basename(n).startswith("LICENSE")
    }


def _cli_arch_ok(platform: str, file_line: str) -> bool:
    lowered = file_line.lower()
    return all(t in lowered for t in CLI_FILE_TOKENS[platform])


def _run_tool(tool: str, target: Path) -> str:
    if shutil.which(tool) is None:
        raise FileNotFoundError(tool)
    proc = subprocess.run(
        [tool, str(target)], capture_output=True, text=True, timeout=60
    )
    return (proc.stdout + proc.stderr).strip()


def _cli_binary_bytes(archive: Path, binary: str) -> bytes:
    """The archived binary's bytes, whatever top dir wraps the archive."""
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zf:
            hit = next(n for n in zf.namelist() if n.endswith(binary))
            return zf.read(hit)
    with tarfile.open(archive, "r:*") as tf:
        hit = next(
            m for m in tf.getmembers() if m.isfile() and m.name.endswith(binary)
        )
        with tf.extractfile(hit) as src:
            assert src is not None
            return src.read()


def check_cli_dir(
    cli_dir: Path,
    version: str,
    min_archive_bytes: int,
    file_lines: dict[str, str] | None = None,
    *,
    allow_missing: bool = False,
    not_built: list[str] | None = None,
) -> list[str]:
    """The CLI channel: all seven archives, each complete and checksummed.

    `file_lines` injects `file -b` output per platform for tests; when None
    the tool runs against the archived binary. A missing tool is a failure
    naming the tool, never a silent skip. The musl static-link check reads
    the archived binary's own program headers (no PT_INTERP and no
    DT_NEEDED), so it judges arm64 bytes on an x86 runner exactly as well
    as native ones — no `ldd`, which answers about the host.
    """
    failures: list[str] = []
    if not cli_dir.is_dir():
        return [f"cli: archive dir not found: {cli_dir}"]

    expected = {
        plat: cli_archive_name(version, triple, ext)
        for plat, triple, ext in CLI_PLATFORMS
    }

    sums_path = cli_dir / "SHA256SUMS"
    if not sums_path.is_file():
        failures.append(f"cli: SHA256SUMS missing beside the archives in {cli_dir}")
        sums: dict[str, str] = {}
    else:
        try:
            sums = parse_sums_file(sums_path)
        except OSError as e:
            failures.append(f"cli: unreadable SHA256SUMS: {e}")
            sums = {}

    lane = allow_missing and not_built is not None
    judged_any = False
    for plat in [p for p, _, _ in CLI_PLATFORMS]:
        archive = cli_dir / expected[plat]
        archive_missing = not archive.is_file()
        if expected[plat] not in sums and not (lane and archive_missing):
            # A missing archive has no line to miss; the archive line below
            # names the platform once.
            failures.append(
                f"cli: SHA256SUMS misses platform {plat} ({expected[plat]})"
            )
        if archive_missing:
            _absent(
                failures, not_built, allow_missing,
                f"cli: missing archive for platform {plat} ({expected[plat]})",
                f"cli: not built: {plat} ({expected[plat]})",
            )
            continue
        judged_any = True
        if archive.stat().st_size < min_archive_bytes:
            failures.append(
                f"cli: archive for {plat} ({expected[plat]}) is "
                f"{archive.stat().st_size} bytes, below floor {min_archive_bytes}"
            )
        if expected[plat] in sums:
            actual = sha256_of(archive)
            if actual != sums[expected[plat]].lower():
                failures.append(
                    f"cli: SHA256SUMS digest mismatch for {plat} "
                    f"({expected[plat]})"
                )
        try:
            members = _cli_archive_members(archive)
        except (tarfile.TarError, zipfile.BadZipFile, OSError) as e:
            failures.append(f"cli: {expected[plat]} is not a readable archive: {e}")
            continue
        backslash = sorted(n for n in members if "\\" in n)
        if backslash:
            shown = backslash[0]
            if len(backslash) > 1:
                shown += f" (+{len(backslash) - 1} more)"
            failures.append(
                f"cli: archive for {plat} ({expected[plat]}): "
                f"entry uses backslashes: {shown}"
            )
        want = [cli_binary_name(plat), *CLI_ARCHIVE_MEMBERS]
        for member in want:
            if member not in members:
                failures.append(
                    f"cli: archive for {plat} ({expected[plat]}) "
                    f"misses {member}"
                )
        # The licence files must carry the licence texts, not placeholders:
        # the same audit the jar/wheel/nupkg checks run, over these members.
        try:
            texts = _cli_licence_texts(archive)
        except (tarfile.TarError, zipfile.BadZipFile, OSError) as e:
            failures.append(
                f"cli: {expected[plat]} licence members unreadable: {e}"
            )
            continue
        failures.extend(
            license_audit.check_licence_texts(
                texts, prefix=f"cli: {expected[plat]}: "
            )
        )
        # Architecture + static-link checks run against the archived binary.
        binary = cli_binary_name(plat)
        if binary not in members:
            continue
        try:
            blob = _cli_binary_bytes(archive, binary)
        except (OSError, StopIteration) as e:
            failures.append(f"cli: could not read {binary} for {plat}: {e}")
            continue
        file_line = (file_lines or {}).get(plat)
        if file_line is None and file_lines is None:
            try:
                with tempfile.TemporaryDirectory() as tmp:
                    dest = Path(tmp) / binary
                    dest.write_bytes(blob)
                    file_line = _run_tool("file", dest)
            except FileNotFoundError:
                failures.append(
                    f"cli: need `file` on PATH for the {plat} architecture "
                    "check (run this gate on Linux or macOS)"
                )
                continue
            except (OSError, subprocess.SubprocessError) as e:
                failures.append(f"cli: could not inspect {binary} for {plat}: {e}")
                continue
        if file_line is not None and not _cli_arch_ok(plat, file_line):
            failures.append(
                f"cli: {binary} for {plat} has the wrong architecture "
                f"(`file` says: {file_line})"
            )
        if plat in CLI_MUSL_PLATFORMS and not elf_is_static(blob):
            interp = _elf_interp(blob)
            needed = _elf_dt_needed(blob) or []
            failures.append(
                f"cli: {binary} for {plat} is not statically linked "
                f"(PT_INTERP={interp or 'none'} "
                f"DT_NEEDED={','.join(needed) if needed else 'none'})"
            )
    if lane and not judged_any:
        failures.append(f"cli: {cli_dir}: nothing built (no archive present)")
    return failures


def check_homebrew_formula(formula: Path, sums: Path, version: str) -> list[str]:
    """The formula names the four Homebrew archives with SHA256SUMS digests."""
    failures: list[str] = []
    if not formula.is_file():
        return [f"homebrew: formula not found: {formula}"]
    if not sums.is_file():
        return [f"homebrew: SHA256SUMS not found: {sums}"]
    try:
        entries = parse_sums_file(sums)
    except OSError as e:
        return [f"homebrew: unreadable SHA256SUMS: {e}"]
    try:
        text = formula.read_text()
    except OSError as e:
        return [f"homebrew: unreadable formula: {e}"]
    triple_of = {plat: triple for plat, triple, _ in CLI_PLATFORMS}
    for plat in HOMEBREW_PLATFORMS:
        archive = cli_archive_name(version, triple_of[plat], "tar.gz")
        if archive not in entries:
            failures.append(
                f"homebrew: SHA256SUMS misses platform {plat} ({archive})"
            )
            continue
        if archive not in text:
            failures.append(
                f"homebrew: formula does not name the {plat} archive ({archive})"
            )
        if entries[archive].lower() not in text.lower():
            failures.append(
                f"homebrew: formula sha256 for {plat} does not match SHA256SUMS "
                f"({entries[archive][:16]}...)"
            )
    if 'pdfluent", "--version"' not in text and "pdfluent', '--version'" not in text:
        failures.append("homebrew: formula has no `pdfluent --version` test")
    return failures


def _is_attestation_entry(entry: object) -> bool:
    """True when an index entry is a build attestation, not a platform.

    buildx appends attestation manifests (provenance, SBOM) to the image
    index with unknown/unknown where a platform would be. They describe
    the build, so the platform check skips them — and the provenance they
    carry stays in the image, which --provenance=false would strip.
    """
    if not isinstance(entry, dict):
        return False
    platform = entry.get("platform", {})
    if not isinstance(platform, dict):
        return False
    return platform.get("os") == "unknown" and platform.get("architecture") == "unknown"


def check_docker_manifest(manifest: Path) -> list[str]:
    """The manifest list holds exactly linux/amd64 + linux/arm64.

    `manifest` is the saved output of
    `docker buildx imagetools inspect ghcr.io/pdfluent/pdfluent:<v> --raw`
    (an OCI image index), so this check runs without a daemon.
    Attestation entries (unknown/unknown) are skipped: they describe the
    build, not a platform.
    """
    failures: list[str] = []
    if not manifest.is_file():
        return [f"docker: manifest file not found: {manifest}"]
    try:
        doc = json.loads(manifest.read_text())
    except (json.JSONDecodeError, OSError) as e:
        return [f"docker: unreadable manifest {manifest}: {e}"]
    entries = doc.get("manifests")
    if not isinstance(entries, list):
        return [f"docker: {manifest.name} is not an image index (no manifests list)"]
    seen = set()
    for entry in entries:
        if _is_attestation_entry(entry):
            continue
        platform = entry.get("platform", {}) if isinstance(entry, dict) else {}
        seen.add((platform.get("os"), platform.get("architecture")))
    want = set(DOCKER_PLATFORMS)
    for os_name, arch in sorted(want - seen):
        failures.append(f"docker: manifest list misses {os_name}/{arch}")
    for os_name, arch in sorted(seen - want):
        failures.append(f"docker: manifest list carries unexpected {os_name}/{arch}")
    return failures


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Artefact gate: every built artefact covers every platform."
    )
    ap.add_argument("--jar", type=Path, help="Maven jar to check")
    ap.add_argument("--wheels", type=Path, help="wheel file or dir of wheels")
    ap.add_argument(
        "--extract-dir",
        type=Path,
        help="dir with the pdfluent-extract sdist and pure wheel",
    )
    ap.add_argument("--nupkg", type=Path, help="NuGet package to check")
    ap.add_argument("--node-dir", type=Path, help="crates/pdf-node checkout")
    ap.add_argument("--capi-dir", type=Path, help="dir of C API release assets")
    ap.add_argument("--cli-dir", type=Path, help="dir of CLI release archives")
    ap.add_argument("--cli-version", help="CLI release version (with --cli-dir)")
    ap.add_argument(
        "--min-archive-bytes",
        type=int,
        default=500_000,
        help="size floor per CLI archive",
    )
    ap.add_argument("--formula", type=Path, help="Homebrew formula to check")
    ap.add_argument("--sums", type=Path, help="SHA256SUMS (with --formula)")
    ap.add_argument(
        "--docker-manifest", type=Path, help="saved OCI image index to check"
    )
    ap.add_argument(
        "--min-native-bytes",
        type=int,
        default=1_000_000,
        help="smallest plausible native lib; each native must clear this x (1-margin)",
    )
    ap.add_argument("--margin", type=float, default=0.2)
    ap.add_argument(
        "--allow-missing",
        action="store_true",
        help="lane mode: report platforms with no artefact as not built "
        "instead of failing (the release never passes this flag)",
    )
    ap.add_argument(
        "--require-all",
        nargs="*",
        metavar="CHANNEL",
        default=None,
        help="fail naming any channel whose flag is absent. Bare means all "
        "six native channels (jar, wheels, nupkg, node, capi, cli); with "
        "names, only those (e.g. --require-all node for the npm lane, "
        "--require-all cli for the binaries lane). The release always "
        "passes it, so a dropped flag fails the run instead of silently "
        "skipping a channel.",
    )
    args = ap.parse_args(argv)

    # (channel name, flag spelling, flag value): the six native channels
    # --require-all can demand. The extract dists, the formula and the
    # Docker manifest are judged when passed, never demanded: they are
    # derived artefacts, not channels with a per-platform native.
    require_known: dict[str, object] = {
        "jar": args.jar,
        "wheels": args.wheels,
        "nupkg": args.nupkg,
        "node": args.node_dir,
        "capi": args.capi_dir,
        "cli": args.cli_dir,
    }
    require_missing: list[str] = []
    if args.require_all is not None:
        want = args.require_all or list(require_known)
        for name in want:
            if name not in require_known:
                ap.error(
                    f"--require-all: unknown channel {name!r} "
                    "(want one of jar, wheels, nupkg, node, capi, cli)"
                )
        flag_of = {
            "jar": "--jar",
            "wheels": "--wheels",
            "nupkg": "--nupkg",
            "node": "--node-dir",
            "capi": "--capi-dir",
            "cli": "--cli-dir",
        }
        require_missing = [
            f"require-all: the {name} channel is missing "
            f"({flag_of[name]} not passed)"
            for name in want
            if not require_known[name]
        ]

    if args.require_all is None and not any(
        [
            args.jar,
            args.wheels,
            args.nupkg,
            args.node_dir,
            args.capi_dir,
            args.cli_dir,
            args.extract_dir,
            args.formula,
            args.docker_manifest,
        ]
    ):
        ap.error(
            "nothing to check: pass at least one of --jar/--wheels/--nupkg/"
            "--node-dir/--capi-dir/--cli-dir/--extract-dir/--formula/"
            "--docker-manifest"
        )
    if args.cli_dir and not args.cli_version:
        ap.error("--cli-dir needs --cli-version")
    if args.formula and not args.sums:
        ap.error("--formula needs --sums")
    if args.formula and not args.cli_version:
        ap.error("--formula needs --cli-version")

    failures: list[str] = []
    not_built: list[str] = []
    # A missing channel fails the run but does not silence the channels
    # that ARE passed: the run reports everything it can judge at once.
    failures.extend(require_missing)
    lane = {"allow_missing": args.allow_missing, "not_built": not_built}
    if args.jar:
        failures.extend(
            check_maven_jar(args.jar, args.min_native_bytes, args.margin, **lane)
        )
    if args.wheels:
        failures.extend(
            check_python_wheels(args.wheels, args.min_native_bytes, args.margin, **lane)
        )
    if args.nupkg:
        failures.extend(
            check_nuget(args.nupkg, args.min_native_bytes, args.margin, **lane)
        )
    if args.node_dir:
        failures.extend(
            check_node_dir(args.node_dir, args.min_native_bytes, args.margin, **lane)
        )
    if args.capi_dir:
        failures.extend(
            check_capi_dir(args.capi_dir, args.min_native_bytes, args.margin, **lane)
        )
    if args.cli_dir:
        assert args.cli_version is not None
        failures.extend(
            check_cli_dir(args.cli_dir, args.cli_version, args.min_archive_bytes, **lane)
        )
    if args.extract_dir:
        failures.extend(check_extract_dir(args.extract_dir))
    if args.formula:
        assert args.sums is not None and args.cli_version is not None
        failures.extend(
            check_homebrew_formula(args.formula, args.sums, args.cli_version)
        )
    if args.docker_manifest:
        failures.extend(check_docker_manifest(args.docker_manifest))

    if failures:
        print("ARTIFACT-GATE: FAIL")
        for f in failures:
            print(f"  - {f}")
        for n in not_built:
            print(f"  - {n}")
        return 1
    for n in not_built:
        print(f"  - {n}")
    if not_built:
        print(
            f"ARTIFACT-GATE: PASS (partial: {len(not_built)} "
            f"platform{'s' if len(not_built) != 1 else ''} not built)"
        )
    else:
        print("ARTIFACT-GATE: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
