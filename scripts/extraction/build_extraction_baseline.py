#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Turn a measured run into the per-platform extraction baseline (#367).

The baseline is what the ratchet compares against, so it carries only the four
axis values per document plus the provenance needed to argue with it later: the
run it came from, the date, the machine class, the engine commit and the tools.

Shape, which is the loop's shape rather than this capability's:

    {"platforms": {"darwin": {"sets": {"gate": {
        "measured_from": "<run id>", "documents": {"<name>": {...}}}}}}}

A value that is not the plain measurement carries a `why`. That is the escape
hatch the protocol asks for: a deliberate trade is a decision with a name and a
date, not a quietly relaxed threshold.

Usage:
    python3 scripts/extraction/build_extraction_baseline.py \
        --run benchmarks/runs/extraction/<run-id>.json \
        --out benchmarks/extraction/baseline.json
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))

import a_baseline_is_not_older_than_what_it_measures as AGE  # noqa: E402

# What the age guard reads to decide whether this baseline still describes the
# tree. Recorded here, at the moment the baseline is built, because the alternative
# -- the commit the run names -- does not survive the landing: `ff3nr.sh` rebases
# with `git commit --amend -s`, so every commit on the branch gets a new sha and a
# baseline measured on a branch names a commit that is gone by the time it is on
# master (#548, #1913). Bytes do not move under a rebase.
BASELINE_PATH = "benchmarks/extraction/baseline.json"

# What the ratchet reads. Anything else in the run file is provenance and
# analysis, and copying it into the baseline would only invite somebody to
# compare against a number nothing guards.
AXIS_KEYS = ("parsed", "char_recall", "order_agreement", "extract_ms")


# The axes as the guard reads them, and which direction is worse. Kept here so
# that `--why` marks exactly the documents `a_baseline_only_moves_with_a_ticket`
# will ask about, and no others: a `why` sprayed over documents that did not move
# is a reason nobody wrote, sitting where a reason is meant to be.
LOWER_IS_WORSE = ("char_recall", "order_agreement")
HIGHER_IS_WORSE = ("extract_ms",)

# On a document whose two readers disagree with each other, `char_recall` is the
# figure for the reader we agree with more (#548), and that reader is chosen by
# our own output. One number therefore cannot say whether the document got
# better: regress to the other reader's reading and the other reader becomes the
# oracle, so the number rises. Both readers' figures travel into the baseline for
# that reason, and `extraction_ratchet.py` judges both (codex, #1913).
PER_READER = "recall_by_reader"


def moved_down(before: dict, after: dict) -> list:
    """Which axes this document gave something up on. Empty when none did.

    Returned as a list rather than a flag because the reason differs per axis,
    and one sentence covering all of them would be true of none. A re-baseline
    that changes what `order_agreement` means also re-measures `extract_ms` on a
    machine with a different load, and calling that second movement a change of
    metric is how a `why` stops being read.
    """
    worse = []
    if before.get("parsed") is True and after.get("parsed") is not True:
        worse.append("parsed")
    # Per reader before the chosen figure, because it is the one that can fall
    # while the chosen figure rises -- which is the whole reason it is recorded.
    was_by_reader = before.get(PER_READER) or {}
    now_by_reader = after.get(PER_READER) or {}
    for reader in sorted(was_by_reader):
        was, now = was_by_reader.get(reader), now_by_reader.get(reader)
        if isinstance(was, (int, float)) and (now is None or now < was):
            worse.append(f"char_recall vs {reader}")
    for axis in LOWER_IS_WORSE:
        was, now = before.get(axis), after.get(axis)
        if isinstance(was, (int, float)) and (now is None or now < was):
            worse.append(axis)
    for axis in HIGHER_IS_WORSE:
        was, now = before.get(axis), after.get(axis)
        if isinstance(was, (int, float)) and (now is None or now > was):
            worse.append(axis)
    return worse


# The clock is the axis that moves without anybody touching the code: the same
# binary on the same machine under a different load. It is recorded as the
# minimum of three runs for that reason (A22), and a re-baseline still shifts it
# by a millisecond either way. Its `why` says so rather than borrowing the
# reason the measured axes were given.
CLOCK_ONLY = (
    "re-measured with the same binary on the same machine under a different "
    "load; the clock is the only axis that moved, and it is the minimum of "
    "three runs for exactly that reason"
)


def previous_value(before: dict, axis: str):
    """What the baseline being replaced held for one axis of `moved_down`.

    `char_recall vs mutool` is not a key: it lives inside `recall_by_reader`, and
    a reason that named the axis and then dropped its figure would be a reason
    with the number missing from exactly the case the number was added for.
    """
    reader = axis.partition("char_recall vs ")[2]
    if reader:
        return (before.get(PER_READER) or {}).get(reader)
    return before.get(axis)


def entry_for(record: dict) -> dict:
    out = {key: record.get(key) for key in AXIS_KEYS}
    # Only where the measurer produced them, which is the documents scored
    # best-of-two. A document whose readers agree has one reading to be measured
    # against and carries no pair.
    if isinstance(record.get(PER_READER), dict):
        out[PER_READER] = dict(record[PER_READER])
        out["oracle"] = record.get("oracle")
    # An axis that measured nothing is recorded as the absence of a value, never
    # as -1 and never as 0. Zero is a real recall -- we found none of what the
    # oracle found. None is "there was nothing to compare against", which the
    # shared comparison skips instead of scoring.
    for key in ("char_recall", "order_agreement"):
        if out.get(key) is not None and out[key] < 0:
            out[key] = None
    if record.get("short"):
        out["why"] = (
            f"oracle text has {record.get('oracle_words')} words, under the 20 "
            "below which a percentage is noise (A12); recall is recorded but "
            "not counted in the corpus figures"
        )
    if record.get("verdict") not in (None, "ok"):
        out["why"] = f"our extractor reported {record['verdict']}"
    if record.get("oracle") is None:
        out["why"] = record.get("why", "no foreign reader produced text")
    return out


def build(run: dict, previous: dict | None = None, why: str | None = None,
          why_for: dict[str, str] | None = None) -> dict:
    """The baseline for one run, with `why` on every document that gave something up.

    The reason is asked for by the guard and it is asked for per document, which
    is right: a re-baseline that lowers three hundred numbers for one stated
    reason still has to say which documents paid. `--why` is for exactly the case
    that reason is the same sentence on all of them -- a changed metric, measured
    once -- and it is written only where a value actually moved down.

    `--why-for` is the other case, and it is the commoner one: three documents
    moved down for three different reasons, and one sentence covering all three
    is true of none. It takes precedence over `--why` on the documents it names.
    It is a flag rather than a hand edit of the output because a reason typed
    into generated JSON is a reason the next rebuild deletes, and because the
    command then says, in one line, what was decided about which document.
    """
    documents = {name: entry_for(rec) for name, rec in run["documents"].items()}
    if why or why_for:
        before = (previous or {}).get("documents", {})
        for name in why_for or {}:
            if name not in before or name not in documents:
                raise SystemExit(
                    f"--why-for names {name}, which is not in both baselines; a "
                    "reason has to sit on a document that moved.")
            if not moved_down(before[name], documents[name]):
                # A reason sprayed over a document that did not move is a reason
                # nobody wrote, sitting where a reason is meant to be -- and it
                # is also a sign the sentence was meant for another document.
                raise SystemExit(
                    f"--why-for names {name}, which gave nothing up against the "
                    "baseline being replaced.")
        for name, entry in documents.items():
            if name not in before:
                continue
            worse = moved_down(before[name], entry)
            if not worse:
                continue
            # The stated reason for the measured axes, and the clock's own
            # reason for the clock. A document where only the clock moved gets
            # only the second, which is the difference between a record and a
            # formality.
            stated = (why_for or {}).get(name)
            reason = stated or (CLOCK_ONLY if worse == ["extract_ms"] else why)
            if not reason:
                # Rather than writing the word "None" where a reason belongs.
                # The guard would accept that string -- it asks whether a `why`
                # is there, not whether it says anything -- and the landing
                # would carry a document nobody explained.
                raise SystemExit(
                    f"{name} moved down on {', '.join(worse)} and no reason was "
                    "given for it. Name it with --why-for, or give the whole "
                    "rebaseline one with --why.")
            # Appended, never replacing: a document can be both short and
            # lowered, and the A12 sentence entry_for wrote is the one that
            # says why its number is not in the corpus figures at all.
            said = entry.get("why")
            entry["why"] = (f"{said}; " if said else "") + f"{reason} ({', '.join(worse)} was: " + ", ".join(
                f"{axis} {was}"
                for axis, was in ((a, previous_value(before[name], a)) for a in worse)
                if was is not None
            ) + ")"
    return {
        "measured_from": run["run_id"],
        "measured_subjects": AGE.fingerprint(REPO, AGE.SUBJECTS[BASELINE_PATH]),
        "date": run["date"],
        "machine_class": run.get("machine_class"),
        "commit": run.get("commit"),
        "tools": run.get("tools"),
        "timing": run.get("timing"),
        "oracle": run.get("oracle"),
        "summary": run.get("summary"),
        "documents": documents,
    }


def merge(existing: dict, platform: str, set_name: str, block: dict) -> dict:
    out = existing or {
        "note": (
            "Per-platform, per-document baseline for the text-extraction "
            "capability (#367). Per platform because the two substitute fonts "
            "differently; per document because a corpus mean improves while a "
            "single document is destroyed, which is the failure A11 was "
            "withdrawn over. Moved only by a commit that names a tracker "
            "ticket and carries a `why` on every value that went down "
            "(STRATEGY_2026H2 4.4.5)."
        ),
        "capability": "extraction",
        "platforms": {},
    }
    out.setdefault("platforms", {}).setdefault(platform, {}).setdefault("sets", {})[
        set_name
    ] = block
    return out


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", required=True)
    p.add_argument("--out", default=str(REPO / "benchmarks" / "extraction" / "baseline.json"))
    p.add_argument(
        "--why",
        help=(
            "the reason to record on every document whose value moved down "
            "against the baseline being replaced; the guard asks for one per "
            "document and this writes the same sentence on each, with the "
            "figures it replaced"
        ),
    )
    p.add_argument(
        "--why-for",
        action="append",
        default=[],
        metavar="DOCUMENT=REASON",
        help=(
            "the reason to record on one named document, repeatable; it wins "
            "over --why where both apply. Three documents that moved down for "
            "three reasons need three sentences, and one sentence covering all "
            "of them is true of none"
        ),
    )
    args = p.parse_args()

    why_for: dict[str, str] = {}
    for pair in args.why_for:
        name, sep, reason = pair.partition("=")
        if not sep or not name.strip() or not reason.strip():
            p.error(f"--why-for takes DOCUMENT=REASON, not {pair!r}")
        why_for[name.strip()] = reason.strip()

    run_path = pathlib.Path(args.run)
    with run_path.open(encoding="utf-8") as f:
        run = json.load(f)

    out_path = pathlib.Path(args.out)
    existing = None
    if out_path.exists():
        with out_path.open(encoding="utf-8") as f:
            existing = json.load(f)

    previous = None
    if existing:
        previous = (
            existing.get("platforms", {})
            .get(run["platform"], {})
            .get("sets", {})
            .get(run["set"])
        )
    merged = merge(existing, run["platform"], run["set"],
                   build(run, previous, args.why, why_for))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as f:
        json.dump(merged, f, indent=1, sort_keys=True)
        f.write("\n")
    block = merged["platforms"][run["platform"]]["sets"][run["set"]]
    whys = [entry["why"] for entry in block["documents"].values() if "why" in entry]
    clock = sum(1 for w in whys if CLOCK_ONLY in w)
    print(
        f"OK {out_path}: platform {run['platform']}, set {run['set']}, "
        f"{len(block['documents'])} documents, from run {run['run_id']}"
        + (f"; {len(whys)} carry a `why`, {clock} of them for the clock alone"
           if whys else "")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
