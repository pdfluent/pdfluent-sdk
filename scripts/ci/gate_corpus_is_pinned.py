#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The gate corpus manifest describes one set of bytes, and says whose they are.

Offline, so it runs anywhere in a second. It checks the manifest is still the
thing the gate can rely on:

  * every entry carries a SHA-256, so a silently changed file fails instead of
    quietly changing what the gate measures;
  * every source is pinned to a 40-character commit, present in the URL it
    fetches from -- a branch name would let upstream move the corpus under us;
  * every source names a licence and a provenance line, because the reason no
    PDF is committed here is #215, and a manifest without provenance
    reintroduces exactly the question that issue asks;
  * no entry's PDF is tracked in git.

# FLOOR: gate corpus entries >= 500 -- and no more, without an edit here. This
# is a two-way ratchet. Downward is the failure everyone expects: a manifest
# that loses entries measures less while still reporting green. Upward matters
# just as much, because a gate whose input grows on its own is a gate whose
# baseline nobody chose -- the same shape as the queued job in #276, where
# nothing was red and nothing was running.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys

VLOER = 500
MANIFEST = pathlib.Path("corpus/GATE_CORPUS_MANIFEST.json")

def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, and a subprocess that
    inherits them acts on the real repository whatever directory you point it
    at. scripts/ci/test_no_test_can_touch_the_real_repo.py caught this one
    before it ran anywhere.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
# Whatever the manifest fetches from has to be reachable and content-addressed.
RAW = re.compile(r"^https://raw\.githubusercontent\.com/[^/]+/[^/]+/[0-9a-f]{40}/$")


def main() -> int:
    p = argparse.ArgumentParser()
    # A path, so the guard's own test can point it at a deliberately broken
    # copy. A guard nobody has tried to defeat is not a guard.
    p.add_argument("--manifest", default=str(MANIFEST))
    p.add_argument("--floor", type=int, default=VLOER)
    args = p.parse_args()
    manifest = pathlib.Path(args.manifest)
    vloer = args.floor

    if not manifest.is_file():
        print(f"FATAL: {manifest} is missing. The gate corpus is what the "
              "pull-request gates measure; without the manifest there is "
              "nothing pinned. (#276)", file=sys.stderr)
        return 1

    try:
        doc = json.loads(manifest.read_text())
    except json.JSONDecodeError as e:
        print(f"FATAL: {manifest} is not valid JSON: {e}", file=sys.stderr)
        return 1

    klachten: list[str] = []
    bronnen = doc.get("sources") or {}
    entries = doc.get("entries") or []

    if not bronnen:
        klachten.append("no sources declared")

    for sid, s in sorted(bronnen.items()):
        if not COMMIT.match(s.get("commit", "")):
            klachten.append(
                f"source `{sid}` is not pinned to a commit "
                f"(got {s.get('commit')!r}); a branch or tag lets upstream "
                "move the corpus without a commit here")
        if not RAW.match(s.get("raw_base", "")):
            klachten.append(
                f"source `{sid}` has raw_base {s.get('raw_base')!r}, which "
                "does not end in a pinned commit")
        elif s.get("commit", "") not in s.get("raw_base", ""):
            klachten.append(
                f"source `{sid}` fetches from a commit other than the one it "
                "declares")
        if not (s.get("licence") or "").strip():
            klachten.append(f"source `{sid}` names no licence (#215)")
        if len((s.get("provenance") or "").strip()) < 40:
            klachten.append(
                f"source `{sid}` has no usable provenance line (#215): every "
                "document this project reads has to say where it came from")

    namen: set[str] = set()
    for e in entries:
        n = e.get("name", "?")
        if not SHA256.match(e.get("sha256", "")):
            klachten.append(f"entry `{n}` has no usable sha256")
        if e.get("source") not in bronnen:
            klachten.append(f"entry `{n}` names undeclared source "
                            f"{e.get('source')!r}")
        if not isinstance(e.get("bytes"), int) or e["bytes"] <= 0:
            klachten.append(f"entry `{n}` has no byte count")
        if n in namen:
            klachten.append(f"entry `{n}` appears twice")
        namen.add(n)

    if doc.get("total") != len(entries):
        klachten.append(f"`total` says {doc.get('total')}, there are "
                        f"{len(entries)} entries")

    # #215: the manifest is text and publishes safely; the PDFs it names must
    # never become tracked files.
    try:
        getrackt = subprocess.run(
            ["git", "ls-files", "--", "corpus/gate/*.pdf", "corpus/gate/**/*.pdf"],
            capture_output=True, text=True, check=False, timeout=60,
            env=schone_omgeving()).stdout.split()
    except (OSError, subprocess.SubprocessError):
        getrackt = []
    if getrackt:
        klachten.append(
            f"{len(getrackt)} gate-corpus PDF(s) are tracked in git; the whole "
            "point of a manifest is that they are not (#215)")

    n = len(entries)
    if klachten:
        print(f"[gate-corpus] FATAL: {len(klachten)} problem(s) in {manifest}:",
              file=sys.stderr)
        for k in klachten[:30]:
            print(f"  {k}", file=sys.stderr)
        return 1

    if n < vloer:  # FLOOR
        print(f"[gate-corpus] FATAL: {n} entries, floor is {vloer}. The gate "
              "corpus lost entries. A smaller corpus still reports green, "
              "which is why this is checked rather than trusted. Rebuild the "
              "manifest, or lower the floor in a commit that says why.",
              file=sys.stderr)
        return 1

    if n > vloer:  # FLOOR, upward half of the ratchet
        print(f"[gate-corpus] FATAL: {n} entries, floor is {vloer}. The corpus "
              "grew without the floor being edited. That is not a smaller "
              "problem than shrinking: the gate is now measuring a set nobody "
              "chose, and every later comparison silently changes meaning. "
              f"Raise VLOER to {n} in the same commit as the manifest.",
              file=sys.stderr)
        return 1

    mb = doc.get("total_bytes", 0) / 1e6
    print(f"[gate-corpus] OK: {n} entries at the floor, {mb:.1f} MB, "
          f"{len(bronnen)} source(s), each pinned to a commit and named with "
          "a licence.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
