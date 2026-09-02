#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""PDF/A gate for the pipeline that blocks a merge: it grades what the
converter *produced*, never what it was fed.

WHAT WAS HERE BEFORE

`.github/workflows/verapdf.yml` pointed veraPDF at `corpus/**/*.pdf` and posted
the result on the pull request. Those five files are XFA *input* fixtures --
`f1040`, `fw7`, `sf15`, `sf181`, `PDFBOX-4322-3`. They are ordinary government
forms. They are not PDF/A and were never meant to be, so the honest answer to
the question the job asked is "no", five times over, for ever.

It did not even get that far. The wrapper passed `--profile 2b`, and veraPDF's
`--profile` takes a path to a validation profile *file*; the flavour flag is
`-f`. So veraPDF printed its usage text and exited, the wrapper failed to find
a verdict in it and counted five `ERROR`s, and the workflow -- which called the
wrapper without `--ci` -- exited 0 and posted a table reading **0%** under a
green tick. Measured on 31-08-2026 with veraPDF 1.28.2: pass rate 0, exit
status 0.

Three defects stacked into one shape: the wrong documents, a tool that never
ran, and a gate with no failing branch. The first two were invisible because of
the third.

WHAT THIS MEASURES INSTEAD

For each fixture: run it through `pdfa::convert_bytes` -- reached by the
`pdfa_convert_real` example, and only that one; `convert_pdfa.rs` hand-rolls a
fixup sequence that has drifted from the shipping pipeline and reports numbers
no release ever produced -- then validate the OUTPUT against PDF/A-2b.

TWO AXES, BECAUSE ONE OF THEM CANNOT SEE DATA LOSS

Conformance alone rewards deletion. A fixup that drops the content stream of a
page removes every violation on it, and veraPDF says "compliant" about a blank
sheet with total conviction. That is not hypothetical here: a single backslash
in a text-repair pass once cost the remainder of a page, and the conformance
number went *up*.

So the second axis is text retention -- non-whitespace characters `mutool`
extracts from the output over the same count from the source. `mutool` is an
independent reader, so we are not grading our own output with our own parser.
Above 100% is normal and good: conversion repairs encodings and adds
`/ToUnicode`, so more text becomes extractable than the source exposed.

A DELIBERATE TWO-WAY RATCHET

Every number below is checked for equality, not for "at least". A floor that
only fails downward stops being a baseline: the number drifts up for reasons
nobody looked at, and the next drop lands on ground that was never chosen. So a
rise fails too, and the fix for a rise is to edit the number here and say what
moved it. That edit is the whole point -- it is the moment somebody looks.

WHY THE FLOORS ARE KEYED BY OPERATING SYSTEM

Font embedding substitutes a *system* font for every non-embedded font and the
available fonts differ per platform (URW/Liberation on Debian, the real
Helvetica and Times on macOS). Different substitute, different glyph widths,
different 6.2.11.5 outcome. The sibling corpus gate measured the same commit at
286/300 on macOS and 275/300 on the CI host. One absolute number would fail on
a machine change rather than on a code change.

Usage:
    pdfa_output_conformance_gate.py [--converter PATH] [--verapdf PATH]
                                    [--mutool PATH] [--out DIR]

Exit codes:
    0  every number matches the floor for this platform
    1  a number moved, in either direction, a tool disagreed with itself, or
       mutool could not read a file it was handed -- an unreadable output is
       a verdict about the output, not a missing tool (codex, #1617)
    2  the gate could not run at all (missing converter, missing veraPDF)
    3  did not judge, and says so: no floor for this platform, or no mutool
       installed, so retention was never measured
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent

# The conversion inputs. These are the same five files the deleted workflow
# validated directly; the difference is that here they are what the converter
# is *fed*, and the verdict is passed on what comes out.
FIXTURES = sorted((REPO / "corpus").glob("*.pdf"))

# FLOOR: fixtures >= 5 — a glob that finds nothing converts nothing, validates
# nothing and reports a clean run in exactly the same words as a real one. That
# is how the deleted workflow's `if [ -z "$(find corpus ...)" ]` branch wrote a
# 0-document summary and exited 0.
MINIMUM_FIXTURES = 5

# Retention is noisy at the margin -- extractors differ by a character or two on
# ligatures and soft hyphens -- but "the output kept at least as much text as
# the source" is not a marginal question. Measured on macOS 31-08-2026 the five
# fixtures land between 100.8% and 103.3%, so a document that drops below 100%
# lost something real.
RETENTION_MIN_PCT = 100.0

# FLOOR: per platform, the exact counts this gate must reproduce.
#
#   converted  — conversions that returned bytes at all
#   conformant — outputs veraPDF calls PDF/A-2b compliant
#   retained   — outputs in which >= RETENTION_MIN_PCT of the SOURCE's characters
#                are still present, in order. Not a length ratio: see
#                behouden_deel(). Measured on darwin, 01-09-2026, veraPDF 1.28.2
#                and mutool from mupdf-tools, on the five corpus fixtures:
#
#                  fixture                len %   kept %
#                  PDFBOX-4322-3.pdf     103.31   100.0
#                  f1040.pdf             100.90   100.0
#                  fw7.pdf               101.01   100.0
#                  sf15.pdf              100.82   100.0
#                  sf181.pdf             101.35   100.0
#
#                The surplus is real and additive -- every source character
#                survives -- which is why the threshold needs no tolerance and
#                why a length ratio could never have told the two apart.
#
# Raise or lower a number only after a measured change on that same platform,
# in the same commit as the change that moved it, and never to turn a red gate
# green.
FLOORS: dict[str, dict[str, int]] = {
    "darwin": {"fixtures": 5, "converted": 5, "conformant": 5, "retained": 5},
}

# Platforms this gate BLOCKS on. A platform with no floor cannot block: it has
# no number to compare against, so `judge()` would fail every run on evidence it
# does not have -- which is how a gate becomes a permanent red nobody reads.
#
# `linux` is deliberately absent. The blocking step runs on the Hetzner image,
# and the floor for it has never been measured, because the step it lives in was
# skipped on every run so far: `workspace::test` fails ahead of it and the step
# carried no `if: ${{ !cancelled() }}`. Both are fixed in this change, so a
# Linux run will now produce the numbers.
#
# To add it, in one commit, with the run it came from named in the message:
#
#   1. read the `[pdfa-output] {...}` line from a Linux run of this script
#   2. add "linux": {...} to FLOORS with exactly those counts
#   3. add "linux" to BLOKKEERT
#
# Never the other way round: a floor invented on a different platform is worse
# than none, because it looks measured. (codex, #1617)
BLOKKEERT: frozenset[str] = frozenset({"darwin"})

PROFILE = "2b"


def die(msg: str, code: int = 2) -> None:
    print(f"[pdfa-output] FATAL: {msg}", file=sys.stderr)
    sys.exit(code)


def platform_key() -> str:
    """Coarse OS key. Deliberately not versioned: the font *sets* differ between
    macOS and Linux, not meaningfully between releases of either."""
    return platform.system().lower()


def verapdf_verdict(verapdf: str, pdf: Path) -> tuple[bool, str]:
    """Is this file PDF/A-2b compliant, according to veraPDF.

    `-f`, not `--profile`. `--profile` wants a path to a profile file, and
    handing it a flavour name makes veraPDF print its usage text and exit --
    which the deleted wrapper stored as a report and counted as an "error".

    A missing `validationResult` is treated as NOT compliant and named as such.
    veraPDF stops early on content it cannot parse, and a run that stopped early
    has fewer findings than one that finished -- which reads like an improvement
    if you only count failures.
    """
    try:
        proc = subprocess.run(
            [verapdf, "-f", PROFILE, "--format", "json", str(pdf)],
            capture_output=True,
            text=True,
            timeout=300,
        )
    except (subprocess.TimeoutExpired, OSError) as e:
        return False, f"veraPDF did not run: {e}"

    try:
        report = json.loads(proc.stdout)["report"]
        job = report["jobs"][0]
    except (ValueError, KeyError, IndexError):
        head = (proc.stdout or proc.stderr or "").strip().splitlines()[:1]
        return False, f"no parsable report: {head[0] if head else 'no output'}"

    results = job.get("validationResult")
    if not results:
        exc = job.get("taskException") or job.get("exceptionMessage") or "no verdict"
        return False, f"veraPDF returned no verdict ({exc})"
    return bool(results[0].get("compliant")), ""


def tekst(mutool: str, pdf: Path) -> bytes | None:
    """The text mutool extracts, whitespace removed. None when it cannot read."""
    try:
        out = subprocess.run(
            [mutool, "draw", "-F", "txt", str(pdf)],
            capture_output=True,
            timeout=300,
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    if out.returncode != 0:
        # A mutool that FAILS is not a document with no text, and the two were
        # indistinguishable here: a broken binary returned empty output, every
        # fixture read as "source exposes no extractable text", and all five
        # counted as retained. That is a green gate on a measurement that never
        # happened -- which I introduced while fixing the missing-mutool case,
        # having only handled the binary being absent.
        return None
    return b"".join(out.stdout.split())


class Onleesbaar(Exception):
    """mutool is installed and could not read this file.

    Distinct from mutool being absent on purpose. Absent means retention was
    not measured, which is a skip and exits 3. Present and failing on a file
    the converter just wrote means the file is not readable, and that is a
    finding about the file. The first version returned 3 for both, and the
    workflow reads 3 as "nothing to calibrate against" -- so an output mutool
    choked on went green (codex, #1617)."""

    def __init__(self, path: Path):
        super().__init__(str(path))
        self.path = path


def retentie(lees, src: Path, dst: Path, row: dict) -> bool | None:
    """Did the output keep the source's text? None when there is none to keep.

    `lees` is `tekst` bound to a mutool. Fills `row` with the numbers and
    raises Onleesbaar, naming the side, when mutool could not read one."""
    bron_t, uit_t = lees(src), lees(dst)
    if bron_t is None:
        raise Onleesbaar(src)
    if uit_t is None:
        raise Onleesbaar(dst)
    src_chars, dst_chars = len(bron_t), len(uit_t)
    row["src_chars"], row["out_chars"] = src_chars, dst_chars
    if src_chars == 0:
        # No extractable text to lose. Counted as retained, recorded so
        # nobody reads it as a measurement that happened.
        row["retention_pct"] = None
        row["note"] = "source exposes no extractable text"
        return None
    # Two numbers, and the second is the one that decides. The length ratio
    # stays because it is what the floors were measured with and it says
    # something about surplus; the containment figure is what notices
    # deletion and replacement.
    pct = round(dst_chars / src_chars * 100, 2)
    row["retention_pct"] = pct
    behouden = behouden_deel(bron_t, uit_t)
    row["source_kept_pct"] = behouden
    if behouden >= RETENTION_MIN_PCT:
        return True
    row["note"] = (
        f"only {behouden}% of the source's characters survive in order, though "
        f"the output is {pct}% of its length"
    )
    return False


def chars(mutool: str, pdf: Path) -> int:
    """Non-whitespace characters mutool extracts, or -1 when it cannot read."""
    t = tekst(mutool, pdf)
    return -1 if t is None else len(t)


def behouden_deel(bron: bytes, uit: bytes) -> float:
    """How much of the SOURCE's text survives in the output, 0..100.

    A ratio of lengths cannot answer this and that is the point. Conversion
    normally exposes a little more text than the source -- the measured range
    here is 100.8 to 103.3 per cent -- and inside that surplus an output can
    drop several per cent of the original characters, or replace them with the
    same number of unrelated ones, and still count as retained. The gate exists
    to catch deletion, and a length was never going to see it. (codex, #1617)

    Longest-common-subsequence-free and deliberately cheap: walk the source in
    order and count how much of it can be found, in order, in the output.
    Reordering therefore reads as loss, which is the safe direction for a gate
    whose job is to notice missing content.
    """
    if not bron:
        return 100.0
    gevonden, k = 0, 0
    for teken in bron:
        j = uit.find(bytes([teken]), k)
        if j < 0:
            continue
        gevonden += 1
        k = j + 1
    return round(gevonden / len(bron) * 100, 2)


def judge(measured: dict, floors: dict[str, dict[str, int]], key: str) -> tuple[int, list[str]]:
    """The whole verdict, as a pure function of numbers.

    Split out so `test_pdfa_output_conformance_gate.py` can hand it the report
    the deleted workflow published -- five documents, nothing compliant -- and
    assert that it comes back red. A gate nobody has ever seen fail is a gate
    nobody knows the failing branch of.
    """
    lines: list[str] = []

    if measured["fixtures"] < MINIMUM_FIXTURES:  # FLOOR
        return 1, [
            f"{measured['fixtures']} fixture(s) found, floor is {MINIMUM_FIXTURES}. "
            "Nothing was converted, so nothing was graded, and an empty run "
            "reports in the same words as a clean one."
        ]

    floor = floors.get(key)
    if floor is None and key not in BLOKKEERT:
        # Measured, printed, and not blocking -- because there is no number on
        # this platform to block against. Announced in the words this repository
        # reserves for "did not judge", so it cannot be read as a pass.
        return 3, [
            f"SKIPPED (not a pass): no floor recorded for platform {key!r}, so this",
            "run graded nothing. It measured:",
            f'    "{key}": {json.dumps(measured, sort_keys=True)},',
            f"Add that to FLOORS and {key!r} to BLOKKEERT, in one commit, naming the",
            "run the numbers came from. Until then this platform is not gated and",
            "that gap is deliberate rather than hidden.",
        ]
    if floor is None:
        return 1, [
            f"platform {key!r} is in BLOKKEERT but has no floor. One of the two is",
            "wrong: either record the numbers or stop claiming to gate here.",
        ]

    verdict = 0
    for name in ("fixtures", "converted", "conformant", "retained"):
        got, want = measured[name], floor[name]
        if got == want:
            continue
        verdict = 1
        direction = "dropped to" if got < want else "rose to"
        lines.append(f"{name} {direction} {got}, floor is {want}.")
        if got < want:
            lines.append(
                f"  Something that used to work no longer does. If {name} is "
                "legitimately lower, lower the floor in the same commit and say why."
            )
        else:
            lines.append(
                f"  Nothing is wrong with a higher {name} -- but a floor that only "
                "fails downward stops being a floor. Raise it here so the gain "
                "cannot be lost again without anybody noticing."
            )
    return verdict, lines


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--converter",
        default="target/release/examples/pdfa_convert_real",
        help="the SHIPPING conversion path; convert_pdfa has drifted from it",
    )
    ap.add_argument("--verapdf", default="verapdf")
    ap.add_argument("--mutool", default="mutool")
    ap.add_argument("--out", default="", help="where to keep the converted output")
    args = ap.parse_args()

    converter = Path(args.converter)
    if not converter.is_file():
        die(f"converter not found at {converter} — build it before grading it")

    verapdf = shutil.which(args.verapdf) or (
        args.verapdf if Path(args.verapdf).is_file() else None
    )
    if verapdf is None:
        # Not a skip. The predecessor installed veraPDF with `|| true`, added
        # /opt/verapdf to PATH whether or not anything landed there, and then
        # counted the resulting nothing as five "errors" under a green tick.
        die(f"veraPDF not found ({args.verapdf}); the gate is meaningless without it")

    mutool = shutil.which(args.mutool)
    if mutool is None:
        print(
            "SKIPPED (not a pass): mutool not found — text retention was NOT "
            "measured on this run, only conformance. Conformance on its own "
            "cannot see a fixup that passes validation by deleting content.",
            file=sys.stderr,
        )

    if len(FIXTURES) < MINIMUM_FIXTURES:  # FLOOR
        die(
            f"{len(FIXTURES)} fixture(s) under corpus/, floor is {MINIMUM_FIXTURES}",
            1,
        )

    out_dir = Path(args.out) if args.out else Path(tempfile.mkdtemp(prefix="pdfa-out-"))
    out_dir.mkdir(parents=True, exist_ok=True)

    converted = conformant = retained = 0
    rows: list[dict] = []

    for src in FIXTURES:
        dst = out_dir / f"{src.stem}.pdfa.pdf"
        if dst.exists():
            dst.unlink()
        proc = subprocess.run(
            [str(converter), str(src), str(dst)], capture_output=True, text=True
        )
        row: dict = {"fixture": src.name}

        if proc.returncode != 0 or not dst.is_file():
            row["converted"] = False
            row["note"] = (proc.stderr or "").strip().splitlines()[-1:] or ["no output"]
            rows.append(row)
            print(f"  {src.name:<24} CONVERT FAILED")
            continue

        # The output is a file this run created in a directory this run made.
        # There is no path by which the source could be graded instead -- which
        # is the entire defect this gate replaces.
        assert dst.resolve() != src.resolve()
        converted += 1
        row["converted"] = True

        ok, why = verapdf_verdict(verapdf, dst)
        row["conformant"] = ok
        if why:
            row["verapdf_note"] = why
        conformant += ok

        pct = None
        if mutool is not None:
            try:
                kept = retentie(lambda p: tekst(mutool, p), src, dst, row)
            except Onleesbaar as e:
                # Not a skip. mutool is here and ran; the file it was handed is
                # what failed. Named, so the next person opens that file and
                # not the tool's install notes.
                kant = "the converter's output" if e.path == dst else "the source"
                print(file=sys.stderr)
                print(f"[pdfa-output] FATAL: mutool could not read {e.path.name} "
                      f"({kant} for {src.name}). A reader that fails is not a "
                      "document without text; retention for this fixture is "
                      "unknown, and unknown is not a pass. Run "
                      f"`{mutool} draw -F txt {e.path}` by hand to see why.",
                      file=sys.stderr)
                return 1
            pct = row.get("retention_pct")
            if kept is not False:
                retained += 1

        rows.append(row)
        print(
            f"  {src.name:<24} conformant={str(ok):<5} "
            f"retention={'n/a' if pct is None else f'{pct}%'}"
            + (f"  [{why}]" if why else "")
        )

    measured = {
        "fixtures": len(FIXTURES),
        "converted": converted,
        "conformant": conformant,
        "retained": retained if mutool is not None else 0,
    }
    key = platform_key()

    print()
    print(f"[pdfa-output] platform {key}, profile PDF/A-{PROFILE}")
    print(f"[pdfa-output] {json.dumps(measured, sort_keys=True)}")
    if mutool is None:
        # Zeroing the retention floor here turned "we could not measure" into
        # "every number matched", on a script whose own header says conformance
        # alone cannot detect content deletion. That is the cannot-run-reported-
        # as-a-pass fault this gate exists to remove, inside the gate.
        # (codex, #1617)
        print("SKIPPED (not a pass): mutool is not installed, so no text was read "
              "back and retention was not measured. Conformance alone cannot see "
              "deleted content, which is the whole reason this gate exists.",
              file=sys.stderr)
        return 3
    floors = FLOORS

    verdict, lines = judge(measured, floors, key)
    if verdict == 0:
        print("[pdfa-output] every number matches the floor for this platform")
        return 0

    if verdict == 3:
        # Did not judge, and says so. Distinct from a failure on purpose: a
        # platform with no recorded floor has nothing to be measured against,
        # and failing every run on evidence we do not have is how a gate becomes
        # a red nobody reads. The numbers to record are in `lines`.
        print(file=sys.stderr)
        for line in lines:
            print(f"  {line}", file=sys.stderr)
        return 3

    print(file=sys.stderr)
    print("[pdfa-output] FATAL:", file=sys.stderr)
    for line in lines:
        print(f"  {line}", file=sys.stderr)
    print(file=sys.stderr)
    print("Per-fixture detail:", file=sys.stderr)
    for row in rows:
        print(f"  {json.dumps(row, sort_keys=True)}", file=sys.stderr)
    return verdict


if __name__ == "__main__":
    raise SystemExit(main())
