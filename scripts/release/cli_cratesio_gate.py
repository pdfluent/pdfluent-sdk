#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""cli_cratesio_gate.py — the `cargo install pdfluent-cli` gate (T6 §3).

The precondition that bites: every dependency must be on crates.io at the
right version. `pdf-syntax`, `pdf-interpret` and `pdf-font` drift away from
what is published, and only a real `cargo publish` shows it. So this gate:

  1. reads the direct dependencies of `pdfluent-cli` from `cargo metadata`
     and fails naming every path dependency without a version requirement
     (normal and build kinds; dev-deps are exempt because `cargo publish`
     accepts a path-only dev-dependency, verified 01-10-2026, while a
     path-only normal dep fails manifest verification);
  2. fails when a `[patch]` section overrides any manifest in the closure
     (a path override masks exactly the fork drift this gate exists to see);
  3. runs `cargo package --list` and requires the licence/readme set;
  4. runs `cargo publish -p pdfluent-cli --dry-run` and reports its verdict.

Run on a clean checkout without path overrides; --require-clean refuses a
dirty tree. Until the release commit flips `publish = false`, step 4 fails
with that precondition named — red, not skipped, so the RC cannot go green
before the crate is publishable.

Usage:
  python3 scripts/release/cli_cratesio_gate.py [--metadata PATH] [--require-clean]
  python3 scripts/release/cli_cratesio_gate.py --metadata /tmp/metadata.json

Tests: scripts/release/test_cli_cratesio_gate.py.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path

CRATE = "pdfluent-cli"

PACKAGE_LIST_REQUIRED = ("Cargo.toml", "LICENSE", "README.md")

PUBLISH_NOT_YET_MARKERS = (
    "cannot be published",
    "`package.publish` must be set",
)


def root_package(metadata: dict, root_name: str) -> dict:
    for pkg in metadata.get("packages", []):
        if pkg.get("name") == root_name:
            return pkg
    raise ValueError(f"{root_name} not in cargo metadata")


def find_path_deps_without_version(
    metadata: dict, root_name: str = CRATE
) -> list[str]:
    """Direct path dependencies of the crate without a version requirement.

    Only the published crate's own manifest matters to `cargo publish`:
    transitive path deps resolve through the registry versions, and dev-deps
    are exempt. Returns failure strings; empty means publish-shaped.
    """
    pkg = root_package(metadata, root_name)
    failures: list[str] = []
    for dep in pkg.get("dependencies", []):
        if dep.get("path") is None:
            continue
        if (dep.get("kind") or "normal") not in ("normal", "build"):
            continue
        req = dep.get("req", "*")
        if req not in ("*", ""):
            continue
        kind = dep.get("kind") or "normal"
        failures.append(
            f"{CRATE}: depends on path-only "
            f"{dep.get('rename') or dep.get('name')} ({kind}, no version)"
        )
    return failures


def find_patch_sections(manifest_paths: list[Path]) -> list[str]:
    """`[patch...]` sections in the given manifests (path overrides)."""
    failures: list[str] = []
    for manifest in manifest_paths:
        try:
            doc = tomllib.loads(manifest.read_text())
        except (OSError, tomllib.TOMLDecodeError):
            continue
        patch = doc.get("patch")
        if isinstance(patch, dict) and patch:
            for source in sorted(patch):
                failures.append(
                    f"{CRATE}: {manifest} carries [patch.{source}] — "
                    "path overrides mask fork drift; run this gate without them"
                )
    return failures


def crate_manifest(metadata: dict, root_name: str = CRATE) -> Path | None:
    try:
        path = root_package(metadata, root_name).get("manifest_path")
    except ValueError:
        return None
    return Path(path) if path else None


def classify_publish_output(text: str) -> str:
    """`ok`, `not-yet-publishable` (publish = false), or `failed`."""
    if any(m in text for m in PUBLISH_NOT_YET_MARKERS):
        return "not-yet-publishable"
    return "failed"


def check_package_list(entries: list[str]) -> list[str]:
    failures = [
        f"{CRATE}: `cargo package --list` misses {name}"
        for name in PACKAGE_LIST_REQUIRED
        if name not in entries
    ]
    if not entries:
        failures.append(f"{CRATE}: `cargo package --list` is empty")
    return failures


def _run(cmd: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env["SCCACHE_DISABLE"] = "1"
    env.pop("RUSTC_WRAPPER", None)
    # A compiler cache that cannot run must not fail the gate: drop the
    # user-level rustc-wrapper, which only a --config flag overrides (an
    # empty CARGO_BUILD_RUSTC_WRAPPER does not clear it).
    if cmd[:1] == ["cargo"]:
        cmd = ["cargo", "--config", 'build.rustc-wrapper=""', *cmd[1:]]
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True,
                          timeout=600)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Gate `cargo install pdfluent-cli` before it can bite."
    )
    ap.add_argument("--metadata", type=Path,
                    help="saved `cargo metadata` JSON (else run cargo)")
    ap.add_argument("--require-clean", action="store_true",
                    help="refuse a dirty working tree")
    ap.add_argument("--root", type=Path,
                    help="workspace root (default: script's grandparent)")
    args = ap.parse_args(argv)

    root = args.root or Path(__file__).resolve().parents[2]
    failures: list[str] = []

    if args.require_clean:
        proc = subprocess.run(["git", "status", "--porcelain"], cwd=root,
                              capture_output=True, text=True, timeout=60)
        if proc.returncode != 0 or proc.stdout.strip():
            failures.append(
                f"{CRATE}: working tree is not clean (--require-clean)"
            )

    if args.metadata is not None:
        try:
            metadata = json.loads(args.metadata.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"CRATESIO-GATE: FAIL\n  - unreadable --metadata: {e}")
            return 1
    else:
        proc = _run(["cargo", "metadata", "--format-version", "1"], root)
        if proc.returncode != 0:
            print(f"CRATESIO-GATE: FAIL\n  - cargo metadata failed: "
                  f"{proc.stderr.strip()[:500]}")
            return 1
        metadata = json.loads(proc.stdout)

    try:
        failures.extend(find_path_deps_without_version(metadata))
    except ValueError as e:
        failures.append(f"{CRATE}: {e}")
    # [patch] is only honored from the workspace root and the crate itself;
    # a patch in a transitive manifest is ignored by cargo, so only these
    # two can mask drift.
    manifests = [root / "Cargo.toml"]
    own = crate_manifest(metadata)
    if own is not None and own not in manifests:
        manifests.append(own)
    failures.extend(find_patch_sections(manifests))

    proc = _run(["cargo", "package", "--list", "-p", CRATE,
                 "--allow-dirty"], root)
    if proc.returncode != 0:
        failures.append(
            f"{CRATE}: `cargo package --list` failed: "
            f"{proc.stderr.strip()[:500]}"
        )
    else:
        failures.extend(check_package_list(proc.stdout.splitlines()))

    proc = _run(["cargo", "publish", "-p", CRATE, "--dry-run",
                 "--allow-dirty"], root)
    combined = proc.stdout + proc.stderr
    if proc.returncode == 0:
        print("  ok: `cargo publish --dry-run` passed")
    elif classify_publish_output(combined) == "not-yet-publishable":
        failures.append(
            f"{CRATE}: crate carries `publish = false`; the release commit "
            "flips it before this gate can pass"
        )
    else:
        failures.append(
            f"{CRATE}: `cargo publish --dry-run` failed: "
            f"{combined.strip()[:800]}"
        )

    if failures:
        print("CRATESIO-GATE: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("CRATESIO-GATE: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
