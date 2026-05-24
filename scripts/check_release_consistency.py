#!/usr/bin/env python3
"""
Release consistency checker for the PDFluent SDK workspace.

Validates that local crate versions match what is live on crates.io,
that required metadata is present, and that license fields are consistent.

Usage:
    python3 scripts/check_release_consistency.py [--fix-report]

Exit codes:
    0 — all checks pass
    1 — one or more checks failed (details printed)
"""

# Allow PEP-604 (`X | None`) annotations to evaluate lazily so this gate runs on
# Python 3.9 (CI runner + macOS system python), not only 3.10+. Without this the
# checker raised `TypeError: unsupported operand type(s) for |` at import time —
# a release gate that crashes provides false safety.
from __future__ import annotations

import argparse
import glob
import json
import re
import sys
import time
import urllib.request
from pathlib import Path


# Crates that are planned for crates.io publication but not yet in the release train.
# They have publish=true in Cargo.toml (intentional — they will be published) but
# have never been published. Treated as warnings, not failures.
UNRELEASED_CRATES = {
    "pdf-invoice",   # FDF/XFDF + ZUGFeRD/Factur-X e-invoicing (planned)
    "pdf-pptx",      # PDF → PPTX conversion (planned)
    "pdf-xlsx",      # PDF table extraction + XLSX conversion (planned)
    "xfa-cli",       # PDFluent CLI (planned)
}

# Crates whose version intentionally diverges from the beta.x scheme
# (they follow their own semver track).
INDEPENDENT_VERSION_CRATES = {
    "pdf-syntax",
    "pdf-interpret",
    "pdf-font",       # currently beta.x but historically independent
    "pdfluent-lopdf",
    "pdfluent-cff",
    "pdfluent-ccitt",
    "pdfluent-jbig2",
    "pdfluent-jpeg2000",
    "pdf-extract",    # pdfluent-extract, independent versioning
}

# Crates with publish = false (internal only, not validated against crates.io)
INTERNAL_CRATES = {
    "pdf-diff",
    "pdf-node",
    "pdf-bench",
    "pdf-desktop",
    "xfa-pdfrest-compare",
    "pdf-capi",        # stale MIT license, not yet ready for publish
    "xfa-wasm",        # WASM artifact, published separately
    "xfa-api-server",
}


def _parse_section(content: str, section_header: str) -> dict:
    """Extract key=value pairs from a named TOML section."""
    result = {}
    in_section = False
    for line in content.split("\n"):
        stripped = line.strip()
        if stripped == section_header:
            in_section = True
            continue
        if stripped.startswith("[") and stripped != section_header:
            in_section = False
        if in_section and "=" in stripped and not stripped.startswith("#"):
            k, _, v = stripped.partition("=")
            key = k.strip()
            val = v.strip().strip('"').strip("'")
            result[key] = val
    return result


def parse_workspace_package(ws_path: str) -> dict:
    """Parse [workspace.package] defaults from the root Cargo.toml."""
    content = Path(ws_path).read_text()
    return _parse_section(content, "[workspace.package]")


def parse_cargo_toml(path: str, workspace_pkg: dict | None = None) -> dict:
    """Minimal TOML parser for [package] sections.

    Resolves `key.workspace = true` references using workspace_pkg defaults.
    """
    content = Path(path).read_text()
    raw = _parse_section(content, "[package]")
    if workspace_pkg is None:
        return raw
    result = {}
    for key, val in raw.items():
        if key.endswith(".workspace") and val == "true":
            # e.g. "repository.workspace" -> look up "repository" in workspace defaults
            ws_key = key[: -len(".workspace")]
            result[ws_key] = workspace_pkg.get(ws_key, "")
        else:
            result[key] = val
    # Also pull in workspace fields that are not explicitly set in [package]
    # (some crates omit the key entirely, relying on workspace inheritance
    # set via workspace = true at a higher level — not common, but safe to skip)
    return result


def fetch_crates_io(crate: str) -> dict | None:
    """Fetch crate metadata from crates.io. Returns None on 404."""
    url = f"https://crates.io/api/v1/crates/{crate}"
    req = urllib.request.Request(url, headers={"User-Agent": "pdfluent-release-check/1.0"})
    try:
        response = urllib.request.urlopen(req, timeout=10)
        return json.loads(response.read())
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise
    except Exception as e:
        print(f"  WARNING: could not fetch {crate} from crates.io: {e}", file=sys.stderr)
        return None


def check_metadata(pkg: dict, path: str) -> list[str]:
    """Return list of metadata issues."""
    issues = []
    for field in ("description", "repository", "homepage", "documentation"):
        if not pkg.get(field):
            issues.append(f"  missing [{field}]")
    # license should be license-file pointing to commercial LICENSE, not MIT
    if pkg.get("license", "").upper() == "MIT" and not pkg.get("license-file"):
        issues.append("  license = MIT (should be license-file pointing to commercial LICENSE)")
    readme = pkg.get("readme", "README.md")
    readme_path = Path(path).parent / readme
    if not readme_path.exists():
        issues.append(f"  README not found at {readme_path}")
    return issues


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fix-report", action="store_true",
                        help="Print actionable fix commands at the end")
    parser.add_argument("--offline", action="store_true",
                        help="Skip crates.io network checks; run only local invariants "
                             "(first-party version consistency + metadata). Runnable as a local gate.")
    args = parser.parse_args()

    # Collect first-party RC-line versions (1.0.0-beta.N) for an internal
    # consistency check, independent of crates.io. INDEPENDENT/INTERNAL/UNRELEASED
    # crates are excluded (they intentionally diverge or are not in the train).
    first_party_versions: dict[str, str] = {}

    tomls = sorted(glob.glob("crates/*/Cargo.toml"))
    if not tomls:
        print("ERROR: no crates/*/Cargo.toml found. Run from workspace root.", file=sys.stderr)
        return 1

    # Read workspace package defaults (version, repository, homepage, documentation, etc.)
    ws_toml_path = "Cargo.toml"
    ws_content = Path(ws_toml_path).read_text()
    ws_pkg = parse_workspace_package(ws_toml_path)
    ws_ver_m = re.search(r'\[workspace\.package\].*?version\s*=\s*"(.+?)"', ws_content, re.DOTALL)
    workspace_version = ws_ver_m.group(1) if ws_ver_m else ws_pkg.get("version", "UNKNOWN")
    print(f"Workspace version: {workspace_version}")
    print()

    failures = []
    warnings = []

    for toml_path in tomls:
        pkg = parse_cargo_toml(toml_path, workspace_pkg=ws_pkg)
        name = pkg.get("name", "?")
        local_ver = pkg.get("version", workspace_version)  # workspace-inherited if absent
        publish = pkg.get("publish", "true")

        if name in INTERNAL_CRATES or publish == "false" or publish == "[]":
            print(f"  SKIP  {name:45} (publish=false)")
            continue

        # Metadata checks (for all publishable crates)
        meta_issues = check_metadata(pkg, toml_path)
        if meta_issues:
            for issue in meta_issues:
                warnings.append(f"WARN  {name}: {issue.strip()}")

        # Version drift check
        if local_ver == "?":
            # Uses version.workspace = true
            local_ver = workspace_version
            if workspace_version != local_ver:
                warnings.append(
                    f"WARN  {name}: uses workspace version ({workspace_version}); "
                    f"ensure workspace version is bumped before next publish"
                )

        # Record first-party RC-line version for the internal-consistency check.
        if (
            name not in INDEPENDENT_VERSION_CRATES
            and name not in UNRELEASED_CRATES
            and re.fullmatch(r"1\.0\.0-beta\.\d+", local_ver or "")
        ):
            first_party_versions[name] = local_ver

        if args.offline:
            print(f"  CHECK {name:45} local={local_ver}  (offline: crates.io skipped)")
            continue

        print(f"  CHECK {name:45} local={local_ver}", end=" ")
        sys.stdout.flush()

        data = fetch_crates_io(name)
        time.sleep(0.2)  # rate limit

        if data is None:
            if name in INDEPENDENT_VERSION_CRATES:
                print("NOT_PUBLISHED (independent crate, may be intentional)")
            elif name in UNRELEASED_CRATES:
                print("NOT_PUBLISHED (planned, not yet in release train)")
                warnings.append(f"WARN  {name}: not yet published — add to publish_ordered.sh when ready")
            else:
                print("NOT_PUBLISHED ⚠️")
                failures.append(f"FAIL  {name}: not found on crates.io but publish=true")
            continue

        published_ver = data["crate"]["newest_version"]
        yanked = next(
            (v.get("yanked", False) for v in data["versions"] if v["num"] == published_ver),
            False,
        )

        if local_ver == published_ver:
            status = "✅ OK"
        elif name in INDEPENDENT_VERSION_CRATES:
            status = f"OK (independent, crates.io={published_ver})"
        else:
            status = f"⚠️  MISMATCH crates.io={published_ver}"
            warnings.append(
                f"WARN  {name}: local={local_ver} crates.io={published_ver} — "
                f"local is {'ahead' if local_ver > published_ver else 'behind'}"
            )

        if yanked:
            status += " (YANKED!)"
            failures.append(f"FAIL  {name}: latest version {published_ver} is yanked!")

        print(f"crates.io={published_ver}  {status}")

    print()

    # Internal first-party version consistency (offline-safe): all RC-line
    # (1.0.0-beta.N) first-party crates should share ONE version. Drift here is a
    # release-train risk regardless of crates.io state. Reported as a WARNING (the
    # alignment is a deliberate release decision; see the systems map report).
    distinct = sorted(set(first_party_versions.values()))
    if len(distinct) > 1:
        from collections import Counter
        counts = Counter(first_party_versions.values())
        print("=== FIRST-PARTY VERSION DRIFT (internal) ===")
        print(f"  RC-line first-party crates span {len(distinct)} versions: {', '.join(distinct)}")
        for ver in distinct:
            crates = sorted(n for n, v in first_party_versions.items() if v == ver)
            print(f"    {ver} ({counts[ver]}): {', '.join(crates)}")
        warnings.append(
            "WARN  first-party version drift: RC-line crates span "
            f"{', '.join(distinct)} — align before a coherent release train (decision required)"
        )
        print()
    elif distinct:
        print(f"First-party RC-line version: {distinct[0]} (consistent across "
              f"{len(first_party_versions)} crates)")
        print()

    if warnings:
        print("=== WARNINGS ===")
        for w in warnings:
            print(f"  {w}")
        print()

    if failures:
        print("=== FAILURES ===")
        for f in failures:
            print(f"  {f}")
        print()
        print(f"RESULT: {len(failures)} failure(s), {len(warnings)} warning(s) — FAIL")

        if args.fix_report:
            print()
            print("=== FIX COMMANDS ===")
            print("  # Workspace version bump (if behind published crates):")
            print(f'  # Current workspace version: {workspace_version}')
            print("  # Edit Cargo.toml [workspace.package] version before next publish")

        return 1
    else:
        print(f"RESULT: 0 failures, {len(warnings)} warning(s) — PASS")
        return 0


if __name__ == "__main__":
    sys.exit(main())
