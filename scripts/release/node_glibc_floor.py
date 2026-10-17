#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No linux-gnu binary needs a glibc newer than the floor.

The 1.1.0 Node candidates for linux-gnu were linked against the build
runner's own glibc and needed GLIBC_2.39 (x86_64: pidfd_spawnp) and
GLIBC_2.43 (aarch64: atan2f). @pdfluent/node-linux-x64-gnu@1.0.0 needed
at most GLIBC_2.17, because 1.0.0 was built with zig against a 2.17
stub (docs/release/REGISTRY_1_0_0_RUNBOOK.md section 3.2, at v1.0.0).
A binding that needs a newer glibc than the user's does not load at all:
"version `GLIBC_2.39' not found".

This reads the version needs off the bytes -- the ELF .gnu.version_r
section (SHT_GNU_verneed) -- in pure Python, so it needs no objdump or
readelf on the runner. Every GLIBC_x.y a file asks for is compared with
--max; the highest one is printed per file, and every one above the floor
is named with the library that carries it.

A file with no GLIBC_ need at all fails too: a gnu binding that asks
nothing of glibc is not a gnu binding (a musl or host build under the
wrong name), and passing it would make this check vacuous.

Usage: node_glibc_floor.py --max 2.17 FILE [FILE ...]
Exit 0 when every file is within the floor, 1 when any is not, 2 on usage.
"""
from __future__ import annotations

import argparse
import re
import struct
import sys
from pathlib import Path

SHT_GNU_VERNEED = 0x6FFFFFFE
GLIBC_RE = re.compile(r"^GLIBC_(\d+(?:\.\d+)*)$")


class NotElf(ValueError):
    """The file is not an ELF object this reader understands."""


def _version(text: str) -> tuple[int, ...]:
    return tuple(int(part) for part in text.split("."))


def version_needs(data: bytes) -> list[tuple[str, str]]:
    """(library, version name) for every entry of .gnu.version_r."""
    if data[:4] != b"\x7fELF":
        raise NotElf("no ELF magic")
    ei_class, ei_data = data[4], data[5]
    if ei_class not in (1, 2) or ei_data not in (1, 2):
        raise NotElf(f"unknown ELF class/data {ei_class}/{ei_data}")
    end = "<" if ei_data == 1 else ">"
    is64 = ei_class == 2
    if is64:
        e_shoff = struct.unpack_from(end + "Q", data, 0x28)[0]
        e_shentsize, e_shnum = struct.unpack_from(end + "HH", data, 0x3A)
        sh_fmt = end + "IIQQQQIIQQ"
    else:
        e_shoff = struct.unpack_from(end + "I", data, 0x20)[0]
        e_shentsize, e_shnum = struct.unpack_from(end + "HH", data, 0x2E)
        sh_fmt = end + "IIIIIIIIII"
    if e_shoff == 0 or e_shnum == 0:
        raise NotElf("no section headers")
    if e_shoff + e_shentsize * e_shnum > len(data):
        raise NotElf("section headers run past the end of the file")

    sections = []
    for i in range(e_shnum):
        f = struct.unpack_from(sh_fmt, data, e_shoff + i * e_shentsize)
        # name, type, flags, addr, offset, size, link, info, addralign, entsize
        sections.append({"type": f[1], "offset": f[4], "size": f[5], "link": f[6], "info": f[7]})

    needs: list[tuple[str, str]] = []
    for sec in sections:
        if sec["type"] != SHT_GNU_VERNEED:
            continue
        if sec["link"] >= len(sections):
            raise NotElf("verneed string table index out of range")
        strtab = sections[sec["link"]]

        def string(off: int) -> str:
            start = strtab["offset"] + off
            stop = data.index(b"\0", start)
            return data[start:stop].decode("ascii", "replace")

        pos = sec["offset"]
        # sh_info is the number of Verneed entries.
        for _ in range(sec["info"]):
            _vn_version, vn_cnt, vn_file, vn_aux, vn_next = struct.unpack_from(end + "HHIII", data, pos)
            lib = string(vn_file)
            aux = pos + vn_aux
            for _ in range(vn_cnt):
                _hash, _flags, _other, vna_name, vna_next = struct.unpack_from(end + "IHHII", data, aux)
                needs.append((lib, string(vna_name)))
                if vna_next == 0:
                    break
                aux += vna_next
            if vn_next == 0:
                break
            pos += vn_next
    return needs


def check(path: Path, floor: tuple[int, ...]) -> tuple[bool, list[str]]:
    """(ok, report lines) for one file against the floor."""
    try:
        data = path.read_bytes()
    except OSError as err:
        return False, [f"FAIL {path}: cannot read: {err}"]
    try:
        needs = version_needs(data)
    except (NotElf, struct.error, ValueError) as err:
        return False, [f"FAIL {path}: not a readable ELF: {err}"]

    glibc = []
    bad = []
    for lib, name in needs:
        if not name.startswith("GLIBC_"):
            continue
        m = GLIBC_RE.match(name)
        if not m:
            # GLIBC_PRIVATE and friends: no stable version, never shippable.
            bad.append(((10**9,), f"  {name} from {lib}: not a released glibc version"))
            continue
        ver = _version(m.group(1))
        glibc.append(ver)
        if ver > floor:
            bad.append((ver, f"  {name} from {lib}"))
    floor_s = ".".join(map(str, floor))
    if not glibc and not bad:
        return False, [f"FAIL {path}: needs no GLIBC_ version at all; not a glibc-linked binary"]
    highest = ".".join(map(str, max(glibc))) if glibc else "none"
    if bad:
        return False, [f"FAIL {path}: needs glibc above {floor_s} (max GLIBC_{highest}):"] + [line for _, line in sorted(set(bad))]
    return True, [f"ok   {path}: max GLIBC_{highest} <= {floor_s}"]


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--max", required=True, help="highest GLIBC_ version allowed, e.g. 2.17")
    ap.add_argument("files", nargs="+", type=Path)
    args = ap.parse_args(argv)
    if not re.fullmatch(r"\d+(\.\d+)*", args.max):
        print(f"node_glibc_floor: --max {args.max!r} is not a version", file=sys.stderr)
        return 2
    floor = _version(args.max)
    all_ok = True
    for path in args.files:
        ok, lines = check(path, floor)
        all_ok &= ok
        print("\n".join(lines), file=sys.stdout if ok else sys.stderr)
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
