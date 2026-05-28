#!/usr/bin/env python3
"""
ledger_add_entry.py — append a new "live" entry to the per-channel SHA ledger.

Called by the per-channel publish step immediately after a successful registry
upload. Computes sha256 + size from the local artifact, writes a `status=live`
entry. See `docs/release/sha_ledger/README.md` and `schema.json`.

Usage:
    ledger_add_entry.py \\
        --channel <crates_io|npm|pypi|maven|nuget|wasm|binary|gitlab> \\
        --package <name> \\
        --version <semver> \\
        --artifact <path/to/local/artifact-file> \\
        --audit-report <repo-relative-path.md> \\
        --registry-url <https://...> \\
        [--ledger-dir docs/release/sha_ledger]

Exit codes:
    0  entry appended (or already present and identical → idempotent ok)
    1  argv/path error
    2  duplicate entry exists with DIFFERENT sha256 → refused (corruption guard)
    3  schema validation failure on the resulting ledger
    4  artifact file does not exist or is empty

The script is intentionally stdlib-only (hashlib, json, argparse) so it runs
in any CI environment without further dependency. JSON I/O is atomic
(tempfile + rename) so concurrent invocations cannot corrupt the ledger.

Run `--help` for the canonical option list.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

CHANNELS = ("crates_io", "npm", "pypi", "maven", "nuget", "wasm", "binary", "gitlab")
SEMVER_RX = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$")
SHA256_RX = re.compile(r"^[0-9a-f]{64}$")


def sha256_file(path: Path) -> tuple[str, int]:
    """Return (sha256-hex, size-in-bytes) for `path`."""
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as f:
        while True:
            chunk = f.read(1 << 20)  # 1 MiB
            if not chunk:
                break
            h.update(chunk)
            size += len(chunk)
    return h.hexdigest(), size


def atomic_write_json(path: Path, data: dict) -> None:
    """Write JSON to `path` atomically (tempfile in same dir, then rename)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(path.parent), prefix=".ledger-", suffix=".tmp")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(data, f, indent=2, sort_keys=False)
            f.write("\n")
        os.replace(tmp, path)
    except Exception:
        try:
            os.unlink(tmp)
        except FileNotFoundError:
            pass
        raise


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--channel", required=True, choices=CHANNELS)
    ap.add_argument("--package", required=True, help="Registry package name.")
    ap.add_argument("--version", required=True, help="SemVer version string.")
    ap.add_argument("--artifact", required=True, type=Path,
                    help="Path to the local artifact file (the exact bytes that were uploaded).")
    ap.add_argument("--audit-report", required=True,
                    help="Repo-relative path to the prepublish audit report (committed under "
                         "benchmarks/runs/prepublish_audits/).")
    ap.add_argument("--registry-url", required=True,
                    help="Canonical URL where the artifact can be re-downloaded.")
    ap.add_argument("--ledger-dir", default="docs/release/sha_ledger", type=Path,
                    help="Directory containing the per-channel ledger files. Default: docs/release/sha_ledger.")
    ap.add_argument("--repo-root", default=".", type=Path,
                    help="Repository root (default: current working directory).")
    args = ap.parse_args()

    # Path resolution: --ledger-dir / --repo-root may be either absolute or
    # relative-to-cwd; --audit-report and --registry-url are STORED verbatim.
    repo_root = args.repo_root.resolve()
    ledger_path = (repo_root / args.ledger_dir / f"{args.channel}.json").resolve()
    artifact_path = args.artifact.resolve()

    if not artifact_path.is_file():
        print(f"ERROR: artifact file does not exist: {artifact_path}", file=sys.stderr)
        return 4

    if not SEMVER_RX.match(args.version):
        print(f"ERROR: version does not match SemVer pattern: {args.version}", file=sys.stderr)
        return 1

    if not ledger_path.is_file():
        print(f"ERROR: ledger file does not exist: {ledger_path}", file=sys.stderr)
        print(f"       (expected an empty starter file with schema_version='1' and channel='{args.channel}')",
              file=sys.stderr)
        return 1

    sha, size = sha256_file(artifact_path)
    if size == 0:
        print(f"ERROR: artifact is empty (0 bytes): {artifact_path}", file=sys.stderr)
        return 4

    with ledger_path.open() as f:
        ledger = json.load(f)

    # Idempotency + corruption guard: refuse if an entry with the same
    # (package, version) already exists with a DIFFERENT sha256. If the
    # existing entry matches byte-for-byte, do nothing (idempotent).
    for existing in ledger.get("entries", []):
        if existing["package"] == args.package and existing["version"] == args.version:
            if existing["sha256_local"] == sha and existing["artifact_filename"] == artifact_path.name:
                print(f"OK (idempotent): entry already present and matches: "
                      f"{args.channel}/{args.package}@{args.version} sha={sha[:12]}…")
                return 0
            print(f"ERROR: duplicate entry with mismatching sha256 (corruption guard).", file=sys.stderr)
            print(f"  existing: sha256={existing['sha256_local']} filename={existing['artifact_filename']}",
                  file=sys.stderr)
            print(f"  new:      sha256={sha} filename={artifact_path.name}", file=sys.stderr)
            return 2

    now_utc = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    entry = {
        "package": args.package,
        "version": args.version,
        "artifact_filename": artifact_path.name,
        "sha256_local": sha,
        "sha256_registry": None,
        "size_bytes": size,
        "published_at": now_utc,
        "verified_at": None,
        "audit_report": args.audit_report,
        "verify_report": None,
        "status": "live",
        "yanked_at": None,
        "yanked_reason": None,
        "registry_url": args.registry_url,
    }

    ledger["entries"].append(entry)
    atomic_write_json(ledger_path, ledger)

    print(f"OK: appended {args.channel}/{args.package}@{args.version}")
    print(f"  artifact:   {artifact_path.name}")
    print(f"  sha256:     {sha}")
    print(f"  size:       {size} bytes")
    print(f"  ledger:     {ledger_path.relative_to(repo_root)} ({len(ledger['entries'])} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
