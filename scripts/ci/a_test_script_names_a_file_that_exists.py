#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""An npm script may not name a test file that is not there (#515).

WHAT BREAKS WITHOUT THIS

`crates/pdf-node/package.json` ran three commands under `test`:

    jest tests/ --testPathIgnorePatterns tests/license
     && jest tests/license.test.js
     && jest tests/license_signed_payload.test.js

Both named files were deleted with the licence machinery in #226. `jest` exits
1 when a path matches nothing, so `npm test` in the published Node package had
failed on every invocation since -- and the first two thirds of that line, the
part that actually tests the binding, ran and passed before the failure. The
output looks like a test suite reporting a problem with the code. It is a
command naming a file that was removed four days earlier.

Nothing noticed for those four days, because the workflow that ran `npm test`
fired on tags only (retired in c46fc77f8), and a retroactive review found it
rather than anything that runs.

WHAT IS CHECKED

Every `scripts` entry of every tracked `package.json`, for path-shaped
arguments that point into the tree: a token containing `/` or ending in a
known test extension. Each must exist, as a file or a directory.

  * flags are not paths -- `--testPathIgnorePatterns tests/license` names a
    PATTERN, not a file, and is skipped along with the flag's value;
  * bare words are not paths. `jest`, `build`, `--coverage` name no file, and
    a guard that demanded they exist would be unusable;
  * a glob is checked by its fixed prefix -- `tests/*.spec.js` must at least
    have a `tests/`.

FLOOR, because a scan that reads nothing approves everything

At least two package.json files with a `scripts` table must be found, and at
least one path checked. The repository has more; fewer means the walk lost its
glob rather than found a clean tree.

Usage:
    python3 scripts/ci/a_test_script_names_a_file_that_exists.py

Exit codes:
    0  every path an npm script names is in the tree
    1  one is not, or the walk found less than the floor
"""
from __future__ import annotations

import json
import os
import pathlib
import re
import shlex
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]


def _sealed() -> dict[str, str]:
    """The caller's environment without the GIT_* variables.

    A pre-push hook exports GIT_DIR and GIT_WORK_TREE pointing at the real
    repository, and git obeys those over the directory it is given -- so a
    guard that inherits them lists the files of a different checkout than the
    one it is judging, and passes by hand while failing in the gate. On
    25-08-2026 that trap set `core.bare = true` on the real repository.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

MIN_MANIFESTS = 2
MIN_PATHS = 1

# Flags whose VALUE is a pattern or a name, never a path in this tree.
VALUE_IS_NOT_A_PATH = {
    "--testPathIgnorePatterns", "--testPathPattern", "--testNamePattern",
    "--reporters", "--runner", "--config", "-t", "--selectProjects",
}

TEST_SUFFIXES = (".js", ".mjs", ".cjs", ".ts", ".tsx", ".json")


def manifests() -> list[pathlib.Path]:
    listed = subprocess.run(["git", "ls-files", "*package.json"], cwd=REPO,
                            capture_output=True, text=True, check=True,
                            env=_sealed()).stdout
    return [REPO / line for line in listed.splitlines()
            if line and "node_modules/" not in line]


def looks_like_a_path(token: str) -> bool:
    if token.startswith("-"):
        return False
    if "/" in token:
        return True
    return token.endswith(TEST_SUFFIXES)


def paths_in(command: str) -> list[str]:
    """Path-shaped arguments, with each flag's non-path value skipped."""
    try:
        tokens = shlex.split(command)
    except ValueError:
        return []  # an unparsable line is not this guard's finding
    found: list[str] = []
    skip_next = False
    for token in tokens:
        if skip_next:
            skip_next = False
            continue
        if token in VALUE_IS_NOT_A_PATH:
            skip_next = True
            continue
        if token.startswith("--") and "=" in token:
            continue
        if looks_like_a_path(token):
            found.append(token)
    return found


def fixed_prefix(path: str) -> str:
    """A glob checked by the part before its first wildcard."""
    cut = re.split(r"[*?\[]", path, maxsplit=1)[0]
    return cut if cut.endswith("/") else str(pathlib.PurePosixPath(cut).parent) \
        if any(ch in path for ch in "*?[") else cut


def judge(root: pathlib.Path, files: list[pathlib.Path]) -> tuple[list[str], int, int]:
    findings: list[str] = []
    manifests_read = 0
    checked = 0
    for manifest in files:
        try:
            scripts = json.loads(manifest.read_text(encoding="utf-8")).get("scripts") or {}
        except (OSError, json.JSONDecodeError) as why:
            findings.append(f"{manifest}: could not be read: {why}")
            continue
        if not scripts:
            continue
        manifests_read += 1
        base = manifest.parent
        for name, command in scripts.items():
            for token in paths_in(str(command)):
                target = fixed_prefix(token)
                if not target or target in (".", "/"):
                    continue
                checked += 1
                if not (base / target).exists():
                    findings.append(
                        f"{manifest.relative_to(root)}: script `{name}` names "
                        f"`{token}`, which is not in the tree. A runner that is "
                        f"given a path matching nothing exits non-zero, so this "
                        f"script cannot succeed")
    return findings, manifests_read, checked


def main() -> int:
    findings, manifests_read, checked = judge(REPO, manifests())

    if manifests_read < MIN_MANIFESTS or checked < MIN_PATHS:
        print(f"[npm-script-paths] FAIL: read {manifests_read} manifest(s) with "
              f"scripts and checked {checked} path(s); floors are "
              f"{MIN_MANIFESTS} and {MIN_PATHS}. An empty scan approves "
              f"everything.", file=sys.stderr)
        return 1

    if findings:
        print("[npm-script-paths] FAIL: an npm script names a file that is not there",
              file=sys.stderr)
        for line in findings:
            print(f"  - {line}", file=sys.stderr)
        return 1

    print(f"[npm-script-paths] OK: {checked} path(s) across {manifests_read} "
          f"package.json script table(s); every one is in the tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
