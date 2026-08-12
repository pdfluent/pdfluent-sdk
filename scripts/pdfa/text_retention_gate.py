#!/usr/bin/env python3
"""Second axis of the PDF/A gate: does the text survive the conversion?

Conformance alone is not a quality signal. Measured on the govdocs sample,
commit 0c0cb2087 raised the pass rate to 300/300 while one document
(002_002202) lost a third of its extractable text — it passed validation by
throwing content away. For an archival format that is worse than the original
violation, and a pass-rate gate cannot see it.

The metric is characters extracted by `mutool` (an independent extractor, so we
are not grading our own output with our own reader), whitespace stripped,
converted over source.

Why the bar is "no worse than the baseline" and not "100%":

  * Some documents never reached 100%. Five sit at 85–91% on master, from
    causes that predate this work. Demanding 100% would fail them forever and
    the gate would be ignored.
  * Retention above 100% is normal and good: conversion repairs broken
    encodings and adds ToUnicode, so more text becomes extractable than the
    source exposed. A ceiling of 100% would flag improvements as faults.

So the question this gate answers is the one that matters: did *this change*
take text away.

Usage:
    # record where things stand (run on the reference build)
    text_retention_gate.py --corpus-dir DIR --binary BIN --update-baseline

    # check a change against it
    text_retention_gate.py --corpus-dir DIR --binary BIN

Exit codes:
    0  no document lost text against the baseline
    1  at least one document regressed
    2  the gate could not run
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
SAMPLE_LIST = REPO / "benchmarks" / "pdfa" / "govdocs_sample_300.txt"
BASELINE = REPO / "benchmarks" / "pdfa" / "text_retention_baseline.json"

# Retention is noisy at the margin: extractors differ by a character or two on
# ligature and soft-hyphen handling. Only a real drop should fail the gate.
TOLERANCE_PP = 1.0


def die(msg: str, code: int = 2) -> None:
    print(f"[retention] {msg}", file=sys.stderr)
    sys.exit(code)


def chars(mutool: str, pdf: Path) -> int:
    """Non-whitespace characters mutool extracts, or -1 when it cannot read."""
    try:
        out = subprocess.run(
            [mutool, "draw", "-F", "txt", str(pdf)],
            capture_output=True,
            timeout=120,
        )
    except (subprocess.TimeoutExpired, OSError):
        return -1
    return len(b"".join(out.stdout.split()))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus-dir", required=True)
    ap.add_argument("--binary", default="target/release/xfa-test-runner")
    ap.add_argument("--mutool", default="mutool")
    ap.add_argument("--update-baseline", action="store_true")
    ap.add_argument("--only", help="comma-separated basenames, for a quick check")
    args = ap.parse_args()

    binary = Path(args.binary)
    if not binary.is_file():
        die(f"runner not found at {binary}")
    if subprocess.run([args.mutool, "-v"], capture_output=True).returncode not in (0, 1):
        die(f"mutool not usable at {args.mutool}")

    corpus = Path(args.corpus_dir)
    names = [n.strip() for n in SAMPLE_LIST.read_text().splitlines() if n.strip()]
    if args.only:
        wanted = {n if n.endswith(".pdf") else f"{n}.pdf" for n in args.only.split(",")}
        names = [n for n in names if n in wanted]

    env = dict(os.environ)
    env["PDFA_NO_EXTERNAL_REPAIR"] = "1"

    measured: dict[str, float] = {}
    unreadable: list[str] = []
    with tempfile.TemporaryDirectory(prefix="retention-") as tmp:
        for name in names:
            src = corpus / name
            if not src.is_file():
                die(f"missing from corpus: {name}")
            out = Path(tmp) / name
            rc = subprocess.run(
                [str(binary), "convert-one", "-p", str(src), "-o", str(out)],
                capture_output=True,
                env=env,
            ).returncode
            if rc != 0 or not out.is_file():
                # Not this gate's business: the conformance gate reports
                # conversion failures. Record it so the two can be reconciled.
                unreadable.append(name)
                continue
            s = chars(args.mutool, src)
            o = chars(args.mutool, out)
            if s <= 0:
                # No extractable text in the source (scans, pure vector art).
                # Nothing to retain, nothing to judge.
                continue
            measured[name] = round(100.0 * o / s, 1)

    if args.update_baseline:
        # Per-platform documents map: font substitution differs per host, so a
        # baseline recorded on one OS says nothing about another (the same
        # reason conformance_baseline.json is keyed by platform).
        #
        # The converter is nondeterministic on a handful of documents (its
        # fallback glyph choices used to follow HashSet iteration order), so a
        # single master sample is itself noisy — measured swings of up to 5pp
        # between identical master runs. Re-running --update-baseline merges
        # per-document minima: the gate then compares against the worst
        # extraction the reference build ever produced, which is the honest
        # bar for "did this change take text away".
        existing: dict = {}
        if BASELINE.is_file():
            existing = json.loads(BASELINE.read_text())
        platforms = existing.get("platforms", {})
        plat = platform.system().lower()
        previous = platforms.get(plat, {})
        merged = dict(previous)
        for name, value in measured.items():
            merged[name] = min(value, previous.get(name, value))
        platforms[plat] = merged
        # Rebuild only the keys this script owns. Anything else a human added —
        # floor_notes explaining an accepted per-document floor, for instance —
        # has to survive, or re-recording the baseline silently deletes the
        # reasoning behind it. (It did, once.)
        out = dict(existing)
        out.update(
            {
                "note": (
                        "Characters mutool extracts, converted over source, per "
                        "document, per platform. The gate fails on a drop against "
                        "these numbers, not against 100%: several documents never "
                        "reached 100% and repaired encodings legitimately push "
                        "others above it. Values are per-document minima over "
                        "repeated runs of the reference build, because the "
                    "converter is not fully deterministic on every document."
                ),
                "tolerance_pp": TOLERANCE_PP,
                "platforms": platforms,
            }
        )
        BASELINE.write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
        print(
            f"[retention] baseline written: {BASELINE} ({len(measured)} documents, {plat}, merged)"
        )
        return

    if not BASELINE.is_file():
        die(f"no baseline at {BASELINE}; run once with --update-baseline")
    baseline_json = json.loads(BASELINE.read_text())
    # Per-platform shape, with the original flat shape as fallback.
    base = baseline_json.get("platforms", {}).get(platform.system().lower())
    if base is None:
        base = baseline_json.get("documents", {})

    regressed = []
    for name, now in sorted(measured.items()):
        was = base.get(name)
        if was is None:
            continue
        if now < was - TOLERANCE_PP:
            regressed.append((name, was, now))

    print(f"[retention] {len(measured)} documents measured, {len(regressed)} regressed")
    if unreadable:
        print(f"[retention] {len(unreadable)} did not convert: {', '.join(unreadable[:5])}")
    for name, was, now in regressed:
        print(f"  {name}: {was}% -> {now}%  ({now - was:+.1f}pp)")

    if regressed:
        print("[retention] FAIL — the change removed text from these documents.")
        sys.exit(1)
    print("[retention] OK")


if __name__ == "__main__":
    main()
