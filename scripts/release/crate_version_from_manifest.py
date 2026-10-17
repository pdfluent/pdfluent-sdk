#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Print the version of the workspace package at a manifest path.

wasm.yml used to resolve the wasm crate's version by a hard-coded package
name (`pdfluent-xfa-wasm`). The crate was renamed to `xfa-wasm`, the lookup
matched nothing, and the step died with `IndexError: list index out of
range` -- after a green 17-minute wasm-pack build (run 37451390781).

A rename must not break the lookup again, so the caller names the manifest
(`crates/xfa-wasm/Cargo.toml`) instead of the package. This script matches
`cargo metadata --no-deps` on `manifest_path` and prints the version. When
no package matches it exits 1 naming the manifest -- never an IndexError.

Usage:
    python3 scripts/release/crate_version_from_manifest.py <manifest-path>
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(
            f"usage: {pathlib.Path(argv[0]).name} <manifest-path>",
            file=sys.stderr,
        )
        return 2
    want = argv[1]
    try:
        proc = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            capture_output=True,
            text=True,
            timeout=120,
        )
    except OSError as exc:
        print(f"error: cargo metadata could not run: {exc}", file=sys.stderr)
        return 1
    if proc.returncode != 0:
        print(
            f"error: cargo metadata failed: {proc.stderr.strip()[-500:]}",
            file=sys.stderr,
        )
        return 1
    try:
        packages = json.loads(proc.stdout)["packages"]
    except (ValueError, KeyError) as exc:
        print(
            f"error: cargo metadata output is not the expected JSON: {exc}",
            file=sys.stderr,
        )
        return 1
    # Cargo reports absolute manifest paths; the caller passes a tree-relative
    # one. The resolved absolute is the match; the relative suffix is the
    # fallback, so a symlinked checkout root cannot miss it.
    resolved = str(pathlib.Path(want).resolve())
    suffix = "/" + want.replace("\\", "/").lstrip("./")
    for package in packages:
        candidate = str(package.get("manifest_path", "")).replace("\\", "/")
        if candidate == resolved or candidate.endswith(suffix):
            print(package["version"])
            return 0
    print(
        f"error: no workspace package has manifest {want!r} "
        f"(cargo metadata --no-deps lists {len(packages)} packages)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
