#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No string literal that ships reads as a build host's path.

binary_host_paths.py reads the built natives, and that is the check that
counts. It runs late, though, and per platform: a literal that only
compiles on one target is only seen by that target's leg. In 1.1.0 an
error message in the CLI's file-URL parser spelled out an example drive
path ("file:///C:/dir/"). The code sat behind a Windows-only branch, so the
macOS and Linux builds eliminated it and passed, and the Windows CLI build
failed the binary scan.

This reads the Rust sources instead, which hold every target's literals at
once. It extracts each string literal from the src/ tree of the crates it
is given (by default every published crate), skips what is under #[cfg(test)] (that never ships), decodes the
escapes the way the compiler does, and asks binary_host_paths.py's own
host_path_strings() about each literal. The rules are therefore the binary
scan's rules, without a second copy that could drift.

Each literal is judged on its own. A binary's string table can run
neighbouring strings together, but pairing every literal with the next one
in the source flagged ordinary PDF tokens ("/f/lzw" followed by
"/filter /lzw" reads as an MSYS path), and source order is not table order
anyway. The case this exists for is a literal that is a path by itself.

Usage: source_literals_name_no_host_path.py [CRATE_DIR ...]
Without arguments it reads every published workspace crate.
Exit 0 when no literal matches, 1 when one does (each hit is printed with
its file and line), 2 on usage.
"""
from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]

_spec = importlib.util.spec_from_file_location("binary_host_paths", HERE / "binary_host_paths.py")
assert _spec and _spec.loader
_bhp = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_bhp)
host_path_strings = _bhp.host_path_strings



def published_crates() -> list[Path]:
    """Every workspace crate with a src/ tree that is not `publish = false`:
    the CLI and the engine, and every library a native links."""
    out = []
    for manifest in sorted((REPO / "crates").glob("*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        if re.search(r"(?m)^publish\s*=\s*false", text) or not (manifest.parent / "src").is_dir():
            continue
        out.append(manifest.parent)
    return out


_CHAR = re.compile(r"'(?:\\u\{[0-9A-Fa-f]{1,6}\}|\\x[0-9A-Fa-f]{2}|\\.|[^\\'\n])'")
_RAW = re.compile(r'b?r(#*)"')
_ESC = re.compile(r"\\(u\{[0-9A-Fa-f]{1,6}\}|x[0-9A-Fa-f]{2}|\n[ \t\r\n]*|.)", re.S)
_SIMPLE = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", '"': '"', "'": "'"}


def unescape(body: str) -> str:
    """A normal (non-raw) Rust string literal's body as the compiler stores it."""

    def one(m: re.Match[str]) -> str:
        e = m.group(1)
        if e.startswith("u{"):
            return chr(int(e[2:-1], 16))
        if e.startswith("x"):
            return chr(int(e[1:], 16))
        if e.startswith("\n"):
            return ""  # line continuation
        return _SIMPLE.get(e, "\\" + e)

    return _ESC.sub(one, body)


def literals(src: str) -> list[tuple[int, str]]:
    """(line, value) of every string literal outside #[cfg(test)] items."""
    out: list[tuple[int, str]] = []
    i, n = 0, len(src)
    depth = 0
    skip_until: int | None = None  # depth at which a cfg(test) item ends
    pending_cfg_test = False
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            nest, j = 1, i + 2
            while j < n and nest:
                if src.startswith("/*", j):
                    nest, j = nest + 1, j + 2
                elif src.startswith("*/", j):
                    nest, j = nest - 1, j + 2
                else:
                    j += 1
            i = j
            continue
        m = _RAW.match(src, i)
        if m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            close = '"' + m.group(1)
            j = src.find(close, m.end())
            j = n if j < 0 else j
            if skip_until is None:
                out.append((src.count("\n", 0, i) + 1, src[m.end():j]))
            i = j + len(close)
            continue
        if c == '"' or (c == "b" and src.startswith('b"', i)
                        and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_"))):
            start = i + (2 if c == "b" else 1)
            j = start
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            if skip_until is None:
                out.append((src.count("\n", 0, i) + 1, unescape(src[start:j])))
            i = j + 1
            continue
        if c == "'":
            cm = _CHAR.match(src, i)
            i = cm.end() if cm else i + 1  # else a lifetime
            continue
        if src.startswith("#[cfg(test)]", i):
            pending_cfg_test = True
            i += len("#[cfg(test)]")
            continue
        if c == "{":
            if pending_cfg_test and skip_until is None:
                skip_until = depth
            pending_cfg_test = False
            depth += 1
        elif c == "}":
            depth -= 1
            if skip_until is not None and depth == skip_until:
                skip_until = None
        elif c == ";" and pending_cfg_test:
            pending_cfg_test = False  # `#[cfg(test)] use ...;`
        i += 1
    return out


def hits_in(path: Path) -> list[str]:
    found: list[str] = []
    lits = literals(path.read_text(encoding="utf-8", errors="replace"))
    for line, value in lits:
        if host_path_strings(value.encode("utf-8", "replace")):
            found.append(f"{path}:{line}: {value!r}")
    return found


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if any(a in ("-h", "--help") for a in args):
        print(__doc__)
        return 0
    crates = [Path(a) for a in args] or published_crates()
    if not crates:
        print("source-literals: no published crate found; nothing was checked", file=sys.stderr)
        return 2
    hits: list[str] = []
    files = 0
    for crate in crates:
        src = crate / "src"
        if not src.is_dir():
            print(f"source-literals: {src} is not a directory", file=sys.stderr)
            return 2
        for rs in sorted(src.rglob("*.rs")):
            files += 1
            hits.extend(hits_in(rs))
    if files == 0:
        print("source-literals: no .rs files read; nothing was checked", file=sys.stderr)
        return 2
    if hits:
        print(f"source-literals: {len(hits)} shipped literal(s) read as a build host's path "
              "(binary_host_paths.py would fail the native that carries them):")
        for h in hits:
            print("    " + h)
        print("  Reword the text so it names no drive, home or runner path; an example "
              "path in an error message is enough to fail a release build.")
        return 1
    print(f"source-literals: OK: {files} file(s) in {len(crates)} crate(s), "
          "no shipped literal reads as a host path")
    return 0


if __name__ == "__main__":
    sys.exit(main())
