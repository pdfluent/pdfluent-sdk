#!/usr/bin/env python3
"""The two coordinate frames this benchmark family works in, and the way across.

WHY THIS FILE EXISTS. The trap has now cost two lanes a measurement each. The
annotations -- both the rig's ground truth and PubTables' cell boxes -- are
y-DOWN: y grows from the top of the page, so a column head has a SMALLER y than
the last row. `structured_bench.geometric` says so in code, sorting a page by
ascending `bbox[1]` and calling that top to bottom. Our own spans and blocks are
y-UP, because that is what the PDF's user space is.

Comparing a box from one frame with a box from the other does not fail loudly. It
returns an answer that looks like a measurement about documents:

  * #600: every engine candidate fell outside every annotated table, and the
    honest reading of "zero of 23 pairs could be labelled" is "the candidates are
    nowhere near the tables". It was the frames.
  * #599: the cell annotations read as monotonically descending down the page,
    which is only strange until you notice they are y-down.

So the conversion lives here once, with a test, and both sides call it.

THE CONVERSION IS NOT "NEGATE Y". A box is (left, y0, right, y1) with y0 <= y1 in
its own frame, so flipping the axis swaps which end is which: the top of a y-down
box becomes the BOTTOM of the y-up one. Forgetting the swap gives a box with its
ends crossed, which every `inside` test then answers no to -- the same silent
wrongness in a different disguise.

Usage:
    from frames import flip_box, engine_height
    engine_box = flip_box(annotation_box, engine_height(page_record))

    python3 scripts/extraction/frames.py --self-test
"""
from __future__ import annotations

import sys

# The height of a US Letter page, used only by the self-test.
LETTER_HEIGHT = 792.0


def engine_height(record: dict) -> float:
    """The page height IN THE ENGINE'S FRAME, taken from the engine itself.

    NOT FROM THE MEDIABOX, and this file used to get that wrong (codex, #596). The
    first raw `/MediaBox` is the wrong number on more pages than it is the right
    one: a 90-degree `/Rotate` on a 612x792 page gives an engine-space height of
    612 where the box says 792, which shifts every flipped annotation by 180
    points; a `/CropBox` narrower than the media, a later page of another size, and
    a box behind an indirect or compressed object are each wrong in their own way.
    The engine's coordinates come from `render_dimensions()` and
    `initial_transform`, so the only height that agrees with our boxes is the one
    the engine reports beside them.

    `feature_dump` (#598) and `structured_markdown --json` both carry `height` per
    page for exactly this reason. Pass that record here rather than reaching for a
    parser: a second opinion about page geometry is a second chance to disagree
    with the boxes we are converting.
    """
    height = record.get("height")
    if not isinstance(height, (int, float)) or height <= 0:
        raise ValueError(
            "the page record carries no usable `height`; the engine writes one per "
            "page and it is the only height that matches its boxes")
    return float(height)


def flip_box(box, height: float):
    """A box from one frame in the other: `height - y`, with the ends swapped.

    The conversion is its own inverse, which is what the self-test leans on: a box
    flipped twice is the box you started with. That also means one function serves
    both directions and there is no way to call the wrong one.
    """
    left, y0, right, y1 = (float(v) for v in box[:4])
    return [left, height - y1, right, height - y0]


def contains(outer, inner, slack: float = 0.0) -> bool:
    """Whether `inner` lies inside `outer`, both in the SAME frame.

    It takes no height and does no converting on purpose: a helper that silently
    flipped one of its arguments would hide the very mistake this file is about.
    Convert first, then ask.
    """
    return (inner[0] >= outer[0] - slack
            and inner[1] >= outer[1] - slack
            and inner[2] <= outer[2] + slack
            and inner[3] <= outer[3] + slack)


def self_test() -> int:
    failures = []

    def check(what, got, want):
        if got != want:
            failures.append(f"{what}: got {got}, want {want}")

    # The real page the #600 measurement turned on. The rig annotates one table
    # on PMC2268700-p0005 at these y-down coordinates, and the engine's four
    # candidate regions are the y-up boxes below. Before the flip not one of them
    # was inside the table; after it, all four are.
    annotated = [58.1, 121.9, 539.1, 514.2]
    flipped = flip_box(annotated, LETTER_HEIGHT)
    check("the table's y-up box", [round(v, 1) for v in flipped],
          [58.1, 277.8, 539.1, 670.1])
    candidates = [
        [265.1, 577.0, 534.0, 605.7],
        [265.2, 546.2, 534.0, 564.7],
        [265.1, 392.5, 534.0, 533.9],
        [265.1, 279.7, 534.0, 349.4],
    ]
    check("candidates inside the flipped table",
          sum(1 for c in candidates if contains(flipped, c, slack=6.0)), 4)
    # AND THE WRONG COMPARISON IS NOT UNIFORMLY WRONG, which is why it survives
    # review. One of these four candidates happens to fall inside the UNflipped
    # box as well, by coincidence of where the table sits on the page. A reader
    # who tries one page and sees a hit concludes the frames agree; the count is
    # pinned here at one so that nobody reads "some of them match" as "the frames
    # match".
    check("candidates inside the UNflipped table, by coincidence",
          sum(1 for c in candidates if contains(annotated, c, slack=6.0)), 1)

    # The ends are swapped, not just negated. Negating without swapping leaves a
    # box whose y0 is above its y1, and `contains` says no to everything.
    crossed = [58.1, LETTER_HEIGHT - 121.9, 539.1, LETTER_HEIGHT - 514.2]
    check("a box flipped without swapping its ends has its y ends crossed",
          crossed[1] > crossed[3], True)
    check("and holds none of the candidates",
          sum(1 for c in candidates if contains(crossed, c, slack=6.0)), 0)

    # THE EVIDENCE THAT SETTLES WHICH FRAME A SET IS IN IS MONOTONICITY, not one
    # box (T4, #599). A single box can sit inside the wrong frame by coincidence,
    # as the count above shows; a run of fifteen rows descending from a column head
    # at y0=72.9 to a last row at y0=460.0 cannot. So the check is on the ORDER of
    # a sequence, and it is the check to reach for on a new set: flip a column of
    # row boxes and their order must reverse, every time.
    rows_down = [[72.0, 72.9 + 26.0 * i, 500.0, 90.9 + 26.0 * i] for i in range(15)]
    check("y-down rows descend down the page",
          [r[1] for r in rows_down] == sorted(r[1] for r in rows_down), True)
    rows_up = [flip_box(r, LETTER_HEIGHT) for r in rows_down]
    check("and the same rows ascend once flipped",
          [r[1] for r in rows_up] == sorted((r[1] for r in rows_up), reverse=True), True)
    check("with every box still its own right way up",
          all(r[1] < r[3] for r in rows_up), True)

    # Its own inverse, so one function serves both directions.
    check("flipping twice is the identity",
          [round(v, 4) for v in flip_box(flipped, LETTER_HEIGHT)],
          [round(v, 4) for v in annotated])

    # The height comes from the engine's own record.
    check("the engine's height is used as given", engine_height({"height": 793.0}), 793.0)
    for bad in ({}, {"height": 0}, {"height": "792"}):
        try:
            engine_height(bad)
        except ValueError:
            pass
        else:
            failures.append(f"engine_height accepted {bad!r}")

    # THE ROTATION TRAP THIS FILE USED TO WALK INTO. A 90-degree-rotated 612x792
    # page has an engine height of 612, and flipping with the MediaBox's 792 moves
    # every box by 180 points -- far enough to put a table in the wrong half of the
    # page and near enough to look like a plausible answer.
    rotated_engine_height = 612.0
    box = [100.0, 100.0, 200.0, 150.0]
    check("flipping with the engine's height and with the box's differ by the gap",
          round(flip_box(box, LETTER_HEIGHT)[1] - flip_box(box, rotated_engine_height)[1], 1),
          round(LETTER_HEIGHT - rotated_engine_height, 1))

    for failure in failures:
        print(f"[frames] FAIL {failure}")
    if failures:
        print(f"[frames] {len(failures)} failure(s)")
        return 1
    print("[frames] OK: the annotation frame is y-down, ours is y-up, and the "
          "way across swaps the ends")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        raise SystemExit(self_test())
    print(__doc__)
