#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`fetch_ort_libs.py` stages verified libraries and refuses anything else.

Every case runs offline: fixture archives are built in a temporary directory
and handed over with `--archive-dir`, so no case touches the network. A
refusal case passes only when the exit code is nonzero AND the output holds
no staged file -- a script that reports an error but leaves the bytes would
still fail here.

The pin-table case holds `ort_pins.toml` to exactly the five
(platform, asset) pairs `OrtLibrary` in crates/pdfluent-cli/src/cmd_models.rs
names, in the same order: a sixth platform, a renamed asset, or a dropped one
fails the suite until both sides agree again.
"""
from __future__ import annotations

import hashlib
import io
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
FETCH = HERE / "fetch_ort_libs.py"
PINS = HERE / "ort_pins.toml"

MINIMUM_CASES = 12  # FLOOR

LIB_BYTES = b"fake-ort-library-bytes"
LICENSE_BYTES = b"MIT License\nCopyright (c) Microsoft Corporation\n"
NOTICES_BYTES = b"Third-party notices fixture\n"

# (platform, asset) exactly as `ORT_LIBRARIES` in cmd_models.rs lists them.
EXPECTED_PLATFORMS = [
    ("macos-arm64", "onnxruntime-macos-arm64-1.23.2.dylib"),
    ("macos-x86_64", "onnxruntime-macos-x86_64-1.23.2.dylib"),
    ("linux-x86_64", "onnxruntime-linux-x86_64-1.23.2.so"),
    ("linux-aarch64", "onnxruntime-linux-arm64-1.23.2.so"),
    ("windows-x86_64", "onnxruntime-windows-x86_64-1.23.2.dll"),
]


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _make_tgz(
    path: pathlib.Path,
    root: str,
    member: str,
    lib: bytes = LIB_BYTES,
    prefix_dot_slash: bool = False,
    symlink_target: str | None = None,
) -> None:
    def name(p: str) -> str:
        return f"./{p}" if prefix_dot_slash else p

    with tarfile.open(path, "w:gz") as tar:
        for arc, data in (
            (f"{root}/LICENSE", LICENSE_BYTES),
            (f"{root}/ThirdPartyNotices.txt", NOTICES_BYTES),
        ):
            info = tarfile.TarInfo(name(arc))
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        info = tarfile.TarInfo(name(member))
        if symlink_target is not None:
            info.type = tarfile.SYMTYPE
            info.linkname = symlink_target
            tar.addfile(info)
        else:
            info.size = len(lib)
            tar.addfile(info, io.BytesIO(lib))


def _make_zip(
    path: pathlib.Path, root: str, member: str, lib: bytes = LIB_BYTES
) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(f"{root}/LICENSE", LICENSE_BYTES)
        archive.writestr(f"{root}/ThirdPartyNotices.txt", NOTICES_BYTES)
        archive.writestr(member, lib)


def _pins_text(
    platform: str,
    archive_name: str,
    archive: pathlib.Path,
    member: str,
    asset: str,
    lib: bytes = LIB_BYTES,
    license_bytes: bytes = LICENSE_BYTES,
    notices_bytes: bytes = NOTICES_BYTES,
    **overrides: object,
) -> str:
    fields: dict[str, object] = {
        "archive_url": f"https://example.invalid/ort/{archive_name}",
        "archive_sha256": _sha(archive.read_bytes()),
        "archive_size": archive.stat().st_size,
        "member": member,
        "asset": asset,
        "lib_sha256": _sha(lib),
        "lib_size": len(lib),
        "license_sha256": _sha(license_bytes),
        "notices_sha256": _sha(notices_bytes),
    }
    fields.update(overrides)
    lines = ['ort_version = "1.23.2"', "", f"[{platform}]"]
    for key, value in fields.items():
        rendered = f'"{value}"' if isinstance(value, str) else str(value)
        lines.append(f"{key} = {rendered}")
    return "\n".join(lines) + "\n"


def _run(pins: pathlib.Path, out: pathlib.Path, archives: pathlib.Path) -> "subprocess.CompletedProcess[str]":
    return subprocess.run(
        [
            sys.executable,
            str(FETCH),
            "--pins",
            str(pins),
            "--out",
            str(out),
            "--archive-dir",
            str(archives),
        ],
        capture_output=True,
        text=True,
        timeout=120,
    )


class FetchOrtLibsTests(unittest.TestCase):
    def test_tgz_stages_the_renamed_library_with_its_licences(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.1.23.2.dylib"
            archive = archives / "ort.tgz"
            # The macOS archives prefix every member with `./`; the fixture
            # does the same so the normalisation is exercised, not assumed.
            _make_tgz(archive, "ort-root", member, prefix_dot_slash=True)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("macos-arm64", archive.name, archive, member, "ort.dylib")
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual((out / "ort.dylib").read_bytes(), LIB_BYTES)
            self.assertEqual((out / "ort.dylib").stat().st_mode & 0o777, 0o644)
            self.assertEqual((out / "ort.dylib.LICENSE").read_bytes(), LICENSE_BYTES)
            self.assertEqual(
                (out / "ort.dylib.ThirdPartyNotices.txt").read_bytes(), NOTICES_BYTES
            )

    def test_zip_stages_the_renamed_library_with_its_licences(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/ort.dll"
            archive = archives / "ort.zip"
            _make_zip(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("windows-x86_64", archive.name, archive, member, "ort.dll")
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertEqual((out / "ort.dll").read_bytes(), LIB_BYTES)
            self.assertEqual((out / "ort.dll.LICENSE").read_bytes(), LICENSE_BYTES)

    def test_mismatching_archive_sha_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text(
                    "linux-x86_64",
                    archive.name,
                    archive,
                    member,
                    "ort.so",
                    archive_sha256="0" * 64,
                )
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("refusing", proc.stderr)
            self.assertFalse((out / "ort.so").exists())

    def test_mismatching_archive_size_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text(
                    "linux-x86_64",
                    archive.name,
                    archive,
                    member,
                    "ort.so",
                    archive_size=archive.stat().st_size + 1,
                )
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("refusing", proc.stderr)
            self.assertFalse((out / "ort.so").exists())

    def test_mismatching_library_sha_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            # Same length as the pinned bytes: only the digest differs, so
            # the size check cannot mask a dead digest check.
            tampered = b"T" * len(LIB_BYTES)
            assert _sha(tampered) != _sha(LIB_BYTES)
            _make_tgz(archive, "ort-root", member, lib=tampered)
            pins = root / "pins.toml"
            # The pin still names the expected bytes; the archive carries
            # something else, so the stage must refuse.
            pins.write_text(
                _pins_text("linux-x86_64", archive.name, archive, member, "ort.so")
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("refusing", proc.stderr)
            self.assertFalse((out / "ort.so").exists())

    def test_missing_member_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", "ort-root/lib/other.so")
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text(
                    "linux-x86_64",
                    archive.name,
                    archive,
                    "ort-root/lib/libort.so.1.23.2",
                    "ort.so",
                )
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("no member", proc.stderr)
            self.assertFalse((out / "ort.so").exists())

    def test_symlink_member_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member, symlink_target="/etc/passwd")
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("linux-x86_64", archive.name, archive, member, "ort.so")
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("not a regular file", proc.stderr)
            self.assertFalse((out / "ort.so").exists())

    def test_unexpected_licence_text_is_refused_and_stages_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text(
                    "linux-x86_64",
                    archive.name,
                    archive,
                    member,
                    "ort.so",
                    license_sha256="0" * 64,
                )
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("refusing", proc.stderr)
            # The library verified but the notices did not: the whole
            # platform is refused, including the file that passed.
            self.assertFalse((out / "ort.so").exists())

    def test_unknown_platform_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("linux-x86_64", archive.name, archive, member, "ort.so")
            )
            out = root / "out"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(FETCH),
                    "--pins",
                    str(pins),
                    "--out",
                    str(out),
                    "--archive-dir",
                    str(archives),
                    "--platform",
                    "plan9-amd64",
                ],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("unknown platform", proc.stderr)

    def test_missing_archive_file_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("linux-x86_64", "absent.tgz", archive, member, "ort.so")
            )
            out = root / "out"
            proc = _run(pins, out, archives)
            self.assertNotEqual(proc.returncode, 0)
            self.assertFalse((out / "ort.so").exists())

    def test_platform_selection_stages_only_what_was_asked(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            archives = root / "archives"
            archives.mkdir()
            member = "ort-root/lib/libort.so.1.23.2"
            archive = archives / "ort.tgz"
            _make_tgz(archive, "ort-root", member)
            pins = root / "pins.toml"
            pins.write_text(
                _pins_text("linux-x86_64", archive.name, archive, member, "ort.so")
            )
            out = root / "out"
            proc = subprocess.run(
                [
                    sys.executable,
                    str(FETCH),
                    "--pins",
                    str(pins),
                    "--out",
                    str(out),
                    "--archive-dir",
                    str(archives),
                    "--platform",
                    "linux-x86_64",
                ],
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertTrue((out / "ort.so").is_file())
            self.assertIn("ok linux-x86_64 ort.so", proc.stdout)

    def test_pin_table_covers_exactly_the_five_cmd_models_platforms(self) -> None:
        pins = tomllib.loads(PINS.read_text())
        self.assertEqual(pins.get("ort_version"), "1.23.2")
        tables = [(name, entry) for name, entry in pins.items() if isinstance(entry, dict)]
        self.assertEqual(
            [(name, entry["asset"]) for name, entry in tables], EXPECTED_PLATFORMS
        )
        for name, entry in tables:
            for key in (
                "archive_sha256",
                "lib_sha256",
                "license_sha256",
                "notices_sha256",
            ):
                digest = entry[key]
                self.assertRegex(
                    digest, r"\A[0-9a-f]{64}\Z", f"{name}.{key} is not a sha256"
                )
                self.assertNotEqual(digest, "0" * 64, f"{name}.{key} is a placeholder")
            for key in ("archive_size", "lib_size"):
                self.assertIsInstance(entry[key], int, f"{name}.{key} is not a size")
                self.assertGreater(entry[key], 0, f"{name}.{key} is not positive")
            self.assertTrue(
                entry["archive_url"].startswith(
                    "https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/"
                ),
                f"{name}.archive_url is not an official v1.23.2 release asset",
            )
            root, _, rest = entry["member"].partition("/")
            self.assertIn(
                "1.23.2", root, f"{name}.member names no versioned archive root"
            )
            self.assertTrue(
                rest.startswith("lib/"),
                f"{name}.member is not under the archive's lib/",
            )


if __name__ == "__main__":
    cases = unittest.TestLoader().loadTestsFromTestCase(FetchOrtLibsTests).countTestCases()
    if cases < MINIMUM_CASES:  # FLOOR
        print(
            f"[test-fetch-ort-libs] FATAL: {cases} cases, floor is {MINIMUM_CASES}. "
            "A suite that lost its cases must not report green.",
            file=sys.stderr,
        )
        sys.exit(1)
    unittest.main(verbosity=2)
