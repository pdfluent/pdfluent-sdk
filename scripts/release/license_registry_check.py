#!/usr/bin/env python3
"""
license_registry_check.py — enforce `docs/release/canonical_licenses.toml`.

For every entry in the canonical registry the script verifies that the
crate's `Cargo.toml` declares the EXACT license the registry mandates,
that all required LICENSE files exist on disk inside the crate dir, and
optionally that the LICENSE first line matches an expected string.

It also performs the reverse check: every `publish = true` crate in the
workspace MUST appear in the registry. This catches the failure mode
"I added a new crate but forgot the license decision".

The check runs as a gate in `scripts/ci/local_ci_gate.sh` and from
`scripts/release/prepublish_crate_audit.sh`. It is also safe to run
standalone for diagnostics:

    python3 scripts/release/license_registry_check.py
    python3 scripts/release/license_registry_check.py --verbose
    python3 scripts/release/license_registry_check.py --registry path/to.toml

Exit codes:
    0  all entries match the registry; no missing registry entries
    1  one or more mismatches OR a workspace crate missing from registry
    2  argv / file-read / TOML-parse error

Standard library only (tomllib + re + pathlib + sys). Requires Python 3.11+
for tomllib.
"""
from __future__ import annotations
import argparse
import re
import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ImportError:
    print("ERROR: this script requires Python 3.11+ (for tomllib).", file=sys.stderr)
    sys.exit(2)


# --------------------------------------------------------------------------
# Cargo.toml parsing helpers
# --------------------------------------------------------------------------

LICENSE_RX      = re.compile(r'(?m)^license\s*=\s*"([^"]+)"')
LICENSE_FILE_RX = re.compile(r'(?m)^license-file\s*=\s*"([^"]+)"')
NAME_RX         = re.compile(r'(?m)^name\s*=\s*"([^"]+)"')
PUBLISH_FALSE_RX = re.compile(r'(?m)^publish\s*=\s*false')


def extract_package_section(cargo_text: str) -> str:
    """Return only the `[package]` section of a Cargo.toml file as text.

    The license declaration must live there; pulling out just that section
    avoids false-positives from `license = "..."` in `[workspace.package]`
    or random comments.
    """
    lines = cargo_text.splitlines()
    out = []
    in_pkg = False
    for line in lines:
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            in_pkg = (s == "[package]")
            continue
        if in_pkg:
            out.append(line)
    return "\n".join(out)


def cargo_license_decl(cargo_path: Path) -> tuple[str | None, str | None]:
    """Return (decl_field, value) for the `[package]` license declaration.

    decl_field is "license", "license-file", or None (neither set).
    """
    if not cargo_path.exists():
        return None, None
    pkg_text = extract_package_section(cargo_path.read_text())
    m_lic = LICENSE_RX.search(pkg_text)
    if m_lic:
        return "license", m_lic.group(1)
    m_lf = LICENSE_FILE_RX.search(pkg_text)
    if m_lf:
        return "license-file", m_lf.group(1)
    return None, None


# --------------------------------------------------------------------------
# Workspace enumeration
# --------------------------------------------------------------------------

def enumerate_publish_eligible_crates(repo_root: Path) -> dict[str, str]:
    """Return {published_name: workspace_dir_basename} for every crate with
    publish ≠ false."""
    crates = {}
    for cargo in sorted((repo_root / "crates").glob("*/Cargo.toml")):
        text = cargo.read_text()
        name_m = NAME_RX.search(text)
        if not name_m:
            continue
        name = name_m.group(1)
        if PUBLISH_FALSE_RX.search(text):
            continue
        crates[name] = cargo.parent.name
    return crates


# --------------------------------------------------------------------------
# Per-entry verifier
# --------------------------------------------------------------------------

def check_entry(entry: dict, repo_root: Path, verbose: bool) -> list[str]:
    """Return a list of failure messages for one registry entry (empty list = PASS)."""
    failures: list[str] = []
    pub_name = entry.get("published_name")
    ws_dir   = entry.get("workspace_dir")
    if not pub_name or not ws_dir:
        failures.append(f"registry entry missing published_name / workspace_dir: {entry}")
        return failures

    crate_dir  = repo_root / "crates" / ws_dir
    cargo_path = crate_dir / "Cargo.toml"
    if not cargo_path.exists():
        failures.append(f"{pub_name}: crates/{ws_dir}/Cargo.toml not found")
        return failures

    # 1) Verify Cargo.toml [package] license declaration matches registry
    decl, value = cargo_license_decl(cargo_path)
    expected_decl  = entry["license_decl"]    # "license" or "license-file"
    expected_value = entry["license_value"]
    if decl != expected_decl or value != expected_value:
        failures.append(
            f"{pub_name}: Cargo.toml [package] license declaration mismatch — "
            f"expected `{expected_decl} = \"{expected_value}\"` "
            f"but found `{decl} = \"{value}\"`"
        )

    # 2) Verify every required file exists in the crate dir
    for fname in entry["required_files"]:
        fp = crate_dir / fname
        if not fp.exists():
            failures.append(f"{pub_name}: required licence file missing — crates/{ws_dir}/{fname}")

    # 3) Verify license_first_line_check (literal or contains)
    check = entry.get("license_first_line_check")
    if check:
        target = crate_dir / check["file"]
        if target.exists():
            first = ""
            for line in target.read_text(errors="replace").splitlines():
                if line.strip():
                    first = line
                    break
            if "literal" in check:
                if first != check["literal"]:
                    failures.append(
                        f"{pub_name}: first line of {check['file']} mismatch — "
                        f"expected literal `{check['literal']!r}` but got `{first!r}`"
                    )
            elif "contains" in check:
                needle = check["contains"]
                # check entire file for the contains-string (LICENSE-MIT's
                # copyright might not be on line 1)
                content = target.read_text(errors="replace")
                if needle not in content:
                    failures.append(
                        f"{pub_name}: {check['file']} does not contain `{needle!r}`"
                    )

    if verbose and not failures:
        print(f"  ✓ {pub_name:28s} ({entry['license_kind']:24s})  decl=`{expected_decl}=\"{expected_value}\"`")
    return failures


# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--registry", default="docs/release/canonical_licenses.toml",
                    help="Path to the canonical registry TOML.")
    ap.add_argument("--verbose", "-v", action="store_true",
                    help="Print a line per crate (PASS or FAIL).")
    args = ap.parse_args()

    repo_root = Path.cwd()
    registry_path = repo_root / args.registry
    if not registry_path.exists():
        print(f"ERROR: registry not found at {registry_path}", file=sys.stderr)
        return 2

    with registry_path.open("rb") as f:
        try:
            registry = tomllib.load(f)
        except tomllib.TOMLDecodeError as e:
            print(f"ERROR: cannot parse {registry_path}: {e}", file=sys.stderr)
            return 2

    entries = registry.get("crate", [])
    if not entries:
        print(f"ERROR: no [[crate]] entries in {registry_path}", file=sys.stderr)
        return 2

    all_failures: list[str] = []

    # --- Phase 1: each registry entry matches reality ---
    if args.verbose:
        print(f"=== license registry: {len(entries)} entries ===")
    for entry in entries:
        all_failures.extend(check_entry(entry, repo_root, args.verbose))

    # --- Phase 2: every publish-eligible crate is in the registry ---
    workspace_pub = enumerate_publish_eligible_crates(repo_root)
    registry_names = {e["published_name"] for e in entries}
    missing = sorted(set(workspace_pub.keys()) - registry_names)
    extra   = sorted(registry_names - set(workspace_pub.keys()))
    if missing:
        for name in missing:
            all_failures.append(
                f"workspace crate missing from registry: `{name}` "
                f"(crates/{workspace_pub[name]}/Cargo.toml has publish ≠ false). "
                f"Add a [[crate]] entry to docs/release/canonical_licenses.toml."
            )
    if extra:
        for name in extra:
            all_failures.append(
                f"registry has entry for `{name}` but no workspace crate of that name exists "
                f"(either workspace_dir is wrong, or the crate is publish = false / removed)."
            )

    # --- Phase 3: also flag stand-alone LICENSE files in publish=true crates
    #     whose registry entry does NOT list them (catches the "two-licence"
    #     ambiguity: a PDFluent Commercial LICENSE next to LICENSE-APACHE/MIT,
    #     which makes the published tarball legally ambiguous). ---
    for entry in entries:
        ws_dir = entry.get("workspace_dir")
        if not ws_dir:
            continue
        crate_dir = repo_root / "crates" / ws_dir
        if not crate_dir.is_dir():
            continue
        on_disk = sorted(p.name for p in crate_dir.iterdir()
                         if p.is_file() and p.name.startswith("LICENSE"))
        allowed = set(entry["required_files"])
        unexpected = [f for f in on_disk if f not in allowed]
        if unexpected:
            all_failures.append(
                f"{entry['published_name']}: unexpected LICENSE file(s) present that the registry "
                f"does not list — {unexpected}. Remove them, or update the registry entry "
                f"if they are deliberately required (the published tarball would otherwise "
                f"contain a confusing mix of licences)."
            )

    # --- Report ---
    if all_failures:
        print()
        print(f"LICENSE_REGISTRY_CHECK: FAIL ({len(all_failures)} issue(s))")
        for msg in all_failures:
            print(f"  ✗ {msg}")
        print()
        print("To fix: edit `docs/release/canonical_licenses.toml` AND the crate's")
        print("Cargo.toml / LICENSE files together in ONE focused commit. The registry")
        print("is the source of truth; reality must conform to it, not the other way.")
        return 1

    print(f"LICENSE_REGISTRY_CHECK: PASS ({len(entries)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
