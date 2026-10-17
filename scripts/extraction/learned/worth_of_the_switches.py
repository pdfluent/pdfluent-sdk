#!/usr/bin/env python3
"""What the learned switches are worth on the heading axis, together and apart.

    feature_dump --list pages.list --refine heading,heading_ends --out on.jsonl
    worth_of_the_switches.py --labels labels.jsonl --runs off.jsonl heading.jsonl ...

WHY THE ENGINE AND NOT THE MODEL. A boundary model changes the BLOCKS a class
model then reads, so measuring the two apart and adding the numbers measures
something that never happens. Each file here is the engine's own output with a
set of switches on (`feature_dump --refine`), so "together" is a run and not a
sum.

WHAT IS COMPARED. The heading axis as `structured_axes` scores it -- matched by
text at the same similarity floor the round uses -- against the annotation's
regions, whose text is the text of the blocks each region claims. Page by page,
because matching a document's thousands of blocks pairwise is hours per shard;
the absolute figure is therefore not the round's and only the DIFFERENCE
between columns means anything. The round measures the rig.

AND AGAINST WHOSE HEADINGS. DocLayNet marks a `section_header` where its
annotator saw one, and on many pages that is the running title and nothing
below it: of the 120 blocks `heading_ends` cut loose on 400 validation pages,
several read as sub-headings to anyone ("Revenues", "Equity and
equity-related compensation benefits") and stand on pages whose annotation
lists no heading at all. So a column that LOSES on this axis has either
invented headings or found ones the dataset does not mark, and this
measurement cannot tell the two apart -- which is why the per-switch precision
table is printed underneath and read as evidence rather than as a verdict.

WHICH CLASSES ARE READ. Only the ones our blocks answer in the annotation's
units. A table is one region and our blocks are its CELLS, so a per-item F1
between the two is near zero however good the extraction is; printing that 0
as a score would be a reading of the units and not of the engine, so the class
is refused by name.

ON VALIDATION, NEVER ON THE RIG. DocLayNet's validation split is neither the
rig's tuning half nor its sealed fifths.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import structured_axes as axes  # noqa: E402
from worth_of_a_swap import (AXIS_CLASSES, OURS_AS_THE_AXIS_READS_IT,  # noqa: E402
                             annotation_of, f1_of, read_jsonl)

# Our blocks are cells and the annotation's table is one region; see the head
# of this file. Refused by name rather than scored as a zero.
READ_AS_ONE_THING = tuple(k for k in AXIS_CLASSES if k != "table")


# The truth is built ONCE, by the instrument both lanes share: a region's text
# is our words inside it, not the text of the blocks that claim it. The second
# reading is blind to a swallowed heading -- the shape this table exists to
# price -- and it cost a decision before anyone noticed (see `annotation_of`).


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--labels", required=True, type=pathlib.Path,
                    help="one object per page: page_id, label_frame, labels")
    ap.add_argument("--runs", nargs="+", required=True, type=pathlib.Path,
                    help="the engine's output per switch set, `--refine` and all")
    args = ap.parse_args()

    labels = {page["page_id"]: page for page in read_jsonl(args.labels)}
    # The annotation is read ONCE, from the run whose switches are all off: the
    # regions are the dataset's and do not move, and the blocks that claim them
    # are the ones the models have not touched -- which is the only reading
    # every column can be compared against.
    base = {page["page_id"]: page for page in read_jsonl(args.runs[0])}
    truth = {}
    for page_id, page in base.items():
        page = dict(page, **{k: v for k, v in (labels.get(page_id) or {}).items()
                             if k in ("labels", "label_frame")})
        truth[page_id] = annotation_of([page])

    table, cuts = {}, {}
    for path in args.runs:
        pages = read_jsonl(path)
        said = (pages[0].get("refine") or "off") if pages else path.stem
        # TWO RUNS CAN CARRY THE SAME SWITCH SET and mean different things --
        # the same switches at two thresholds is the commonest sweep there is.
        # Keyed by the stamp alone the second run overwrote the first and the
        # table lost a column without saying so, which is the failure mode a
        # table is least able to afford.
        if said in table:
            said = f"{said} [{path.stem}]"
        per_axis: dict[str, list[float]] = collections.defaultdict(list)
        right = wrong = 0
        for page in pages:
            annotated = truth.get(page["page_id"]) or {}
            ours: dict[str, list[str]] = collections.defaultdict(list)
            for block in page.get("blocks") or []:
                kind = OURS_AS_THE_AXIS_READS_IT.get(block.get("kind"), "paragraph")
                if kind in READ_AS_ONE_THING:
                    ours[kind].append(block.get("text") or "")
            for name in READ_AS_ONE_THING:
                want = annotated.get(name) or []
                got = f1_of(want, ours.get(name) or [])
                if got is not None:
                    per_axis[name].append(got)
            # WHICH HEADINGS THE SWITCHES MADE, scored on their own. An axis
            # says how a column ENDS; this says whether the headings the column
            # gained are ones the annotation knows. A switch can lose on the
            # axis and still be right here -- see the head of this file.
            #
            # GAINED, not new: the two actions differ. A boundary makes a block
            # that was not there, a class promotes one that was, and counting
            # only unseen text would credit the first and report the second as
            # having done nothing. So a heading is gained when its text was not
            # a heading in the first column.
            before = {block.get("text"): block.get("kind") for block in
                      (base.get(page["page_id"]) or {}).get("blocks") or []}
            made = [axes.normalise(block.get("text") or "")
                    for block in page.get("blocks") or []
                    if block.get("kind") == "heading"
                    and before.get(block.get("text")) != "heading"]
            made = [text for text in made if text]
            want = [text for text in (axes.normalise(t) for t in
                                      (annotated.get("heading") or [])) if text]
            matched = len(axes.match_items(want, made)) if want and made else 0
            right, wrong = right + matched, wrong + len(made) - matched
        table[said] = {name: statistics.fmean(values) if values else None
                       for name, values in per_axis.items()}
        cuts[said] = (right, wrong)

    order = list(table)
    print(f"{len(base)} page(s) of the validation split, one engine run per column\n")
    print(f"{'axis':12s}" + "".join(f"{name[:22]:>24s}" for name in order))
    for name in READ_AS_ONE_THING:
        row = f"{name:12s}"
        for said in order:
            value = table[said].get(name)
            row += f"{value:24.4f}" if value is not None else f"{'-':>24s}"
        print(row)
    print(f"{'table':12s}" + "".join(f"{'refused: cells':>24s}" for _ in order))
    print()
    first = order[0]
    for said in order[1:]:
        moved = {name: (table[said].get(name) or 0) - (table[first].get(name) or 0)
                 for name in READ_AS_ONE_THING}
        print(f"{said} against {first}: " +
              ", ".join(f"{name} {value:+.4f}" for name, value in moved.items()))
    print()
    for said in order:
        right, wrong = cuts[said]
        total = right + wrong
        if not total:
            print(f"{said}: no heading {first} did not already have")
            continue
        print(f"{said}: {total} heading(s) gained over {first}; "
              f"{right} the annotation marks as a heading, {wrong} it does not "
              f"({right / total:.1%} right)")
    # AND THE MEASURE'S OWN FLOOR, printed rather than subtracted: the first
    # column is compared with itself, so its count should be nought. It is not,
    # because a heading is looked up by its TEXT and a page can carry the same
    # short text twice in two kinds ("Notes" over a column and beside it).
    # Whatever it reads is the noise under every other column's count.
    floor = sum(cuts[first])
    if floor:
        print(f"\n{first} against itself reads {floor}, which is this "
              "measure's floor: a heading is looked up by its text and a page "
              "can carry one text in two kinds")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
