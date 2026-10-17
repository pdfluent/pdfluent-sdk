#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Refuse a 1.x tree that adds an exhaustive public enum since v1.0.0 (#574).

WHY

cargo-semver-checks compares against v1.0.0, so it cannot see this: an
exhaustive public enum that is NEW since v1.0.0 passes the gate when it
ships in 1.1.0, and its first new variant after that is a major break.
The rule (COS, 02-10) is that every public enum new since v1.0.0 is
#[non_exhaustive] before it ships.

WHAT IT DOES

For each crate the semver gate checks (the "published at v1.0.0" list),
it builds rustdoc JSON on HEAD and on v1.0.0 with the pinned nightly
(scripts/ci/NIGHTLY_RUSTDOC_JSON) and takes the set of public enums
reachable from the crate root, by definition path, on each side. Every
enum on HEAD that is absent at v1.0.0 must carry #[non_exhaustive] (the
`attrs` in rustdoc JSON). Each one that does not is reported with its
path, unless it is allowlisted in
scripts/ci/exhaustive_enums_allowed.txt.

With --features-file (scripts/ci/semver_features.txt, the same list the
gate's check-release feature pass reads), a crate with a line gets its
rustdoc JSON built with defaults plus those features instead of defaults
alone: one pair, not two, since defaults stay on. Each side enables only
the features its own manifest declares, so a feature that is new since
v1.0.0 is built on HEAD only -- which is precisely what surfaces an enum
that is both new and feature-gated.

cargo-semver-checks 0.50.0 does not expose the rustdoc JSON it
generates, so this builds its own with the same pinned toolchain; both
tools read rustdoc format 57 (verified: check-release under
cargo +nightly-2026-03-15 parses what that nightly emits).

A MISSING TOOL IS NOT A PASS. No nightly toolchain, no rustdoc JSON, an
unknown format_version, or a malformed allowlist prints SKIPPED on
stderr and exits 2, never 0. The release workflow fails on 2, as for
the main gate.

Exit codes: 0 all new enums sealed or allowlisted, 1 new exhaustive
enum found (paths on stdout), 2 skipped.

Usage:
    python3 scripts/ci/check_new_public_enums.py --crates-file SELECTED
    python3 scripts/ci/check_new_public_enums.py --crates pdf-engine pdfluent
    python3 scripts/ci/check_new_public_enums.py --crates-file SELECTED \\
        --features-file scripts/ci/semver_features.txt   # the feature pass
    python3 scripts/ci/check_new_public_enums.py --head-json H.json \\
        --baseline-json B.json --crate demo   # fixture mode (tests)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PIN_FILE = REPO / "scripts" / "ci" / "NIGHTLY_RUSTDOC_JSON"
DEFAULT_ALLOWLIST = REPO / "scripts" / "ci" / "exhaustive_enums_allowed.txt"
DEFAULT_BASELINE_REV = "v1.0.0"

# The only rustdoc JSON schema this parser understands. The pinned
# nightly emits 57; anything else is SKIPPED, not guessed at.
SUPPORTED_FORMAT_VERSIONS = frozenset({57})

SKIP_PREFIX = "SKIPPED (not a pass)"


class Skip(Exception):
    """Claim nothing: the caller exits 2."""


def read_pin() -> str:
    try:
        pin = PIN_FILE.read_text(encoding="utf-8").strip()
    except OSError as exc:
        raise Skip(f"cannot read {PIN_FILE}: {exc}") from exc
    if not pin or pin.startswith("#"):
        raise Skip(f"no toolchain pinned in {PIN_FILE}")
    return pin.split()[0]


def run(cmd: list[str], cwd: Path | None = None, extra_env: dict | None = None) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    if extra_env:
        env.update(extra_env)
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)


def is_public(item: dict) -> bool:
    return item.get("visibility") == "public"


def inner_kind(item: dict) -> tuple[str, dict]:
    inner = item.get("inner")
    if isinstance(inner, dict) and len(inner) == 1:
        kind = next(iter(inner))
        body = inner[kind]
        return kind, body if isinstance(body, dict) else {}
    return "", {}


def public_enums(doc: dict) -> dict[str, bool]:
    """Public enums reachable from the crate root: definition path -> sealed.

    The walk starts at the root module and follows public module items
    and public re-exports (`use`,née `import`). An enum counts only when
    its definition path starts at the local crate: a re-export of
    another crate's enum is that crate's business, and external item ids
    are not stable across builds. `attrs` carries #[non_exhaustive] as
    the bare string "non_exhaustive" in format 57.
    """
    fmt = doc.get("format_version")
    if fmt not in SUPPORTED_FORMAT_VERSIONS:
        raise Skip(f"unknown rustdoc format_version {fmt}")
    index = doc.get("index") or {}
    paths = doc.get("paths") or {}
    root = doc.get("root")
    if root is None or str(root) not in index:
        raise Skip("rustdoc JSON has no usable crate root")
    root_path = (paths.get(str(root)) or {}).get("path") or []
    local_crate = root_path[0] if root_path else None

    found: dict[str, bool] = {}
    seen: set[str] = set()
    stack = [str(root)]
    while stack:
        key = stack.pop()
        if key in seen:
            continue
        seen.add(key)
        item = index.get(key)
        if not isinstance(item, dict):
            continue
        kind, body = inner_kind(item)
        if kind == "module":
            if body.get("is_stripped"):
                continue
            for child in body.get("items") or []:
                child_item = index.get(str(child))
                if isinstance(child_item, dict) and is_public(child_item):
                    stack.append(str(child))
        elif kind in ("use", "import"):
            # A public re-export makes its target reachable whatever the
            # target's own visibility says (a `pub use` of a non-public
            # item does not compile).
            if is_public(item) and body.get("id") is not None:
                stack.append(str(body["id"]))
        elif kind == "enum":
            entry = paths.get(key) or {}
            path = entry.get("path") or []
            if local_crate is not None and (not path or path[0] != local_crate):
                continue
            dotted = "::".join(path) if path else str(item.get("name") or key)
            attrs = item.get("attrs") or []
            sealed = any(a == "non_exhaustive" for a in attrs if isinstance(a, str))
            found.setdefault(dotted, sealed)
    return found


def load_allowlist(path: Path) -> set[str]:
    allowed: set[str] = set()
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise Skip(f"cannot read allowlist {path}: {exc}") from exc
    for lineno, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        parts = stripped.split(None, 1)
        if len(parts) != 2 or not parts[1].strip():
            raise Skip(f"malformed allowlist {path}:{lineno}: a reason is required")
        allowed.add(parts[0])
    return allowed


def lib_target_of(metadata: dict, package: str) -> str | None:
    for pkg in metadata.get("packages") or []:
        if pkg.get("name") != package:
            continue
        for target in pkg.get("targets") or []:
            kinds = target.get("kind") or []
            if "lib" in kinds or "rlib" in kinds:
                return str(target.get("name")).replace("-", "_")
        return None
    return None


def package_features(metadata: dict, package: str) -> set[str]:
    for pkg in metadata.get("packages") or []:
        if pkg.get("name") == package:
            feats = pkg.get("features") or {}
            return set(feats)
    return set()


def load_features_file(path: Path) -> dict[str, list[str]]:
    """Non-default features per crate: `crate: f1 f2` lines, `#` comments.

    The same grammar the gate validates in semver_gate.sh (which also
    rejects unknown crates and repeats; this parser fails closed the same
    way, except that a line for a crate that is not being checked is left
    for the caller to ignore -- `--crates` runs a subset).
    """
    wanted: dict[str, list[str]] = {}
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise Skip(f"cannot read features file {path}: {exc}") from exc
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = re.fullmatch(r"([A-Za-z0-9_-]+)\s*:\s*(.+)", line)
        if not m:
            raise Skip(f"malformed features file {path}:{lineno}")
        crate, feats = m.group(1), m.group(2).split()
        if crate in wanted:
            raise Skip(f"features file {path}:{lineno} repeats crate '{crate}'")
        for feat in feats:
            if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.\-/:+]*", feat):
                raise Skip(f"features file {path}:{lineno} has a bad feature "
                           f"name '{feat}'")
        if not feats:
            raise Skip(f"malformed features file {path}:{lineno}")
        wanted[crate] = feats
    return wanted


def cargo_metadata(manifest: Path) -> dict:
    proc = run(["cargo", "metadata", "--no-deps", "--format-version", "1",
                "--manifest-path", str(manifest)])
    if proc.returncode != 0:
        raise Skip(f"cargo metadata failed for {manifest}: {proc.stderr.strip()[-500:]}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise Skip(f"cargo metadata output for {manifest} is not JSON: {exc}") from exc


def check_toolchain(toolchain: str) -> None:
    proc = run(["cargo", f"+{toolchain}", "--version"])
    if proc.returncode != 0:
        detail = (proc.stderr or proc.stdout).strip().splitlines()
        hint = detail[0][-200:] if detail else "not installed"
        raise Skip(f"nightly toolchain '{toolchain}' is not usable: {hint}")


def build_rustdoc(toolchain: str, root: Path, package: str, lib: str, side: str,
                 target_dir: Path | None, features: list[str] | None = None) -> Path:
    # The baseline side always builds in its own target dir: an ambient
    # CARGO_TARGET_DIR would otherwise point two source trees at one
    # directory, thrashing fingerprints and overwriting doc/<lib>.json.
    extra = {"CARGO_TARGET_DIR": str(target_dir)} if target_dir else None
    cmd = ["cargo", f"+{toolchain}", "rustdoc", "-p", package, "--lib",
           "--manifest-path", str(root / "Cargo.toml")]
    if features:
        # Defaults stay on: the file lists non-default features only, and
        # the feature pair must still see everything the default pair sees.
        cmd += ["--features", ",".join(features)]
    cmd += ["--", "-Z", "unstable-options", "--output-format", "json"]
    proc = run(cmd, extra_env=extra)
    base = target_dir if target_dir else Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
    out = base / "doc" / f"{lib}.json"
    if proc.returncode != 0 or not out.is_file():
        tail = (proc.stderr or proc.stdout).strip().splitlines()
        hint = " / ".join(tail[-3:])[-500:] if tail else "no output"
        raise Skip(f"no rustdoc JSON for {package} ({side}): {hint}")
    return out


def load_json(path: Path, what: str) -> dict:
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise Skip(f"cannot read {what} {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise Skip(f"{what} {path} is not JSON: {exc}") from exc
    if not isinstance(doc, dict):
        raise Skip(f"{what} {path} is not a rustdoc document")
    return doc


def crate_names_from_file(path: Path) -> list[str]:
    names: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if parts:
            names.append(parts[0])
    return names


def check_crate(package: str, head_doc: dict, base_doc: dict,
                allowed: set[str]) -> tuple[list[str], int]:
    """Returns (violations, new_enum_count) for one crate."""
    head = public_enums(head_doc)
    base = public_enums(base_doc)
    violations = sorted(
        path for path, sealed in head.items()
        if path not in base and not sealed and path not in allowed
    )
    new_count = sum(1 for path in head if path not in base)
    return violations, new_count


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else "")
    inputs = parser.add_mutually_exclusive_group()
    inputs.add_argument("--crates-file", type=Path,
                        help="file with `name version` lines from the gate selection")
    inputs.add_argument("--crates", nargs="*", help="package names to check")
    parser.add_argument("--crate", default="fixture", help="crate name in fixture mode")
    parser.add_argument("--head-json", type=Path, help="HEAD rustdoc JSON (fixture mode)")
    parser.add_argument("--baseline-json", type=Path, help="baseline rustdoc JSON (fixture mode)")
    parser.add_argument("--toolchain", help="rustdoc toolchain (default: the pin file)")
    parser.add_argument("--allowlist", type=Path, default=DEFAULT_ALLOWLIST)
    parser.add_argument("--baseline-rev", default=DEFAULT_BASELINE_REV)
    parser.add_argument("--features-file", type=Path, default=None,
                        help="crate → features file (default: defaults alone)")
    parser.add_argument("--repo-root", type=Path, default=REPO,
                        help="repository to check (tests only; default: this one)")
    args = parser.parse_args(argv)

    fixture = args.head_json is not None or args.baseline_json is not None
    if fixture and args.features_file is not None:
        parser.error("--features-file builds rustdoc; it contradicts --head-json")
    try:
        allowed = load_allowlist(args.allowlist)
        # Before the toolchain check: a malformed file fails closed without
        # needing any toolchain, and the test asserts exactly that.
        wanted = load_features_file(args.features_file) if args.features_file else {}
        if fixture:
            if args.head_json is None or args.baseline_json is None:
                parser.error("--head-json and --baseline-json come as a pair")
            violations, new_count = check_crate(
                args.crate, load_json(args.head_json, "head"),
                load_json(args.baseline_json, "baseline"), allowed)
            for path in violations:
                print(f"new exhaustive public enum: {path} (crate {args.crate})")
            print(f"[enums] {args.crate}: {new_count} new public enum(s), "
                  f"{len(violations)} unsealed", file=sys.stderr)
            return 1 if violations else 0

        if args.crates_file is not None:
            packages = crate_names_from_file(args.crates_file)
        elif args.crates:
            packages = list(args.crates)
        else:
            parser.error("one of --crates-file, --crates, or the --*-json pair is required")
        if not packages:
            raise Skip("no crates to check")
        toolchain = args.toolchain or read_pin()
        check_toolchain(toolchain)

        work = Path(tempfile.mkdtemp(prefix="new-public-enums-"))
        baseline_root = work / "baseline"
        try:
            proc = run(["git", "-C", str(args.repo_root), "worktree", "add", "--detach",
                        str(baseline_root), args.baseline_rev])
            if proc.returncode != 0:
                raise Skip(f"cannot check out {args.baseline_rev} baseline: "
                            f"{proc.stderr.strip()[-300:]}")
            head_meta = cargo_metadata(args.repo_root / "Cargo.toml")
            base_meta = cargo_metadata(baseline_root / "Cargo.toml")
            all_violations: list[tuple[str, str]] = []
            checked = 0
            new_total = 0
            for package in packages:
                head_lib = lib_target_of(head_meta, package)
                if head_lib is None:
                    print(f"[enums] {package}: no lib target, nothing public to seal",
                          file=sys.stderr)
                    continue
                base_lib = lib_target_of(base_meta, package)
                if base_lib is None:
                    raise Skip(f"no rustdoc JSON for {package} (baseline): "
                                "no lib target at the baseline revision")
                # Each side enables only the features its own manifest
                # declares: a feature new since the baseline builds on HEAD
                # only, and one removed since builds on the baseline only.
                # Features that exist on neither side are a typo the gate
                # validation missed, so they fail closed rather than
                # silently checking defaults.
                listed = wanted.get(package, [])
                head_have = package_features(head_meta, package)
                base_have = package_features(base_meta, package)
                ghost = [f for f in listed if f not in head_have and f not in base_have]
                if ghost:
                    raise Skip(f"features file lists unknown features for {package}: "
                               f"{', '.join(ghost)}")
                head_feats = [f for f in listed if f in head_have]
                base_feats = [f for f in listed if f in base_have]
                feat_note = f" features: {' '.join(head_feats)}" if head_feats else ""
                print(f"[enums] {package}: rustdoc on HEAD and {args.baseline_rev} "
                      f"({toolchain}){feat_note}", file=sys.stderr)
                if head_feats != base_feats:
                    only = [f for f in head_feats if f not in base_feats]
                    if only:
                        print(f"[enums] {package}: baseline lacks features "
                              f"{' '.join(only)} (enabled on HEAD only)", file=sys.stderr)
                head_doc = load_json(build_rustdoc(toolchain, args.repo_root, package,
                                                   head_lib, "HEAD", None, head_feats),
                                     f"head {package}")
                base_doc = load_json(build_rustdoc(toolchain, baseline_root, package, base_lib,
                                                   args.baseline_rev, work / "baseline-target",
                                                   base_feats),
                                     f"baseline {package}")
                violations, new_count = check_crate(package, head_doc, base_doc, allowed)
                checked += 1
                new_total += new_count
                for path in violations:
                    all_violations.append((package, path))
                    print(f"new exhaustive public enum: {path} (crate {package})")
            if checked == 0:
                raise Skip("no crates with a lib target to check")
            if all_violations:
                print(f"[enums] FAIL: {len(all_violations)} new exhaustive public enum(s) "
                      f"in {checked} crate(s)", file=sys.stderr)
                return 1
            print(f"[enums] OK: {checked} crate(s), {new_total} new public enum(s), "
                  "all sealed or allowlisted", file=sys.stderr)
            return 0
        finally:
            run(["git", "-C", str(args.repo_root), "worktree", "remove", "--force",
                 str(baseline_root)])
            shutil.rmtree(work, ignore_errors=True)
    except Skip as exc:
        print(f"{SKIP_PREFIX}: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
