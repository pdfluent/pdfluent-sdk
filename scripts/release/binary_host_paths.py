#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No shipped native names the machine that built it.

The 1.1.0 candidates embedded the CI runners' own paths: the linux wheel's
extension module carried 209 strings under the runner's checkout, its cargo
home and the toolchain, where 1.0.0 carried one. Panic locations, `file!()`
and C `__FILE__` all keep the path the compiler was handed unless the build
remaps it (scripts/release/release_rustflags.sh). Nothing failed, because
the only check (fix_python_wheel_paths.py) looked for /Users/ and /home/
alone and only inside wheels.

This reads the printable runs of every file the way `strings -a` does, both
ASCII and UTF-16LE (PE files keep paths that way), and counts the ones that
look like a build host's absolute path: homes and volumes (/Users/,
/Volumes/, /home/, /root/), runner and container roots (/srv/, /__w/,
/github/workspace/, /build/, /workspace/, /io/, /mnt/, /tmp/<x>/,
/private/var/ and the macOS per-user temp root under /var/ that it
aliases), toolchains outside the remap
(/opt/hostedtoolcache/, /opt/python/, /usr/local/cargo/, /usr/local/rustup/),
a Windows drive path in any slash style (C:/x/, C:\\x\\, the escaped
C:\\\\x\\\\), its MSYS spelling (/c/x/y), or a UNC/long-path prefix
(\\\\?\\). http(s) URLs are ignored; file:// URLs are not. The manylinux
gcc headers under /opt/rh/devtoolset-N/root/ are not a runner path.
Archives (.whl, .zip, .nupkg, .jar, .tgz, .tar.gz)
are opened and their native members (.so, .dylib, .dll, .pyd, .node) are
read instead. Every hit is printed with its file, so a failure names what
leaked; the remap targets (/src, /buildenv, /git, /registry, /rustc, and
/cargo, /rustup) never match.

Usage: binary_host_paths.py --max 0 FILE [FILE ...]
Exit 0 when every file is within --max hits, 1 when any is over or a named
file is missing or holds no native, 2 on usage.
"""
from __future__ import annotations

import argparse
import io
import re
import sys
import tarfile
import zipfile
from pathlib import Path

# Halves joined at runtime, so this file does not itself trip a path audit
# that greps the tree for these bytes.
_U, _V, _H = "/Use" "rs/", "/Volu" "mes/", "/ho" "me/"
_PT, _VF = "/private" "/tmp/", "/var" "/folders/"
_SEG = r"[A-Za-z0-9_.$~ -]{2,}"  # one directory name
_SEG1 = r"[A-Za-z0-9_.$~ -]+"  # one directory name, any length
_SEP = r"(?:/|\\{1,2})"  # /, \ or the escaped \\ of a string literal
_AT_ROOT = r"(?<![A-Za-z0-9_.$~-])"  # starts a path, not inside a name
HOST_PATH = re.compile(
    (
        r"(?:"
        # Roots that only ever name a machine: matched anywhere in a run,
        # because Rust string tables run strings together ("/lenIV/Vol...").
        + re.escape(_U) + r"|" + re.escape(_V) + r"|" + re.escape(_H)
        + r"|/srv/|/private/var/|" + re.escape(_PT) + r"|" + re.escape(_VF)
        + r"|/opt/hostedtoolcache/|/opt/python/"
        + r"|/usr/local/cargo/|/usr/local/rustup/"
        # Container and runner roots with common names: only where a path
        # starts, so ".../devtoolset-10/root/usr/include" (manylinux gcc
        # headers) and "crates/x/build/out" stay quiet.
        + r"|" + _AT_ROOT + r"(?:/root/|/github/workspace/|/__w/|/build/"
        + r"|/workspace/|/io/|/mnt/|/tmp/" + _SEG1 + r"/)"
        # A drive path needs a whole directory segment after the drive:
        # "C:\x\", "C:\\x\\" or "E:/ns-cargo-home/". A bare "V:\y" is a byte
        # pattern in compressed tables, not a path (seen in the 1.1.0 dylib).
        # GitHub's own Windows runners work in D:\a\<repo>, hence the
        # one-letter first segment when a second one follows.
        # No lookbehind: string tables glue a path onto the previous string
        # ("...repairC:\actions-runner\..."), which hid 2986 real hits in
        # older Windows builds from this pattern (scan2, 15-10, #693).
        + r"|[A-Za-z]:" + _SEP
        + r"(?:" + _SEG + _SEP + r"|" + _SEG1 + _SEP + _SEG1 + r")"
        # The MSYS spelling of a drive path Git Bash hands out, lower-case
        # drive and two components after it, the second a real name:
        # /c/x/yy, /e/a/b.rs. Lookup tables of single characters
        # ("/&/,/2/8/>/D/J/", in the 1.1.0 dylib) are not paths.
        + r"|(?<![A-Za-z0-9_.$~/:-])/[a-z]/" + _SEG1 + r"/" + _SEG
        # Plain UNC paths (\\server\share\...) are NOT matched here: the rule read
        # machine-code bytes in the macOS C API dylib as a path (16-10,
        # "\\\ll\lX\t0\", release-natives 38837596651). They are checked by the
        # mandatory independent end scan (release-evidence scan2) on the exact
        # bytes to be shipped, which blocks a release on any unclassified hit.
        # UNC and the Win32 long-path prefixes (\\?\, \\.\, //?/) when a
        # path follows them: "\\?\E:\w\a.rs", "\\.\pipe\x". The bare
        # prefixes, "\\?\UNC\" and "\\.\NUL" are constants of Rust's std
        # in every Windows binary and name no machine (1.1.0 win-x64, 15-10).
        + r"|\\\\[?.]\\(?:UNC\\)?[A-Za-z0-9_.$~ :-]+\\[A-Za-z0-9_.$~ -]"
        + r"|//[?.]/[A-Za-z0-9_.$~ :-]+/[A-Za-z0-9_.$~ -]"
        + r")"
    ).encode()
)
# Directories every Windows machine has, which the SDK itself reads at run
# time: not a build host. Exact spellings only, each with the source that
# names it; anything below them is still read as a path. C:\Windows\Fonts\
# is the system font directory in crates/pdf-manip/src/pdfa_fonts.rs. The
# NetworkService profile under C:\Windows is a runner home and stays a hit.
RUNTIME_SYSTEM_DIRS = re.compile(rb"(?i)C:(?:\\\\|\\)Windows(?:\\\\|\\)Fonts(?:\\\\|\\)(?![A-Za-z0-9_.$~ -]*(?:\\|/))")
# Generated binary constants whose bytes happen to read as a drive letter
# right before a remapped path. Each entry is one exact byte string, with the
# source that generates it; only those bytes are blanked before the strings
# are read, so the remapped path after them is judged on its own. Nothing
# else is exempt: a real "U:/src\..." or "U:\actions-runner\..." with any other
# bytes in front is still a hit.
#   4e 28 4e 8e e4 cd 55 3a: the little-endian u64 hash key 0x3a55cde48e4e284e
#   of phf::Map MAC_ROMAN_INVERSE (crates/pdf-interpret/src/font/generated/
#   mac_roman.rs:223, phf_map!, key derived deterministically from its 208
#   entries). On win-x64 the linker placed the panic-location path
#   "/src\crates\pdf-interpret\src\shading.rs" right after it, so its top
#   bytes "U:" read as a drive (release-natives 38809768457, 16-10; byte proof
#   in the release evidence, c3/diag-win-wheel/REPORT.md).
KNOWN_CONSTANTS = (
    bytes.fromhex("4e284e8ee4cd553a"),
)
# Web addresses are not host paths, whatever their path looks like; a
# file:// URL is kept, because it names a path on this machine.
WEB_URL = re.compile(rb"https?://[^\s\"'<>]*")
PRINTABLE_RUN = re.compile(rb"[\x20-\x7e\t]{4,}")
# PE files (.dll, .pyd, the Windows .node) keep many strings as UTF-16LE.
UTF16_RUN = re.compile(rb"(?:[\x20-\x7e\t]\x00){4,}")
NATIVE_SUFFIXES = (".so", ".dylib", ".dll", ".pyd", ".node")
ZIP_SUFFIXES = (".whl", ".zip", ".nupkg", ".jar")
TAR_SUFFIXES = (".tgz", ".tar.gz")


def is_native(name: str) -> bool:
    low = name.lower()
    return low.endswith(NATIVE_SUFFIXES) or ".so." in low.rsplit("/", 1)[-1]


def host_path_strings(data: bytes) -> list[str]:
    """Every printable run of `data` (ASCII, then UTF-16LE) holding a
    build-host path."""
    for const in KNOWN_CONSTANTS:
        data = data.replace(const, b"\x00" * len(const))
    runs = PRINTABLE_RUN.findall(data) + [
        r.decode("utf-16-le").encode("ascii", "replace") for r in UTF16_RUN.findall(data)
    ]
    return [
        run.decode("ascii", "replace")
        for run in runs
        if HOST_PATH.search(RUNTIME_SYSTEM_DIRS.sub(b" ", WEB_URL.sub(b" ", run)))
    ]


def snippet(run: str, width: int = 160) -> str:
    """The part of a printable run around its first host path."""
    m = HOST_PATH.search(WEB_URL.sub(b" ", run.encode("ascii", "replace")))
    start = max(0, (m.start() if m else 0) - 20)
    return ("..." if start else "") + run[start : start + width]


def natives_in(path: Path) -> list[tuple[str, bytes]]:
    """(label, bytes) for the file itself or each native inside an archive."""
    name = path.name.lower()
    if name.endswith(ZIP_SUFFIXES):
        with zipfile.ZipFile(path) as zf:
            return [
                (f"{path}!{m}", zf.read(m))
                for m in zf.namelist()
                if is_native(m)
            ]
    if name.endswith(TAR_SUFFIXES):
        out = []
        with tarfile.open(path) as tf:
            for m in tf.getmembers():
                if m.isfile() and is_native(m.name):
                    fh = tf.extractfile(m)
                    out.append((f"{path}!{m.name}", fh.read() if fh else b""))
        return out
    return [(str(path), path.read_bytes())]


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--max", type=int, required=True,
                    help="most host-path strings allowed per native")
    ap.add_argument("--show", type=int, default=20,
                    help="hits printed per native (default 20)")
    ap.add_argument("files", nargs="+", type=Path)
    args = ap.parse_args(argv)

    bad = False
    for path in args.files:
        if not path.is_file():
            print(f"FAIL {path}: no such file", file=sys.stderr)
            bad = True
            continue
        try:
            members = natives_in(path)
        except (zipfile.BadZipFile, tarfile.TarError) as exc:
            print(f"FAIL {path}: unreadable archive: {exc}", file=sys.stderr)
            bad = True
            continue
        if not members:
            # An archive with no native in it would pass vacuously.
            print(f"FAIL {path}: holds no native to check", file=sys.stderr)
            bad = True
            continue
        for label, data in members:
            hits = host_path_strings(data)
            verdict = "ok" if len(hits) <= args.max else "FAIL"
            print(f"{verdict} {label}: {len(hits)} host-path string(s), max {args.max}")
            if verdict == "FAIL":
                bad = True
                for hit in hits[: args.show]:
                    print(f"    {snippet(hit)}")
                if len(hits) > args.show:
                    print(f"    ... and {len(hits) - args.show} more")
    if bad:
        print("binary_host_paths: a native names its build host; the build "
              "missed scripts/release/release_rustflags.sh or a path it does "
              "not map", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
