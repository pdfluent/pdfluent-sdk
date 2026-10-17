#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No `napi build` line passes a flag the pinned CLI does not list.

The aarch64 musl leg of node-bindings.yml passed --cross-compile, a napi 3
flag, while the repo pins @napi-rs/cli 2.18 — so the leg died with
"Unknown Syntax Error: Unsupported option name" before compiling anything.
The allowed flags are parsed from the checked-in snapshot of
`napi build --help` from the pinned CLI (napi_build_help_2_18_0.txt,
byte-identical to `npx napi build --help` from @napi-rs/cli 2.18.0);
every `napi build` line in the workflows and the release lane is judged
against it.

Held only while the pinned CLI is 2.x: a 3.x pin renames flags again, and
this test must not forbid what it has not measured (same shape as the
NapiArgsTests in test_build_all_platforms.py).
"""
from __future__ import annotations

import json
import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"
RELEASE = REPO / "scripts" / "release"
NODE_PKG = REPO / "crates" / "pdf-node" / "package.json"
SNAPSHOT = RELEASE / "napi_build_help_2_18_0.txt"

# FLOOR: `napi build` lines judged. If the scan finds fewer, the search is
# broken and the tree is not clean; a scan that matches nothing approves
# everything, which is exactly the mistake this file exists to catch.
MINIMUM_BUILD_LINES = 4

# FLOOR: flags parsed from the snapshot. 2.18.0 lists 22 long options plus
# 2 short ones; fewer means the snapshot or the parser broke.
MINIMUM_FLAGS = 20


def _napi_cli_major() -> int:
    """The major version of the pinned @napi-rs/cli."""
    pkg = json.loads(NODE_PKG.read_text())
    pinned = pkg["devDependencies"]["@napi-rs/cli"]
    return int(re.search(r"(\d+)", pinned).group(1))


def _logical_lines(text: str) -> list[str]:
    """Shell lines with backslash continuations joined."""
    out = []
    buf = ""
    for line in text.splitlines():
        stripped = line.rstrip()
        if stripped.endswith("\\"):
            buf += stripped[:-1]
        else:
            out.append(buf + line)
            buf = ""
    if buf:
        out.append(buf)
    return out


def _allowed_flags() -> set[str]:
    """The long and short flags the snapshot's `napi build` lists."""
    text = SNAPSHOT.read_text()
    allowed = set(re.findall(r"--[A-Za-z0-9][\w-]*", text))
    # Short flags ride beside their long form (`--config,-c #0`) or alone
    # (`-p #0`); the prose mentions none beside `-w` in an example, which
    # is harmless to allow.
    allowed.update(
        "-" + short
        for short in re.findall(r"(?:^|[\s,])-([A-Za-z])(?=[\s,]|$)", text, re.M)
    )
    return allowed


def _build_lines() -> list[tuple[str, str]]:
    """Every `napi build` line in the workflows and the release lane."""
    sources = sorted(WORKFLOWS.glob("*.yml")) + sorted(RELEASE.glob("*.sh"))
    found = []
    for src in sources:
        for line in _logical_lines(src.read_text()):
            if "napi build" in line:
                found.append((src.name, line))
    return found


def _flag_tokens(line: str) -> list[str]:
    """The dash-led tokens of a build line (flags, short or long)."""
    return [tok for tok in line.split() if tok.startswith("-") and len(tok) > 1]


class NapiBuildFlagTests(unittest.TestCase):
    def test_snapshot_lists_the_flags_this_tree_uses(self):
        # The snapshot is the contract: it must be 2.18.0's help (the
        # `$ napi build` usage line), list --zig (the musl leg's route),
        # and parse to a full flag set. A stale or truncated snapshot
        # fails here rather than forbidding flags the CLI lists.
        if _napi_cli_major() != 2:
            self.skipTest("pinned napi CLI is not 2.x")
        text = SNAPSHOT.read_text()
        self.assertIn("$ napi build", text)
        self.assertIn("--zig", text)
        self.assertGreaterEqual(len(_allowed_flags()), MINIMUM_FLAGS)

    def test_no_napi_build_line_passes_an_unlisted_flag(self):
        # --cross-compile beside `napi build` is a napi 3 flag: the pinned
        # 2.18 CLI dies on it with "Unsupported option name" before it
        # compiles. Every dash-led token on every build line must be a
        # flag the snapshot lists.
        if _napi_cli_major() != 2:
            self.skipTest("pinned napi CLI is not 2.x")
        allowed = _allowed_flags()
        lines = _build_lines()
        self.assertGreaterEqual(len(lines), MINIMUM_BUILD_LINES)
        offenders = []
        for name, line in lines:
            for tok in _flag_tokens(line):
                flag = tok.split("=", 1)[0]
                if flag == "--":
                    continue
                if flag not in allowed:
                    offenders.append(f"{name}: {flag} in: {line.strip()}")
        self.assertEqual(offenders, [])


if __name__ == "__main__":
    unittest.main()
