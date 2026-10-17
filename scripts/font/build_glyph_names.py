#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Rebuild the glyph-name table from permissively licensed sources.

Reads the pinned Adobe Glyph List files (vendored under
``scripts/font/vendor/`` together with their BSD licence text) and writes
``crates/pdf-interpret/src/font/generated/glyph_names.rs``: a sorted static
forward table (glyph name to Unicode) plus a reverse table (code point to
preferred name).

Every input is hash-checked against its pinned sha256 before parsing, so a
corrupt or substituted download fails loudly instead of silently shipping a
wrong table. The output is deterministic: entries are sorted, and the run
carries no timestamps, so running the script twice yields byte-identical
files.

Usage:
    python3 scripts/font/build_glyph_names.py
    python3 scripts/font/build_glyph_names.py --fetch /tmp/agl-cache
    python3 scripts/font/build_glyph_names.py --source-dir DIR --output FILE
"""

import argparse
import hashlib
import sys
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

# Pinned upstream revision of github.com/adobe-type-tools/agl-aglfn.
PIN_COMMIT = "4036a9ca80a62f64f9de4f7321a9a045ad0ecfd6"
PIN_URL = f"https://raw.githubusercontent.com/adobe-type-tools/agl-aglfn/{PIN_COMMIT}"
LICENCE_SPDX = "BSD-3-Clause"

# sha256 of the exact bytes used, as recorded in
# crates/pdf-interpret/src/font/generated/PROVENANCE.md.
PINNED = {
    "glyphlist.txt": "a3b2f61ced9f3644cc0d4ecde5c59df34ca286c689d9484a43a710a81c466789",
    "aglfn.txt": "ed735a9ea58549b4cb2b8e804341548adb25aff43e81001ac7f3917247cacca1",
    "LICENSE.md": "58147d341e7a34aa2196862395a34d2fd95716c41d5ed26efb59ab0e12f92089",
}

DEFAULT_VENDOR = REPO_ROOT / "scripts/font/vendor/agl-aglfn-4036a9c"
DEFAULT_OUTPUT = (
    REPO_ROOT / "crates/pdf-interpret/src/font/generated/glyph_names.rs"
)


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch(sources, cache_dir):
    """Download the pinned files into ``cache_dir`` and hash-check them."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    for name in sources:
        target = cache_dir / name
        url = f"{PIN_URL}/{name}"
        print(f"fetching {url}")
        urllib.request.urlretrieve(url, target)
    check_hashes(cache_dir, sources)


def check_hashes(source_dir, names):
    for name in names:
        path = source_dir / name
        if not path.is_file():
            sys.exit(f"error: missing source file: {path}")
        actual = sha256_of(path)
        if actual != PINNED[name]:
            sys.exit(
                f"error: sha256 mismatch for {path}:\n"
                f"  want {PINNED[name]}\n"
                f"  got  {actual}"
            )


def parse_glyphlist(path):
    """Parse ``name;HEX[ HEX...]`` records, skipping comments and blanks."""
    table = {}
    with open(path, encoding="ascii") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, _, codes = line.partition(";")
            if not name or not codes:
                sys.exit(f"error: {path}:{lineno}: malformed record: {line!r}")
            if name in table:
                sys.exit(f"error: {path}:{lineno}: duplicate name: {name!r}")
            values = []
            for group in codes.split(" "):
                try:
                    value = int(group, 16)
                except ValueError:
                    sys.exit(
                        f"error: {path}:{lineno}: bad hex group: {group!r}"
                    )
                if value > 0x10FFFF or 0xD800 <= value <= 0xDFFF:
                    sys.exit(
                        f"error: {path}:{lineno}: not a scalar value: {group!r}"
                    )
                values.append(value)
            table[name] = tuple(values)
    return table


def parse_aglfn(path):
    """Parse ``HEX;name;description`` records, skipping comments and blanks."""
    table = {}
    with open(path, encoding="ascii") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            fields = line.split(";")
            if len(fields) != 3 or not fields[0] or not fields[1]:
                sys.exit(f"error: {path}:{lineno}: malformed record: {line!r}")
            table[fields[1]] = int(fields[0], 16)
    return table


def rust_escape_char(value):
    return f"\\u{{{value:04X}}}"


def emit_string(values):
    return '"' + "".join(rust_escape_char(v) for v in values) + '"'


def render(forward, reverse):
    lines = []
    lines.append("// Copyright (c) 2026 Innovation Trigger B.V.")
    lines.append("//")
    # pdf-interpret is a forked crate (Apache-2.0 OR MIT): our copyright rides
    # under the crate's licence, never under the proprietary header (#662).
    lines.append("// Licensed under the same terms as this crate: Apache-2.0 OR MIT.")
    lines.append("")
    lines.append("// Generated by scripts/font/build_glyph_names.py -- do not edit by hand.")
    lines.append("// Regenerate with: python3 scripts/font/build_glyph_names.py")
    lines.append("//")
    lines.append("// Sources (pinned; provenance in generated/PROVENANCE.md):")
    lines.append(f"//   glyphlist.txt ({LICENCE_SPDX})")
    lines.append(f"//     {PIN_URL}/glyphlist.txt")
    lines.append(f"//     sha256 {PINNED['glyphlist.txt']}")
    lines.append(f"//   aglfn.txt ({LICENCE_SPDX}, cross-checked against glyphlist.txt)")
    lines.append(f"//     {PIN_URL}/aglfn.txt")
    lines.append(f"//     sha256 {PINNED['aglfn.txt']}")
    lines.append(f"// Forward entries: {len(forward)}; reverse entries: {len(reverse)}.")
    lines.append("use phf::phf_map;")
    lines.append("")
    lines.append("pub(crate) fn get(name: &str) -> Option<&'static str> {")
    lines.append("    GLYPH_NAMES.get(name).copied()")
    lines.append("}")
    lines.append("")
    lines.append("pub(crate) fn get_reverse(unicode: char) -> Option<&'static str> {")
    lines.append("    REVERSE_GLYPH_NAMES.get(&unicode).copied()")
    lines.append("}")
    lines.append("")
    lines.append("static GLYPH_NAMES: phf::Map<&'static str, &'static str> = phf_map! {")
    for name in sorted(forward):
        lines.append(f'    "{name}" => {emit_string(forward[name])},')
    lines.append("};")
    lines.append("")
    lines.append("static REVERSE_GLYPH_NAMES: phf::Map<char, &'static str> = phf_map! {")
    for value in sorted(reverse):
        lines.append(f"    '{rust_escape_char(value)}' => \"{reverse[value]}\",")
    lines.append("};")
    lines.append("")
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-dir",
        type=Path,
        default=DEFAULT_VENDOR,
        help="directory holding the pinned glyphlist.txt and aglfn.txt",
    )
    parser.add_argument(
        "--output", type=Path, default=DEFAULT_OUTPUT, help="table to write"
    )
    parser.add_argument(
        "--fetch",
        type=Path,
        metavar="CACHE_DIR",
        default=None,
        help="download the pinned sources into CACHE_DIR first, then use it",
    )
    args = parser.parse_args(argv)

    source_dir = args.source_dir
    if args.fetch is not None:
        fetch(list(PINNED), args.fetch)
        source_dir = args.fetch

    check_hashes(source_dir, ["glyphlist.txt", "aglfn.txt"])

    forward = parse_glyphlist(source_dir / "glyphlist.txt")

    # AGLFN is the preferred-name list for new fonts. Every one of its names
    # already sits in the AGL with the same code point (checked below), so it
    # contributes no extra rows; it is kept pinned beside the AGL as the
    # cross-check that the table agrees with it.
    preferred = parse_aglfn(source_dir / "aglfn.txt")
    for name, value in preferred.items():
        if forward.get(name) != (value,):
            sys.exit(
                f"error: aglfn.txt disagrees with glyphlist.txt on {name!r}"
            )

    # Reverse lookup: one name per code point. Several names can share a
    # code (aliases); the byte-smallest wins so the choice is stable and
    # needs no extra data. Multi-code sequences have no single key and are
    # forward-only.
    reverse = {}
    for name, values in forward.items():
        if len(values) != 1:
            continue
        value = values[0]
        if value not in reverse or name < reverse[value]:
            reverse[value] = name

    args.output.write_text(render(forward, reverse), encoding="utf-8")
    print(
        f"wrote {args.output} "
        f"({len(forward)} forward, {len(reverse)} reverse entries)"
    )


if __name__ == "__main__":
    main()
