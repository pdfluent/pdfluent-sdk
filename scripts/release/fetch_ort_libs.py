#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Stage the pinned ONNX Runtime libraries for the models release.

Reads `ort_pins.toml`, downloads each platform's official archive, verifies
the archive digest, extracts the runtime library member, verifies the library
digest, and writes `<out>/<asset>` -- the file `pdfluent models install`
fetches. The archive's LICENSE (MIT) and ThirdPartyNotices.txt travel with
each library as `<asset>.LICENSE` and `<asset>.ThirdPartyNotices.txt`; the MIT
licence requires the notice to accompany the software, so a missing or
unexpected licence text refuses the platform like a digest mismatch does.

Refuses on ANY mismatch: wrong archive or library digest or size, a member
that is absent or not a regular file (a symlink at the member name stages
nothing), a licence text that differs from the pin. Nothing is left behind on
a refusal: outputs land by rename, and a failed platform removes its partial
files.

Usage:
    python3 scripts/release/fetch_ort_libs.py --out <dir>
    python3 scripts/release/fetch_ort_libs.py --out <dir> --platform linux-x86_64
    python3 scripts/release/fetch_ort_libs.py --out <dir> --archive-dir <dir>

`--archive-dir` reads `<basename of the archive URL>` from a local directory
instead of downloading (still fully verified): the offline seam for tests
and for re-staging without the network. Otherwise archives download into
`--cache-dir` (default `<out>/.ort-archive-cache`) and a cached copy whose
size and digest verify is reused.

Exit codes: 0 staged every requested platform; 1 anything was refused.
"""
from __future__ import annotations

import argparse
import hashlib
import pathlib
import shutil
import sys
import tarfile
import tempfile
import tomllib
import urllib.request
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_PINS = HERE / "ort_pins.toml"

LICENSE_NAME = "LICENSE"
NOTICES_NAME = "ThirdPartyNotices.txt"

# The Windows archive is ~78 MB; the timeout covers a slow mirror, not a
# fast one, and the digest decides afterwards regardless.
DOWNLOAD_TIMEOUT_SECONDS = 600
_CHUNK = 1024 * 1024


class Refusal(Exception):
    """A platform that must not be staged, with the reason it must not."""


def _sha256_of(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        while True:
            chunk = fh.read(_CHUNK)
            if not chunk:
                return digest.hexdigest()
            digest.update(chunk)


def _normalise_member(name: str) -> str:
    # The macOS archives store every member under a `./` prefix; the pin
    # table records the canonical path without it. Strip one leading `./`
    # on both sides of the comparison so the pin stays readable.
    while name.startswith("./"):
        name = name[2:]
    return name


def _require_keys(platform: str, entry: dict, keys: tuple[str, ...]) -> None:
    for key in keys:
        if key not in entry:
            raise Refusal(f"{platform}: pin table entry has no {key!r}")


def load_pins(path: pathlib.Path) -> tuple[str, list[tuple[str, dict]]]:
    """The ORT version and the per-platform pins, in file order."""
    try:
        pins = tomllib.loads(path.read_text())
    except OSError as exc:
        raise Refusal(f"cannot read {path}: {exc}") from exc
    except tomllib.TOMLDecodeError as exc:
        raise Refusal(f"cannot parse {path}: {exc}") from exc
    version = pins.get("ort_version")
    if not isinstance(version, str) or not version:
        raise Refusal(f"{path} names no ort_version")
    platforms = [
        (name, entry)
        for name, entry in pins.items()
        if isinstance(entry, dict)
    ]
    if not platforms:
        raise Refusal(f"{path} names no platforms")
    for name, entry in platforms:
        _require_keys(
            name,
            entry,
            (
                "archive_url",
                "archive_sha256",
                "archive_size",
                "member",
                "asset",
                "lib_sha256",
                "lib_size",
                "license_sha256",
                "notices_sha256",
            ),
        )
    return version, platforms


def _download(url: str, dest: pathlib.Path) -> None:
    tmp_fd, tmp_name = tempfile.mkstemp(
        prefix=dest.name + ".", suffix=".part", dir=str(dest.parent)
    )
    try:
        with open(tmp_fd, "wb") as fh:
            with urllib.request.urlopen(url, timeout=DOWNLOAD_TIMEOUT_SECONDS) as resp:
                shutil.copyfileobj(resp, fh, _CHUNK)
        shutil.move(tmp_name, dest)
    except Exception:
        pathlib.Path(tmp_name).unlink(missing_ok=True)
        raise


def _verify_archive(
    platform: str, archive: pathlib.Path, expected_sha256: str, expected_size: int
) -> None:
    size = archive.stat().st_size
    if size != expected_size:
        raise Refusal(
            f"{platform}: {archive.name} is {size} bytes, pin says "
            f"{expected_size}; refusing"
        )
    digest = _sha256_of(archive)
    if digest != expected_sha256:
        raise Refusal(
            f"{platform}: {archive.name} sha256 {digest}, pin says "
            f"{expected_sha256}; refusing"
        )


def obtain_archive(
    platform: str,
    url: str,
    expected_sha256: str,
    expected_size: int,
    cache_dir: pathlib.Path,
    archive_dir: pathlib.Path | None,
) -> pathlib.Path:
    """The verified archive file, downloaded or taken from `--archive-dir`."""
    name = url.rsplit("/", 1)[-1]
    if not name:
        raise Refusal(f"{platform}: archive URL has no file name: {url}")
    if archive_dir is not None:
        archive = archive_dir / name
        if not archive.is_file():
            raise Refusal(f"{platform}: no {name} in {archive_dir}")
        _verify_archive(platform, archive, expected_sha256, expected_size)
        return archive
    cache_dir.mkdir(parents=True, exist_ok=True)
    archive = cache_dir / name
    if archive.is_file():
        # A cached copy from an older pin refuses once, then is replaced: the
        # re-download is verified again before it is trusted.
        try:
            _verify_archive(platform, archive, expected_sha256, expected_size)
            return archive
        except Refusal:
            archive.unlink(missing_ok=True)
    try:
        _download(url, archive)
    except Exception as exc:
        archive.unlink(missing_ok=True)
        raise Refusal(f"{platform}: could not download {url}: {exc}") from exc
    _verify_archive(platform, archive, expected_sha256, expected_size)
    return archive


def _read_tar_member(archive: pathlib.Path, member: str) -> bytes:
    want = _normalise_member(member)
    try:
        tar = tarfile.open(archive, "r")
    except tarfile.TarError as exc:
        raise Refusal(f"{archive.name}: not a readable tar archive: {exc}") from exc
    with tar:
        for info in tar.getmembers():
            if _normalise_member(info.name) != want:
                continue
            if not info.isreg():
                raise Refusal(
                    f"{archive.name}: member {member} is not a regular file; refusing"
                )
            fh = tar.extractfile(info)
            if fh is None:
                raise Refusal(f"{archive.name}: cannot read member {member}; refusing")
            return fh.read()
    raise Refusal(f"{archive.name}: no member {member}; refusing")


def _read_zip_member(archive: pathlib.Path, member: str) -> bytes:
    want = _normalise_member(member)
    try:
        archive_zip = zipfile.ZipFile(archive, "r")
    except zipfile.BadZipFile as exc:
        raise Refusal(f"{archive.name}: not a readable zip archive: {exc}") from exc
    with archive_zip:
        for info in archive_zip.infolist():
            if _normalise_member(info.filename) != want:
                continue
            if info.is_dir():
                raise Refusal(
                    f"{archive.name}: member {member} is not a regular file; refusing"
                )
            return archive_zip.read(info.filename)
    raise Refusal(f"{archive.name}: no member {member}; refusing")


def read_member_bytes(archive: pathlib.Path, member: str) -> bytes:
    """The bytes of one archive member, never extracted to disk."""
    if archive.suffix == ".zip":
        return _read_zip_member(archive, member)
    return _read_tar_member(archive, member)


def _write_new(path: pathlib.Path, data: bytes) -> None:
    tmp_fd, tmp_name = tempfile.mkstemp(
        prefix=path.name + ".", suffix=".part", dir=str(path.parent)
    )
    try:
        with open(tmp_fd, "wb") as fh:
            fh.write(data)
        # mkstemp lands 0600; staged release assets are world-readable.
        pathlib.Path(tmp_name).chmod(0o644)
        shutil.move(tmp_name, path)
    except Exception:
        pathlib.Path(tmp_name).unlink(missing_ok=True)
        raise


def stage_platform(
    platform: str,
    entry: dict,
    out: pathlib.Path,
    cache_dir: pathlib.Path,
    archive_dir: pathlib.Path | None,
) -> pathlib.Path:
    """Stage one platform; returns the library path. Refuses on any mismatch."""
    archive = obtain_archive(
        platform,
        entry["archive_url"],
        entry["archive_sha256"],
        entry["archive_size"],
        cache_dir,
        archive_dir,
    )
    member = entry["member"]
    lib = read_member_bytes(archive, member)
    if len(lib) != entry["lib_size"]:
        raise Refusal(
            f"{platform}: {member} is {len(lib)} bytes, pin says "
            f"{entry['lib_size']}; refusing"
        )
    digest = hashlib.sha256(lib).hexdigest()
    if digest != entry["lib_sha256"]:
        raise Refusal(
            f"{platform}: {member} sha256 {digest}, pin says "
            f"{entry['lib_sha256']}; refusing"
        )
    # The licence texts sit beside the library's top directory: the first
    # path segment of the member is the archive root on all five archives.
    root = _normalise_member(member).split("/", 1)[0]
    staged: list[pathlib.Path] = []
    try:
        asset = out / entry["asset"]
        _write_new(asset, lib)
        staged.append(asset)
        for filename, key, suffix in (
            (LICENSE_NAME, "license_sha256", ".LICENSE"),
            (NOTICES_NAME, "notices_sha256", ".ThirdPartyNotices.txt"),
        ):
            text = read_member_bytes(archive, f"{root}/{filename}")
            text_digest = hashlib.sha256(text).hexdigest()
            if text_digest != entry[key]:
                raise Refusal(
                    f"{platform}: {filename} sha256 {text_digest}, pin says "
                    f"{entry[key]}; refusing"
                )
            sidecar = out / (entry["asset"] + suffix)
            _write_new(sidecar, text)
            staged.append(sidecar)
    except Exception:
        for path in staged:
            path.unlink(missing_ok=True)
        raise
    return asset


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="stage the pinned ONNX Runtime libraries for the models release"
    )
    parser.add_argument("--pins", default=str(DEFAULT_PINS), help="pin table (TOML)")
    parser.add_argument("--out", required=True, help="directory for the staged files")
    parser.add_argument(
        "--platform",
        action="append",
        default=[],
        help="stage only this platform (repeatable; default: all five)",
    )
    parser.add_argument(
        "--cache-dir",
        default=None,
        help="where downloads live (default: <out>/.ort-archive-cache)",
    )
    parser.add_argument(
        "--archive-dir",
        default=None,
        help="read archives from here instead of downloading (still verified)",
    )
    args = parser.parse_args(argv)

    out = pathlib.Path(args.out)
    cache_dir = (
        pathlib.Path(args.cache_dir) if args.cache_dir else out / ".ort-archive-cache"
    )
    archive_dir = pathlib.Path(args.archive_dir) if args.archive_dir else None

    try:
        version, platforms = load_pins(pathlib.Path(args.pins))
        wanted = set(args.platform)
        known = {name for name, _ in platforms}
        unknown = sorted(wanted - known)
        if unknown:
            raise Refusal(f"unknown platform(s): {', '.join(unknown)}")
        out.mkdir(parents=True, exist_ok=True)
        for name, entry in platforms:
            if wanted and name not in wanted:
                continue
            asset = stage_platform(name, entry, out, cache_dir, archive_dir)
            print(f"ok {name} {asset.name} (ORT {version})")
    except Refusal as exc:
        print(f"fetch_ort_libs: error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
