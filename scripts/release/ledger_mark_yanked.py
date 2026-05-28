#!/usr/bin/env python3
"""
ledger_mark_yanked.py — mark a SHA ledger entry as yanked.

Sets `status=yanked` + `yanked_at` + `yanked_reason` on an existing entry.
DOES NOT modify the original `sha256_local`, `sha256_registry`,
`published_at`, or `audit_report` — those remain the immutable record of
what was actually published, even after yanking. The entry stays in the
ledger forever; deletion is forbidden by R6-3 in the README.

Idempotent: if the entry is already yanked, leaves it alone (preserves the
original yanked_at and yanked_reason — re-running this script after a yank
must not silently rewrite history).

Usage:
    ledger_mark_yanked.py \\
        --channel <crates_io|npm|pypi|maven|nuget|wasm|binary|gitlab> \\
        --package <name> \\
        --version <semver> \\
        --reason <short string> \\
        [--remediation-report <path-to-incident-md>] \\
        [--ledger-dir docs/release/sha_ledger]

Pre-conditions (operator MUST satisfy before invoking this script):
  - The actual registry yank has already been executed (the script does NOT
    perform the yank; it only records that the operator did).
  - A remediation report exists under `benchmarks/runs/remediation/`
    documenting the yank per PUBLISH_PROTOCOL.md §12.
  - The yank reason satisfies the yanking-policy preconditions in §13.

Exit codes:
    0  marked yanked (or already yanked → idempotent)
    1  argv/path/precondition error
    2  entry not found
    3  reason missing / empty (R5/R6 require a non-empty reason on every yank)

stdlib-only.
"""
from __future__ import annotations
import argparse
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

CHANNELS = ("crates_io", "npm", "pypi", "maven", "nuget", "wasm", "binary", "gitlab")


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
    ap.add_argument("--reason", required=True,
                    help="Short string explaining why the version was yanked (e.g. "
                         "'licence-file missing from tarball').")
    ap.add_argument("--remediation-report", default=None,
                    help="Optional repo-relative path to the remediation incident report.")
    ap.add_argument("--ledger-dir", default="docs/release/sha_ledger", type=Path)
    ap.add_argument("--repo-root", default=".", type=Path)
    args = ap.parse_args()

    if not args.reason.strip():
        print("ERROR: --reason cannot be empty.", file=sys.stderr)
        return 3

    repo_root = args.repo_root.resolve()
    ledger_path = (repo_root / args.ledger_dir / f"{args.channel}.json").resolve()
    if not ledger_path.is_file():
        print(f"ERROR: ledger file does not exist: {ledger_path}", file=sys.stderr)
        return 1

    with ledger_path.open() as f:
        ledger = json.load(f)

    entry_idx = None
    for i, e in enumerate(ledger.get("entries", [])):
        if e["package"] == args.package and e["version"] == args.version:
            entry_idx = i
            break
    if entry_idx is None:
        print(f"ERROR: no entry found for {args.channel}/{args.package}@{args.version}", file=sys.stderr)
        return 2

    entry = ledger["entries"][entry_idx]
    if entry["status"] == "yanked":
        print(f"OK (idempotent): entry already yanked")
        print(f"  yanked_at:     {entry['yanked_at']}")
        print(f"  yanked_reason: {entry['yanked_reason']}")
        return 0

    # Construct the reason string. If the operator gave a remediation report
    # path, append the reference so the entry is self-contained for any
    # later auditor reading just the ledger.
    reason = args.reason.strip()
    if args.remediation_report:
        reason = f"{reason} (see {args.remediation_report})"

    now_utc = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    entry["status"] = "yanked"
    entry["yanked_at"] = now_utc
    entry["yanked_reason"] = reason
    ledger["entries"][entry_idx] = entry
    atomic_write_json(ledger_path, ledger)

    print(f"OK: marked yanked {args.channel}/{args.package}@{args.version}")
    print(f"  yanked_at:     {now_utc}")
    print(f"  yanked_reason: {reason}")
    print(f"  ledger:        {ledger_path.relative_to(repo_root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
