#!/usr/bin/env python3
"""
fix_python_wheel_paths.py — strip private build-machine paths from a
maturin-built wheel before publish.

Root causes fixed:
  1. maturin auto-embeds a CycloneDX SBOM at
     `<dist-info>/sboms/<crate>.cyclonedx.json`. Its `bom-ref`/`ref`/`purl`
     fields carry the absolute on-disk `path+file:///...` and
     `?download_url=file:///...` of every path-dependency, leaking the
     build machine's home directory. maturin has no flag to disable or
     canonicalise this (only `--sbom-include` to add *more* files), so we
     patch it post-build, same canonicalisation convention already used
     for the workspace SBOM in scripts/release/sbom-generate.sh.
  2. A stray `*.dSYM` bundle sitting in the wheel's python-source tree
     (e.g. a leftover from a local `cargo build` outside maturin) gets
     swept into the wheel verbatim by maturin's file bundling. Its debug
     relocation map (`.yml`) embeds the absolute build path too. Dropped
     entirely — debug symbols are not required for a working wheel.

Usage:
    fix_python_wheel_paths.py <wheel1.whl> [wheel2.whl ...]

Exit codes:
    0  all wheels fixed and verified clean
    1  a wheel still leaks a private path after fixing, or has no RECORD
    2  usage error
"""
from __future__ import annotations

import base64
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

PATH_FILE_PAT = re.compile(r'path\+file:///[^#"\s]*?/([^/#"\s]+)#')
DOWNLOAD_URL_PAT = re.compile(r'download_url=file:///.*/([^/"\s]+)$')
LEAK_BYTES = (b"/Users/", b"/home/")


def canon_str(s: str) -> str:
    s = PATH_FILE_PAT.sub(r"path://\1#", s)
    s = DOWNLOAD_URL_PAT.sub(r"download_url=path://\1", s)
    return s


def canon(obj):
    if isinstance(obj, dict):
        return {k: canon(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [canon(v) for v in obj]
    if isinstance(obj, str):
        return canon_str(obj)
    return obj


def record_hash_entry(data: bytes) -> str:
    digest = hashlib.sha256(data).digest()
    return "sha256=" + base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")


def fix_wheel(path: Path) -> int:
    with zipfile.ZipFile(path, "r") as zin:
        contents: dict[str, tuple[zipfile.ZipInfo, bytes]] = {}
        dropped: list[str] = []
        for info in zin.infolist():
            name = info.filename
            if ".dSYM/" in name or name.endswith(".dSYM"):
                dropped.append(name)
                continue
            data = zin.read(info.filename)
            if re.search(r"\.dist-info/sboms/.*\.json$", name):
                doc = canon(json.loads(data))
                data = json.dumps(doc, indent=2).encode("utf-8") + b"\n"
            contents[name] = (info, data)

    record_name = next((n for n in contents if n.endswith(".dist-info/RECORD")), None)
    if record_name is None:
        print(f"[fix-wheel] ERROR: no RECORD entry found in {path}", file=sys.stderr)
        return 1

    record_lines = []
    for name in sorted(contents):
        if name == record_name:
            continue
        _, data = contents[name]
        record_lines.append(f"{name},{record_hash_entry(data)},{len(data)}")
    record_lines.append(f"{record_name},,")
    record_info, _ = contents[record_name]
    contents[record_name] = (record_info, ("\n".join(record_lines) + "\n").encode("utf-8"))

    tmp = path.with_suffix(".whl.tmp")
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
        for name in sorted(contents):
            info, data = contents[name]
            new_info = zipfile.ZipInfo(name, date_time=info.date_time)
            new_info.external_attr = info.external_attr
            new_info.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(new_info, data)
    tmp.replace(path)

    if dropped:
        print(f"[fix-wheel] {path.name}: dropped {len(dropped)} dSYM entr{'y' if len(dropped) == 1 else 'ies'}")

    leaks = []
    with zipfile.ZipFile(path, "r") as zcheck:
        for info in zcheck.infolist():
            data = zcheck.read(info.filename)
            if any(b in data for b in LEAK_BYTES):
                leaks.append(info.filename)
    if leaks:
        print(f"[fix-wheel] FAIL: {path.name} still leaks private paths in: {leaks}", file=sys.stderr)
        return 1
    print(f"[fix-wheel] OK: {path.name} clean ({len(contents)} files)")
    return 0


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: fix_python_wheel_paths.py <wheel1.whl> [wheel2.whl ...]", file=sys.stderr)
        return 2
    rc = 0
    for arg in sys.argv[1:]:
        rc |= fix_wheel(Path(arg))
    return rc


if __name__ == "__main__":
    sys.exit(main())
