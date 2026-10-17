#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Print `<release> <pom version>` for maven_channel_guard.sh (#358).

Two lines of shell would be two more ways to read a pom. Both halves come from
`scripts/ci/every_registry_names_the_same_release.py`, which is the guard that
already holds the six manifests to one release -- so this check and that one
cannot disagree about what a release is, nor about which `<version>` element in
a pom is the project's own rather than a plugin's.

Exits non-zero when either half cannot be read, so the caller fails closed
instead of comparing an empty string with an empty string.
"""
from __future__ import annotations

import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
import every_registry_names_the_same_release as registry  # noqa: E402


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: maven_canonical_version_pair.py <path to pom.xml>", file=sys.stderr)
        return 2
    pom_path = pathlib.Path(argv[1])
    if not pom_path.is_file():
        print(f"no pom at {pom_path}", file=sys.stderr)
        return 1
    try:
        release = registry.workspace_version(pathlib.Path("."))
    except (OSError, ValueError) as why:
        print(f"cannot read the release version: {why}", file=sys.stderr)
        return 1
    pom = registry.pom_version(pom_path.read_text(encoding="utf-8"))
    if not pom:
        print(f"{pom_path} names no version of its own", file=sys.stderr)
        return 1
    print(f"{release} {pom}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
