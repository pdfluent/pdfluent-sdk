#!/usr/bin/env python3
"""The fifty go into the image by hash, or the build stops.

scripts/infra/build_ci_snapshot.sh recovers the fifty SSIM documents from git
history and checks each against the SHA-256 in corpus/SSIM_GATE_MANIFEST.json.
That check is the only thing standing between an image and a corpus nobody can
identify: the documents are IRS and USCIS forms, reissued yearly, so a file of
the right name proves nothing.

The failure this prevents is quiet. An image built from almost-right documents
produces an SSIM gate that reports a difference against a corpus that was never
the calibrated one -- and the difference reads as a regression in the renderer.

Checked here rather than on the instance, because discovering it there costs a
server and a build.
"""
from __future__ import annotations

import hashlib
import os
import json
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = REPO / "corpus" / "SSIM_GATE_MANIFEST.json"
SCRIPT = REPO / "scripts" / "infra" / "build_ci_snapshot.sh"

fouten: list[str] = []


def expect(wat: str, ok: bool, detail: str = "") -> None:
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fouten.append(wat)


def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, and a subprocess
    inherits them -- so a git command meant for one place operates on the real
    repository instead. On 25-08-2026 that set `core.bare = true` here and
    stopped all thirty worktrees. scripts/ci/een_echte_repo... refused this file
    without it, which is the guard doing exactly its job on the change that
    added the guard's own subject.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*a: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *a], cwd=REPO, capture_output=True,
                          env=schone_omgeving())


def blob_van(pad: str) -> bytes | None:
    if git("cat-file", "-e", f"HEAD:{pad}").returncode == 0:
        return git("cat-file", "blob", f"HEAD:{pad}").stdout
    c = git("log", "--all", "--format=%H", "--diff-filter=D", "-n1", "--", pad).stdout.decode().strip()
    if not c:
        return None
    r = git("cat-file", "blob", f"{c}^:{pad}")
    return r.stdout if r.returncode == 0 else None


def main() -> int:
    if not MANIFEST.is_file() or not SCRIPT.is_file():
        print(f"SKIPPED (not a pass): {MANIFEST.name} or {SCRIPT.name} is missing, "
              "so nothing was checked", file=sys.stderr)
        return 3

    entries = json.loads(MANIFEST.read_text())["entries"]
    print("the snapshot can recover the fifty")

    expect(f"the manifest still lists fifty ({len(entries)})", len(entries) == 50,
           f"{len(entries)} entries")
    zonder = [e["file"] for e in entries if not e.get("sha256")]
    expect("every entry carries a sha256", not zonder,
           f"{len(zonder)} without: {zonder[:3]}")

    hersteld, mismatch, weg = 0, [], []
    for e in entries:
        blob = blob_van(e["source"])
        if blob is None:
            weg.append(e["file"])
            continue
        if e.get("sha256") and hashlib.sha256(blob).hexdigest() != e["sha256"]:
            mismatch.append(e["file"])
            continue
        hersteld += 1

    expect("all fifty are still reachable from git history", not weg,
           f"unreachable: {weg[:3]}")
    expect("and every one matches its recorded hash", not mismatch,
           f"mismatched: {mismatch[:3]}")
    expect(f"so the build would recover {len(entries)} of {len(entries)}",
           hersteld == len(entries), f"recovered {hersteld}")

    # The script must FAIL on a mismatch rather than ship a partial image. Read
    # from its source: a build that warns and continues is how an image ends up
    # holding documents nobody can identify.
    bron = SCRIPT.read_text()
    expect("the build script exits non-zero when a document cannot be established",
           "raise SystemExit(1)" in bron, "no hard failure found in the recovery step")
    expect("and it does not copy them from a directory on a machine",
           not re.search(r"(cp|rsync|scp)\s+[^\n]*ssim", bron, re.I),
           "a copy of a copy cannot be checked against the manifest")

    print(f"\n  7 assertion(s) ran, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
