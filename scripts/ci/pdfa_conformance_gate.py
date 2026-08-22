#!/usr/bin/env python3
"""PDF/A conformance gate: convert a fixed corpus sample, validate with veraPDF,
fail on any drop against the committed baseline.

This gate is the thing that was missing. PDF/A conformance on this corpus was
once at 100%; two separate regressions landed and neither was noticed for
months, because nothing measured conformance on real documents between
releases. A unit test cannot catch this class of defect — only running the
converter over a body of real-world PDFs and asking an external validator can.

Why a *fixed* sample, listed file by file in `govdocs_sample_300.txt`: a fresh
random draw each run makes the number move for reasons that have nothing to do
with the code, and a gate that moves on its own is a gate people learn to
ignore.

Why the baseline is keyed by operating system: font embedding substitutes a
*system* font for every non-embedded font, and the search paths differ per OS
(URW/Liberation on Debian, the real Helvetica and Times on macOS). Different
substitute, different glyph widths, different §6.2.11.5 outcome — measured, the
same commit scores 286/300 on macOS and 275/300 on the CI host. One absolute
number would make the gate fail on a machine change rather than a code change.

Usage:
    pdfa_conformance_gate.py --corpus-dir /mnt/storagebox/corpus/general/govdocs
                             --binary target/release/xfa-test-runner

Exit codes:
    0  pass count >= baseline
    1  regression, or a document that used to convert now crashes
    2  the gate could not run (missing corpus, binary, or veraPDF)
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
DEFAULT_SAMPLE_LIST = REPO / "benchmarks" / "pdfa" / "govdocs_sample_300.txt"
SAMPLE_LIST = DEFAULT_SAMPLE_LIST
BASELINE = REPO / "benchmarks" / "pdfa" / "conformance_baseline.json"


def die(msg: str, code: int = 2) -> None:
    print(f"[pdfa-gate] {msg}", file=sys.stderr)
    sys.exit(code)


def corpus_label(sample_list, aantal: int) -> str:
    """Welk monster is gedraaid, afgeleid van de lijst zelf.

    Dit stond hard op "govdocs (fixed 300-document sample)", ongeacht welke
    lijst je meegaf. De holdoutrun van 1000 documenten schreef dus in zijn eigen
    `measured.json` dat hij uit de 300 kwam — precies het monster waarop vijf
    ronden reparatiewerk zijn geoptimaliseerd, en precies het onderscheid waar
    de holdout voor bestaat. Een cijfer dat zichzelf verkeerd etiketteert is
    erger dan geen cijfer.
    """
    return f"govdocs ({sample_list.name}, {aantal} documents)"


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
    ap.add_argument("--corpus-dir", required=True, help="directory holding the sampled PDFs")
    ap.add_argument("--binary", default="target/release/xfa-test-runner")
    ap.add_argument("--verapdf", default="/usr/local/bin/verapdf")
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--output-dir", default="benchmarks/runs/pdfa_gate")
    ap.add_argument(
        "--update-baseline",
        action="store_true",
        help="write the measured result as the new baseline instead of comparing",
    )
    args = ap.parse_args()

    # Module-level defaults stay for readability; the run uses whatever the
    # caller asked for, so a holdout set can be measured with the same code.
    global SAMPLE_LIST, BASELINE
    SAMPLE_LIST = args.sample_list
    BASELINE = args.baseline if args.baseline else BASELINE

    binary = Path(args.binary)
    if not binary.is_file():
        die(f"test runner not found at {binary} — build it first")
    if not Path(args.verapdf).exists():
        die(f"veraPDF not found at {args.verapdf}; the gate is meaningless without it")
    if not SAMPLE_LIST.is_file():
        die(f"sample list missing: {SAMPLE_LIST}")

    corpus = Path(args.corpus_dir)
    names = [n.strip() for n in SAMPLE_LIST.read_text().splitlines() if n.strip()]
    missing = [n for n in names if not (corpus / n).is_file()]
    if missing:
        die(
            f"{len(missing)} of {len(names)} sampled files are absent from {corpus} "
            f"(first: {missing[0]}) — the gate would silently measure a different set"
        )

    out_dir = Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    db = out_dir / "results.sqlite"
    if db.exists():
        db.unlink()

    # The runner takes a directory; symlinks keep us from copying ~1 GB.
    with tempfile.TemporaryDirectory(prefix="pdfa-gate-") as staging:
        for n in names:
            os.symlink((corpus / n).resolve(), Path(staging) / n)

        env = dict(os.environ)
        # Measure the shipped configuration: no qpdf shell-out, because the
        # WASM build, the C ABI and the facade have none.
        env["PDFA_NO_EXTERNAL_REPAIR"] = "1"

        cmd = [
            str(binary), "run",
            "--corpus", staging,
            "--db", str(db),
            "--tests", "pdfa_convert",
            "--tier", "full",
            "-j", str(args.jobs),
            "--timeout", str(args.timeout),
            "--verapdf-path", args.verapdf,
        ]
        print("[pdfa-gate] " + " ".join(cmd))
        proc = subprocess.run(cmd, env=env)
        if proc.returncode != 0:
            print(f"[pdfa-gate] runner exited {proc.returncode}", file=sys.stderr)

    if not db.exists():
        die("runner produced no results database")

    con = sqlite3.connect(db)
    counts = dict(
        con.execute(
            "SELECT status, COUNT(*) FROM test_results WHERE test_name='pdfa_convert' GROUP BY status"
        ).fetchall()
    )
    failures = [
        r[0]
        for r in con.execute(
            "SELECT pdf_path FROM test_results "
            "WHERE test_name='pdfa_convert' AND status!='pass' ORDER BY pdf_path"
        ).fetchall()
    ]
    con.close()

    total = sum(counts.values())
    passed = counts.get("pass", 0)
    measured = {
        "corpus": corpus_label(SAMPLE_LIST, len(names)),
        "platform": platform_key(),
        "sample_size": len(names),
        "measured_total": total,
        "pass": passed,
        "counts": counts,
        # Unlicensed runs carry the free-tier watermark, which the pipeline then
        # has to embed a font for — a different document than a licensed run
        # produces. Recording it keeps two numbers from being compared blind.
        "licensed": bool(
            os.environ.get("PDFLUENT_LICENSE_FILE") or os.environ.get("PDFLUENT_LICENSE_KEY")
        ),
        "verapdf": verapdf_version(args.verapdf),
        "profile": "PDF/A-2b",
    }
    (out_dir / "measured.json").write_text(json.dumps(measured, indent=2) + "\n")
    print(json.dumps(measured, indent=2))

    key = platform_key()

    if args.update_baseline:
        doc = json.loads(BASELINE.read_text()) if BASELINE.is_file() else {"platforms": {}}
        doc.setdefault("platforms", {})[key] = measured
        doc["note"] = BASELINE_NOTE
        BASELINE.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
        print(f"[pdfa-gate] baseline for {key} updated: {BASELINE}")
        return

    if not BASELINE.is_file():
        die(f"no baseline at {BASELINE}; run once with --update-baseline")
    doc = json.loads(BASELINE.read_text())
    baseline = doc.get("platforms", {}).get(key)
    if baseline is None:
        die(
            f"no baseline recorded for platform {key!r} "
            f"(have: {sorted(doc.get('platforms', {}))}). Font substitution is "
            "OS-dependent, so a baseline from another platform would be wrong. "
            "Run once with --update-baseline on this host.",
            1,
        )

    if total != baseline["sample_size"]:
        die(
            f"measured {total} documents, expected {baseline['sample_size']} — "
            "the runner skipped some, so the comparison would be meaningless",
            1,
        )

    expected = baseline["pass"]
    print(f"[pdfa-gate] {key}: pass {passed}/{total}, baseline {expected}/{baseline['sample_size']}")
    if passed < expected:
        print(f"[pdfa-gate] REGRESSION: {expected - passed} document(s) stopped converting")
        for f in failures:
            print(f"  fail: {Path(f).name}")
        sys.exit(1)
    if passed > expected:
        print(
            f"[pdfa-gate] {passed - expected} more documents convert than the baseline. "
            f"Re-run with --update-baseline to lock the improvement in."
        )
    print("[pdfa-gate] OK")


BASELINE_NOTE = (
    "Per-OS because font embedding substitutes a system font and the available "
    "fonts differ per platform, which changes glyph widths and therefore "
    "6.2.11.5 conformance. Raise a number only after a measured improvement on "
    "that same platform; never to turn a red gate green."
)


def platform_key() -> str:
    """Coarse OS key. Deliberately not versioned: the font *sets* differ between
    macOS and Linux, not meaningfully between releases of either."""
    return platform.system().lower()


def verapdf_version(path: str) -> str:
    try:
        out = subprocess.run(
            [path, "--version"], capture_output=True, text=True, timeout=120
        )
        return (out.stdout or out.stderr).strip().splitlines()[0]
    except Exception as e:  # noqa: BLE001 - version is informational only
        return f"unknown ({e})"


if __name__ == "__main__":
    main()
