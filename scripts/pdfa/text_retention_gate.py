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
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
DEFAULT_SAMPLE_LIST = REPO / "benchmarks" / "pdfa" / "govdocs_sample_300.txt"
SAMPLE_LIST = DEFAULT_SAMPLE_LIST
BASELINE = REPO / "benchmarks" / "pdfa" / "text_retention_baseline.json"

# Retention is noisy at the margin: extractors differ by a character or two on
# ligature and soft-hyphen handling. Only a real drop should fail the gate.
TOLERANCE_PP = 1.0


def die(msg: str, code: int = 2) -> None:
    print(f"[retention] {msg}", file=sys.stderr)
    sys.exit(code)


# Extraction counts, keyed on the bytes extracted from.
#
# Both halves of this gate are cacheable, for different reasons. chars(source)
# depends only on the input document, and the holdout corpus does not change --
# those thousand mutool runs give the same answer forever. chars(output) depends
# on the converter, but conversion is byte-reproducible (measured 2026-08-20:
# identical output from two runs seconds apart), so a document a round of fixes
# did not touch produces identical bytes and its count still holds.
#
# Keyed on content, never on path: identical bytes deserve one entry, and a file
# that changed must miss even if its name did not.
_CACHE_DIR = Path(os.environ.get("RETENTION_CACHE_DIR", "/mnt/storagebox/pdfa-textcounts"))
_CACHE_ON = os.environ.get("RETENTION_CACHE", "on").lower() not in ("off", "0", "false")


def _digest(pdf: Path) -> str | None:
    try:
        h = hashlib.sha256()
        with pdf.open("rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


def chars(mutool: str, pdf: Path) -> int:
    """Non-whitespace characters mutool extracts, or -1 when it cannot read."""
    key = _digest(pdf) if _CACHE_ON else None
    entry = _CACHE_DIR / key[:2] / f"{key}.txt" if key else None

    if entry is not None and entry.is_file():
        try:
            cached = int(entry.read_text().strip())
            # -1 means mutool could not read it. That can be a broken install
            # rather than a property of the document, so it is never replayed --
            # a cached failure would quietly become a permanent verdict.
            if cached >= 0:
                return cached
        except (OSError, ValueError):
            pass

    try:
        out = subprocess.run(
            [mutool, "draw", "-F", "txt", str(pdf)],
            capture_output=True,
            timeout=120,
        )
    except (subprocess.TimeoutExpired, OSError):
        return -1
    count = len(b"".join(out.stdout.split()))

    if entry is not None and count >= 0:
        try:
            entry.parent.mkdir(parents=True, exist_ok=True)
            tmp = entry.with_suffix(".part")
            tmp.write_text(str(count))
            tmp.replace(entry)
        except OSError:
            pass  # an unwritable cache may slow this down, never break it

    return count


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--sample-list",
        type=Path,
        default=DEFAULT_SAMPLE_LIST,
        help="file of PDF basenames, one per line; defaults to the committed 300",
    )
    ap.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="where to read/write the baseline; defaults to the one beside the sample list",
    )
    ap.add_argument("--corpus-dir", required=True)
    ap.add_argument("--binary", default="target/release/xfa-test-runner")
    ap.add_argument("--mutool", default="mutool")
    ap.add_argument("--update-baseline", action="store_true")
    ap.add_argument("--only", help="comma-separated basenames, for a quick check")
    args = ap.parse_args()

    # Module-level defaults stay for readability; the run uses whatever the
    # caller asked for, so a holdout set can be measured with the same code.
    global SAMPLE_LIST, BASELINE
    SAMPLE_LIST = args.sample_list
    BASELINE = args.baseline if args.baseline else BASELINE

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
