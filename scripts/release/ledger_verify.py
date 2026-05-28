#!/usr/bin/env python3
"""
ledger_verify.py — verify a SHA ledger entry against the registry.

Downloads the artifact from the entry's `registry_url`, sha256s it, compares
to the committed `sha256_local`. On match: writes `sha256_registry` +
`verified_at` and bumps status `live` → `verified` (atomic). On mismatch:
refuses to write anything, prints the diff, exits non-zero.

This is the heart of indefinite drift detection: any third party can re-run
this years after the publish and the script will reproduce the original
sha256 comparison exactly.

Usage:
    ledger_verify.py \\
        --channel <crates_io|npm|pypi|maven|nuget|wasm|binary|gitlab> \\
        --package <name> \\
        --version <semver> \\
        [--verify-report <path-after-write.md>] \\
        [--ledger-dir docs/release/sha_ledger] \\
        [--write] \\
        [--insecure]

By default the script runs in *report-only* mode: it downloads, compares,
prints the verdict, exits 0 on match / 1 on mismatch, and does NOT mutate
the ledger. Pass `--write` to mutate the ledger entry (set
`sha256_registry`, `verified_at`, status=`verified`) when verification
succeeds. The `--verify-report` arg, if given, is stored on the entry as
the path to the committed post-publish verify report.

Exit codes:
    0  verified (sha256 match)
    1  mismatch / drift detected
    2  entry not found / argv/path error
    3  download error (transient network etc.)
    4  ledger file corrupt or schema-incompatible

stdlib-only (urllib, hashlib, json, argparse). Atomic ledger write via the
same tempfile+rename pattern as ledger_add_entry.py.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import ssl
import sys
import tempfile
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

USER_AGENT = "pdfluent-ledger-verify/1.0"
CHANNELS = ("crates_io", "npm", "pypi", "maven", "nuget", "wasm", "binary", "gitlab")


def sha256_stream(stream, max_bytes: int = 1 << 30) -> tuple[str, int]:
    """sha256 + size of a read-once byte stream. Default cap 1 GiB."""
    h = hashlib.sha256()
    size = 0
    while True:
        chunk = stream.read(1 << 20)  # 1 MiB
        if not chunk:
            break
        size += len(chunk)
        if size > max_bytes:
            raise RuntimeError(f"download exceeded {max_bytes} bytes; refusing to continue")
        h.update(chunk)
    return h.hexdigest(), size


def fetch_and_sha256(url: str, insecure: bool = False) -> tuple[str, int]:
    """GET `url`, return (sha256-hex, size). Streams to avoid loading large
    artifacts into memory. Default verifies TLS; `--insecure` for diagnostic
    runs only (never trust a no-verify result for real verification)."""
    ctx = ssl._create_unverified_context() if insecure else ssl.create_default_context()
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, context=ctx, timeout=120) as resp:
        if resp.status != 200:
            raise RuntimeError(f"unexpected HTTP {resp.status} for {url}")
        return sha256_stream(resp)


def atomic_write_json(path: Path, data: dict) -> None:
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
    ap.add_argument("--package", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--verify-report", default=None,
                    help="If --write is set, store this repo-relative path as `verify_report` on the entry.")
    ap.add_argument("--ledger-dir", default="docs/release/sha_ledger", type=Path)
    ap.add_argument("--repo-root", default=".", type=Path)
    ap.add_argument("--write", action="store_true",
                    help="Mutate the entry on successful match (default: report-only).")
    ap.add_argument("--insecure", action="store_true",
                    help="Disable TLS cert verification (DIAGNOSTIC ONLY — never trust the result).")
    args = ap.parse_args()

    repo_root = args.repo_root.resolve()
    ledger_path = (repo_root / args.ledger_dir / f"{args.channel}.json").resolve()
    if not ledger_path.is_file():
        print(f"ERROR: ledger file does not exist: {ledger_path}", file=sys.stderr)
        return 2

    with ledger_path.open() as f:
        ledger = json.load(f)

    entry_idx = None
    for i, e in enumerate(ledger.get("entries", [])):
        if e["package"] == args.package and e["version"] == args.version:
            entry_idx = i
            break
    if entry_idx is None:
        print(f"ERROR: no entry found for {args.channel}/{args.package}@{args.version}", file=sys.stderr)
        print(f"       (use ledger_add_entry.py first)", file=sys.stderr)
        return 2

    entry = ledger["entries"][entry_idx]
    sha_local = entry["sha256_local"]
    url = entry["registry_url"]
    print(f"=== ledger entry ===")
    print(f"  channel:    {args.channel}")
    print(f"  package:    {entry['package']}")
    print(f"  version:    {entry['version']}")
    print(f"  status:     {entry['status']}")
    print(f"  sha256_local: {sha_local}")
    print(f"  registry_url: {url}")
    print()
    print(f"=== downloading from registry ===")
    try:
        sha_registry, size_registry = fetch_and_sha256(url, insecure=args.insecure)
    except urllib.error.HTTPError as e:
        print(f"DOWNLOAD ERROR: HTTP {e.code} {e.reason}", file=sys.stderr)
        return 3
    except urllib.error.URLError as e:
        print(f"DOWNLOAD ERROR: {e.reason}", file=sys.stderr)
        return 3
    except Exception as e:
        print(f"DOWNLOAD ERROR: {type(e).__name__}: {e}", file=sys.stderr)
        return 3

    print(f"  sha256_registry: {sha_registry}")
    print(f"  size_registry: {size_registry} bytes (expected {entry['size_bytes']})")
    print()

    if sha_registry != sha_local:
        print(f"❌ DRIFT DETECTED ({args.channel}/{args.package}@{args.version})")
        print(f"   committed sha256_local:    {sha_local}")
        print(f"   registry sha256_registry:  {sha_registry}")
        print(f"   sizes:  local={entry['size_bytes']}  registry={size_registry}")
        print(f"   This is a §11 post-publish-drift defect. Triage per PUBLISH_PROTOCOL.md §12.",
              file=sys.stderr)
        return 1

    if size_registry != entry["size_bytes"]:
        # sha256 matched but size mismatches — extremely unlikely (different
        # bytes producing same sha is collisions territory), but flag it.
        print(f"⚠️  WARNING: sha256 matched but size differs "
              f"(local={entry['size_bytes']} bytes vs registry={size_registry} bytes)")

    print(f"✅ VERIFIED MATCH ({args.channel}/{args.package}@{args.version})")

    if args.write:
        now_utc = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        # Only mutate the relevant fields; preserve everything else.
        entry["sha256_registry"] = sha_registry
        entry["verified_at"] = now_utc
        if args.verify_report is not None:
            entry["verify_report"] = args.verify_report
        # Only bump status if it was `live`; `yanked` is preserved (yanked
        # versions can still be verified against the registry to confirm the
        # yanked bytes are still what we yanked, but their lifecycle status
        # remains yanked).
        if entry["status"] == "live":
            entry["status"] = "verified"
        ledger["entries"][entry_idx] = entry
        atomic_write_json(ledger_path, ledger)
        print(f"  ledger updated: {ledger_path.relative_to(repo_root)}")
        print(f"  status:         {entry['status']}")
        print(f"  verified_at:    {now_utc}")
    else:
        print(f"  (report-only run; rerun with --write to mutate the ledger entry)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
