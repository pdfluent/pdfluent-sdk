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

ONE GATE PER FLAVOUR, ONE ROUND FOR ALL OF THEM (#443)

The gate used to convert to PDF/A-2b and grade PDF/A-2b, and that was the only
conformance number this product had. Meanwhile the C header advertised
conversion to 1b and 3b, and a published website snippet calls
`convert_to_pdfa(PdfAProfile::A1b)`. Claims we never measured.

So the flavour is a parameter — `--flavour {1b,2b,2u,3b,3u}`, one baseline file
each, declared in `pdfa_flavours.py` — and `--all-flavours` measures every one
of them in a single invocation. A single round matters for more than
convenience: the five numbers then describe the same binary, the same corpus and
the same veraPDF on the same machine, so the difference between two of them is
the flavour and nothing else.

WHY THE BASELINE CARRIES A VERDICT PER DOCUMENT

A count is a weak baseline. 300/300 stays 300/300 when one document starts
failing and another starts passing, and that is a regression plus a windfall
reported as "no change". The per-document verdicts make that visible, and they
make the set itself checkable: a baseline missing a document it used to hold is
refused rather than quietly compared against a smaller corpus.

Older baselines that hold only counts are still read — the count comparison is
what they can support — so this gate does not go red on the day it lands.

Usage:
    pdfa_conformance_gate.py --corpus-dir /mnt/storagebox/corpus/general/govdocs
                             --binary target/release/xfa-test-runner
    pdfa_conformance_gate.py --all-flavours --corpus-dir … --update-baseline

Exit codes:
    0  every measured flavour is at or above its baseline
    1  regression at one or more flavours, or a document that used to convert
       now crashes
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
sys.path.insert(0, str(HERE))
from pdfa_flavours import (  # noqa: E402  one declaration of what a flavour is
    DEFAULT_FLAVOUR,
    FLAVOURS,
    baseline_path,
)

DEFAULT_SAMPLE_LIST = REPO / "benchmarks" / "pdfa" / "govdocs_sample_300.txt"
SAMPLE_LIST = DEFAULT_SAMPLE_LIST


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


def die(msg: str, code: int = 2) -> None:
    print(f"[pdfa-gate] {msg}", file=sys.stderr)
    sys.exit(code)


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
        help="where to read/write the baseline; defaults to the flavour's own file. "
        "Only valid for a single flavour: one path cannot hold five",
    )
    ap.add_argument(
        "--flavour",
        default=DEFAULT_FLAVOUR,
        choices=sorted(FLAVOURS),
        help=f"PDF/A flavour to convert to and grade against (default {DEFAULT_FLAVOUR})",
    )
    ap.add_argument(
        "--all-flavours",
        action="store_true",
        help="measure every declared flavour in this one round, on this one binary",
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

    # Module-level default stays for readability; the run uses whatever the
    # caller asked for, so a holdout set can be measured with the same code.
    global SAMPLE_LIST
    SAMPLE_LIST = args.sample_list

    flavours = sorted(FLAVOURS) if args.all_flavours else [args.flavour]
    if args.baseline is not None and len(flavours) > 1:
        die("--baseline names one file and --all-flavours measures five; pass one or the other")

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

    root_out = Path(args.output_dir)
    root_out.mkdir(parents=True, exist_ok=True)
    version = verapdf_version(args.verapdf)

    # One flavour writes where it always wrote, so the CI artefact paths and the
    # two jobs that name `measured.json` keep working; a round of five gives
    # each its own directory and a summary beside them.
    single = len(flavours) == 1

    failures: list[str] = []
    summary: dict[str, dict] = {}

    for flavour in flavours:
        out_dir = root_out if single else root_out / flavour
        out_dir.mkdir(parents=True, exist_ok=True)
        measured, per_document = measure(flavour, names, corpus, args, out_dir, version)
        summary[flavour] = measured

        target = args.baseline if args.baseline is not None else baseline_path(flavour)

        if args.update_baseline:
            record(target, measured)
            print(f"[pdfa-gate] baseline for {platform_key()} at {flavour} updated: {target}")
            continue

        problem = judge(flavour, target, measured, per_document)
        if problem:
            failures.append(problem)

    if not single:
        (root_out / "flavours.json").write_text(
            json.dumps({"platform": platform_key(), "verapdf": version, "flavours": summary},
                       indent=2, sort_keys=True)
            + "\n"
        )
        print(f"[pdfa-gate] {len(flavours)} flavours written to {root_out / 'flavours.json'}")

    if failures:
        for f in failures:
            print(f"[pdfa-gate] {f}", file=sys.stderr)
        sys.exit(1)
    if not args.update_baseline:
        print("[pdfa-gate] OK")


def measure(
    flavour: str,
    names: list[str],
    corpus: Path,
    args: argparse.Namespace,
    out_dir: Path,
    version: str,
) -> tuple[dict, dict[str, str]]:
    """Convert and validate the sample at one flavour. Returns the summary and
    the per-document verdicts."""
    db = out_dir / "results.sqlite"
    if db.exists():
        db.unlink()

    # The runner takes a directory; symlinks keep us from copying ~1 GB.
    with tempfile.TemporaryDirectory(prefix=f"pdfa-gate-{flavour}-") as staging:
        for n in names:
            os.symlink((corpus / n).resolve(), Path(staging) / n)

        env = dict(os.environ)
        # Measure the shipped configuration: no qpdf shell-out, because the
        # WASM build, the C ABI and the facade have none.
        env["PDFA_NO_EXTERNAL_REPAIR"] = "1"

        cmd = [
            str(Path(args.binary)), "run",
            "--corpus", staging,
            "--db", str(db),
            "--tests", "pdfa_convert",
            "--tier", "full",
            "--pdfa-level", flavour,
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
    rows = con.execute(
        "SELECT pdf_path, status FROM test_results "
        "WHERE test_name='pdfa_convert' ORDER BY pdf_path"
    ).fetchall()
    con.close()

    per_document = {Path(p).name: status for p, status in rows}
    total = sum(counts.values())
    passed = counts.get("pass", 0)
    measured = {
        "corpus": corpus_label(SAMPLE_LIST, len(names)),
        "platform": platform_key(),
        "sample_size": len(names),
        "measured_total": total,
        "pass": passed,
        "counts": counts,
        "documents": per_document,
        # Unlicensed runs carry the free-tier watermark, which the pipeline then
        # has to embed a font for — a different document than a licensed run
        # produces. Recording it keeps two numbers from being compared blind.
        "licensed": bool(
            os.environ.get("PDFLUENT_LICENSE_FILE") or os.environ.get("PDFLUENT_LICENSE_KEY")
        ),
        "verapdf": version,
        "flavour": flavour,
        "profile": FLAVOURS[flavour].iso,
    }
    (out_dir / "measured.json").write_text(json.dumps(measured, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in measured.items() if k != "documents"}, indent=2))
    return measured, per_document


def record(target: Path, measured: dict) -> None:
    doc = json.loads(target.read_text()) if target.is_file() else {"platforms": {}}
    clash = flavour_clash(doc, measured["flavour"])
    if clash:
        die(
            f"{target} already holds {clash} numbers and this run measured "
            f"{measured['flavour']}. `--flavour X --baseline <Y's file>` would overwrite "
            "one flavour's ledger with another's, and the two are indistinguishable "
            "afterwards. Name the right file, or drop --baseline and let the flavour "
            "choose it.",
            1,
        )
    doc.setdefault("platforms", {})[platform_key()] = measured
    doc["note"] = BASELINE_NOTE
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")


def flavour_clash(doc: dict, flavour: str) -> str | None:
    """The flavour a committed baseline says it holds, when that is not this one.

    Only reachable through `--baseline`, which is how a holdout run names its own
    file -- and how a typo names somebody else's. Baselines written before #443
    carry no `flavour` field and are not second-guessed here: absence is age, not
    disagreement. (codex, #443)
    """
    for entry in doc.get("platforms", {}).values():
        named = entry.get("flavour")
        if named and named != flavour:
            return named
    return None


def judge(flavour: str, target: Path, measured: dict, per_document: dict[str, str]) -> str | None:
    """Compare one flavour against its baseline. Returns the reason it is red,
    or None."""
    key = platform_key()
    if not target.is_file():
        die(f"no baseline at {target} for flavour {flavour}; run once with --update-baseline")
    doc = json.loads(target.read_text())
    clash = flavour_clash(doc, flavour)
    if clash:
        return (
            f"{flavour}: {target} holds {clash} numbers, so comparing this run against it "
            "would grade one flavour by another's ledger"
        )
    # From here on a problem is REPORTED rather than exited on, so that a round
    # of five says what happened to all five. A first run on a new platform used
    # to stop at the first flavour, which turned four unmeasured levels into a
    # single sentence about the first one.
    baseline = doc.get("platforms", {}).get(key)
    if baseline is None:
        return (
            f"{flavour}: no baseline recorded for platform {key!r} "
            f"(have: {sorted(doc.get('platforms', {}))}). Font substitution is "
            "OS-dependent, so a baseline from another platform would be wrong. "
            "Run once with --update-baseline on this host."
        )

    # THE LICENCE MODE IS PART OF THE MEASUREMENT, and until now nothing read it
    # back (codex, #443). An unlicensed run carries the free-tier watermark,
    # which the pipeline then has to embed a font for -- a different document
    # than a licensed run produces, on a different page. Recording the field and
    # never comparing it is the "compared blind" the field was added to prevent.
    if measured["licensed"] != baseline.get("licensed", measured["licensed"]):
        here = "licensed" if measured["licensed"] else "unlicensed"
        there = "licensed" if baseline.get("licensed") else "unlicensed"
        return (
            f"{flavour}: this run is {here} and the baseline is {there}. The free-tier "
            "watermark is a page the pipeline has to embed a font for, so the two runs "
            "converted different documents and neither number judges the other."
        )

    total = measured["measured_total"]
    passed = measured["pass"]
    if total != baseline["sample_size"]:
        return (
            f"{flavour}: measured {total} documents, expected {baseline['sample_size']} — "
            "the runner skipped some, so the comparison would be meaningless"
        )

    # The per-document half. A baseline written before #443 has no `documents`
    # key; it is judged on its count, which is all it can support, rather than
    # being called broken for being old.
    was = baseline.get("documents")
    if was:
        vanished = sorted(set(was) - set(per_document))
        appeared = sorted(set(per_document) - set(was))
        if vanished or appeared:
            print(
                f"[pdfa-gate] {flavour}: the measured set is not the baseline's set "
                f"({len(vanished)} missing, {len(appeared)} new; "
                f"first missing: {vanished[0] if vanished else '-'}, "
                f"first new: {appeared[0] if appeared else '-'})"
            )
            return (
                f"{flavour}: baseline covers {len(was)} documents and this run covers "
                f"{len(per_document)} — a comparison across two different sets is not a verdict"
            )
        regressed = sorted(
            name
            for name, status in was.items()
            if status == "pass" and per_document.get(name) != "pass"
        )
        if regressed:
            print(f"[pdfa-gate] {flavour}: REGRESSION on {len(regressed)} document(s)")
            for name in regressed[:20]:
                print(f"  {name}: pass -> {per_document.get(name)}")
            return f"{flavour}: {len(regressed)} document(s) that used to conform no longer do"

    expected = baseline["pass"]
    print(
        f"[pdfa-gate] {key} {flavour}: pass {passed}/{total}, "
        f"baseline {expected}/{baseline['sample_size']}"
    )
    if passed < expected:
        print(f"[pdfa-gate] {flavour}: REGRESSION: {expected - passed} document(s) stopped converting")
        for name, status in sorted(per_document.items()):
            if status != "pass":
                print(f"  fail: {name}")
        return f"{flavour}: {expected - passed} fewer documents conform than the baseline"
    if passed > expected:
        print(
            f"[pdfa-gate] {flavour}: {passed - expected} more documents convert than the "
            f"baseline. Re-run with --update-baseline to lock the improvement in."
        )
    return None


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
