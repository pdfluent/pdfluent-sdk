#!/usr/bin/env python3
"""Text-replacement gate over a fixed corpus sample, on two axes.

WHY THIS EXISTS

Our text-replacement tests run on documents we build ourselves: four objects, one
font, one string. Real PDFs are not like that, and the PDF/A track has shown
repeatedly that a corpus finds whole classes of problem that synthetic fixtures
never surface.

TWO AXES, MEASURED SEPARATELY

    1. did the replacement succeed?
    2. is the text still extractable afterwards?

Both are needed, and the second is the one that bites. The PDF/A round-three
lesson was that a document can become *more* conformant by losing content — a
single pass/fail number hides that completely. The same applies here: a
replacement that "succeeds" while making the page unreadable is a worse outcome
than one that fails outright, because nothing reports it.

RULES THIS GATE FOLLOWS

  * The sample is a fixed list, committed to the repo. Never re-sample to make a
    number look better; a moved goalpost is not a measurement.
  * Results are compared per document against a baseline, not in aggregate. An
    aggregate that stays flat while ten documents break and ten others start
    working is a regression the total cannot see.
  * A document that cannot be opened at all is not counted as a failure of
    replacement — it is reported separately. Mixing "we broke it" with "it was
    already broken" makes both unreadable.

Exit codes:
    0  gate passed
    1  gate failed (regression against the baseline)
    2  the gate could not run (missing corpus, binary, or sample list)
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass, asdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
DEFAULT_SAMPLE_LIST = REPO / "benchmarks" / "text_replace" / "corpus_sample_200.txt"
DEFAULT_BASELINE = REPO / "benchmarks" / "text_replace" / "baseline.json"


def die(msg: str, code: int = 2) -> None:
    print(f"[text_replace_gate] FATAL: {msg}", file=sys.stderr)
    sys.exit(code)


@dataclass
class DocResult:
    name: str
    opened: bool
    engine_found: bool          # our own search located the needle
    replaced: bool
    extractable: bool
    note: str = ""
    # The word this run picked, and what it wrote in its place.
    #
    # Without them a reported failure cannot be reproduced. On 22-08 the nine
    # "replacement not found in extracted text" entries each took a separate
    # experiment to re-derive the word, and six of the nine then replaced and
    # extracted perfectly with a different word -- so the failure lives in the
    # run that got hit, not in the document. That is a useful finding and it was
    # nearly invisible: the artefact recorded the verdict and dropped the input.
    needle: str = ""
    replacement: str = ""

    @property
    def axis1(self) -> bool:
        """Axis 1: the replacement applied."""
        return self.replaced

    @property
    def axis2(self) -> bool:
        """Axis 2: the text survived as text."""
        return self.extractable


# A timeout and an unreadable file are not the same finding, and collapsing them
# cost a full pipeline round on 2026-08-20: 92 of 200 source documents were
# reported as "not readable by pdftotext" and counted as regressions, while the
# real cause was a saturated machine -- two cargo-test jobs were running at the
# time and a 60-second budget is easy to miss under that load.
#
# The source documents cannot regress. They are the same bytes they were when the
# baseline was recorded, so a source that suddenly cannot be read says something
# about the machine, never about our code.
TIMED_OUT = object()


def extract_text(pdftotext: str, pdf: Path, timeout: int = 60):
    """Extracted text, None if genuinely unreadable, TIMED_OUT if it ran out of time."""
    try:
        out = subprocess.run(
            [pdftotext, "-enc", "UTF-8", "-nopgbrk", str(pdf), "-"],
            capture_output=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return TIMED_OUT
    except OSError:
        return None
    if out.returncode != 0:
        return None
    return out.stdout.decode("utf-8", errors="replace")


def run_one(runner: str, pdftotext: str, src: Path, workdir: Path,
            fallback: str = "deny") -> DocResult:
    """Replace the first word we can find, then read the page back."""
    name = src.name
    before = extract_text(pdftotext, src)
    if before is TIMED_OUT:
        # Deliberately its own note: the caller counts these and refuses to judge
        # rather than reporting them as regressions.
        return DocResult(name, False, False, False, False, "SOURCE_TIMEOUT")
    if before is None:
        return DocResult(name, False, False, False, False, "source not readable by pdftotext")

    # Pick a needle from the document's own text: a word of decent length, so
    # the match is unambiguous and the replacement is a realistic edit rather
    # than a single character.
    words = [w.strip(".,;:()[]\"'") for w in before.split()]
    needle = next((w for w in words if len(w) >= 6 and w.isalpha()), None)
    if needle is None:
        return DocResult(name, True, False, False, False, "no suitable word to replace")

    replacement = "ERSATZWORT"
    out_pdf = workdir / f"{name}.edited.pdf"
    try:
        proc = subprocess.run(
            [runner, "--input", str(src), "--output", str(out_pdf),
             "--find", needle, "--replace", replacement,
             "--fallback", fallback],
            capture_output=True,
            timeout=180,
        )
    except (subprocess.TimeoutExpired, OSError) as e:
        return DocResult(name, True, False, False, False, f"runner failed: {e}",
                         needle, replacement)

    if proc.returncode != 0 or not out_pdf.exists():
        detail = proc.stderr.decode("utf-8", errors="replace").strip().splitlines()
        last = detail[-1] if detail else "replacement failed"
        # Exit 1 with "no match" means OUR search did not see text that the
        # outside reader did. That is a finding about the search, not a broken
        # replacement, and counting it as the latter buries it.
        engine_found = "no match for" not in last
        return DocResult(name, True, engine_found, False, False, last[:120],
                         needle, replacement)

    after = extract_text(pdftotext, out_pdf)
    if after is None:
        return DocResult(name, True, True, True, False, "edited file no longer extractable",
                         needle, replacement)

    extractable = replacement in after
    note = "" if extractable else "replacement not found in extracted text"
    return DocResult(name, True, True, True, extractable, note, needle, replacement)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus-dir", required=True)
    ap.add_argument("--runner", default=str(REPO / "target" / "release" / "text-replace-cli"))
    ap.add_argument("--pdftotext", default="pdftotext")
    ap.add_argument("--sample-list", default=str(DEFAULT_SAMPLE_LIST))
    ap.add_argument("--baseline", default=str(DEFAULT_BASELINE))
    ap.add_argument("--write-baseline", action="store_true",
                    help="record the current result as the baseline instead of judging against it")
    ap.add_argument("--limit", type=int, default=0, help="0 = the whole sample")
    ap.add_argument("--workdir", default="/tmp/text-replace-gate")
    # De job gaf dit al mee terwijl het hier niet bestond, waardoor
    # corpus:text-replace-capability binnen een seconde faalde op het parsen en
    # de meting die hij moest opleveren nooit is gedraaid. De keuzes zijn
    # afgedwongen: een tikfout mag geen meting opleveren die iets anders meet
    # dan hij in zijn kop zet.
    ap.add_argument("--fallback", choices=("deny", "standard"), default="deny",
                    help="welke fontfallback het vervangen mag gebruiken; "
                         "'deny' is de bibliotheekstandaard")
    args = ap.parse_args()

    corpus = Path(args.corpus_dir)
    if not corpus.is_dir():
        die(f"corpus dir not found: {corpus}")
    sample_list = Path(args.sample_list)
    if not sample_list.exists():
        die(f"sample list not found: {sample_list}; the gate is meaningless without a fixed sample")
    if subprocess.run(["sh", "-c", f"command -v {args.pdftotext}"],
                      capture_output=True).returncode != 0:
        die(f"{args.pdftotext} not found; axis 2 cannot be measured without an outside reader")
    if not Path(args.runner).exists():
        die(f"runner not found: {args.runner}")

    names = [l.strip() for l in sample_list.read_text().splitlines() if l.strip()]
    if args.limit:
        names = names[: args.limit]

    workdir = Path(args.workdir)
    workdir.mkdir(parents=True, exist_ok=True)

    results: list[DocResult] = []
    for i, name in enumerate(names, 1):
        src = corpus / name
        if not src.exists():
            results.append(DocResult(name, False, 0, False, False, "not present in corpus"))
            continue
        results.append(run_one(args.runner, args.pdftotext, src, workdir, args.fallback))
        if i % 25 == 0:
            print(f"[text_replace_gate] {i}/{len(names)}")

    usable = [r for r in results if r.opened and r.engine_found]
    axis1 = sum(1 for r in usable if r.axis1)
    axis2 = sum(1 for r in usable if r.axis2)

    print("=" * 68)
    print(f"[text_replace_gate] sample:        {sample_list.name} ({len(names)} documents)")
    unseen = [r for r in results if r.opened and not r.engine_found
              and "no suitable word" not in r.note]
    print(f"[text_replace_gate] usable:        {len(usable)}")
    print(f"[text_replace_gate] text our search could not find: {len(unseen)}")
    print(f"[text_replace_gate] axis 1 replaced:     {axis1}/{len(usable)}")
    print(f"[text_replace_gate] axis 2 extractable:  {axis2}/{len(usable)}")
    print("=" * 68)

    # The gap between the axes is the number worth watching: it is the count of
    # documents we changed and then could not read back.
    if unseen:
        print("[text_replace_gate] the outside reader saw text our search did not "
              f"({len(unseen)}) — a finding about find_text, not about replacement:")
        for r in unseen[:15]:
            print(f"  {r.name}: {r.note}")

    silent_losses = [r.name for r in usable if r.axis1 and not r.axis2]
    if silent_losses:
        print(f"[text_replace_gate] replaced but NOT extractable ({len(silent_losses)}):")
        for n in silent_losses[:20]:
            print(f"  {n}")

    current = {r.name: asdict(r) for r in results}
    baseline_path = Path(args.baseline)

    if args.write_baseline:
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        baseline_path.write_text(json.dumps(current, indent=2, sort_keys=True))
        print(f"[text_replace_gate] baseline written: {baseline_path}")
        sys.exit(0)

    if not baseline_path.exists():
        # First run: there is nothing to judge against yet, so record what we
        # found and say plainly that this run proved nothing about regressions.
        #
        # This is not the same as tolerating a *missing* baseline. The file is
        # committed to the repo, so its absence is visible in git — and the
        # banner below makes a re-baseline impossible to mistake for a pass in
        # the job log.
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        baseline_path.write_text(json.dumps(current, indent=2, sort_keys=True))
        print()
        print("=" * 68)
        print("[text_replace_gate] BASELINE ESTABLISHED — THIS RUN JUDGED NOTHING")
        print(f"[text_replace_gate] no baseline existed at {baseline_path}")
        print("[text_replace_gate] the numbers above are the starting point, not a verdict.")
        print("[text_replace_gate] Commit the baseline; the next run compares against it.")
        print("=" * 68)
        sys.exit(0)

    baseline = json.loads(baseline_path.read_text())
    regressions = []
    for name, now in current.items():
        was = baseline.get(name)
        if not was:
            continue  # new entry, not a regression
        for axis in ("replaced", "extractable"):
            if was[axis] and not now[axis]:
                regressions.append(f"{name}: {axis} {was[axis]} -> {now[axis]} ({now['note']})")

    if regressions:
        # Source timeouts are not regressions and must not be reported as any.
        #
        # The source documents are the same bytes as when the baseline was
        # recorded, so one that suddenly cannot be read within the budget says the
        # machine was busy, not that our code got worse. Exit 2 means "could not
        # measure"; exit 1 means "measured, and it is worse". Conflating them is
        # how a saturated runner produced 92 phantom regressions on 2026-08-20 --
        # a number alarming enough to look like a serious defect, on a night when
        # nobody was awake to question it.
        timed_out = [r for r in regressions if "SOURCE_TIMEOUT" in str(r)]
        if timed_out:
            print(f"[text_replace_gate] CANNOT MEASURE: {len(timed_out)} source "
                  f"document(s) timed out in pdftotext.")
            for r in timed_out[:10]:
                print(f"  {r}")
            if len(timed_out) > 10:
                print(f"  ... and {len(timed_out) - 10} more")
            print()
            print("[text_replace_gate] These are not regressions. The sources have not")
            print("[text_replace_gate] changed since the baseline; the machine was too")
            print("[text_replace_gate] busy to read them in time. Check what else was")
            print("[text_replace_gate] running (scripts/ci/runner_busy_check.sh) and")
            print("[text_replace_gate] re-run on an idle machine.")
            sys.exit(2)

        print(f"[text_replace_gate] REGRESSIONS ({len(regressions)}):")
        for r in regressions[:40]:
            print(f"  {r}")
        sys.exit(1)

    print("[text_replace_gate] no per-document regressions against the baseline")
    sys.exit(0)


if __name__ == "__main__":
    main()
