#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What a pair of words looks like, and what the annotation says it is (#600).

One row per pair of words on a table: the geometry between them, the shape of
what they say, and one label of four -- same cell, same row only, same column
only, neither.

ONE LABEL AND NOT FOUR, by the owner's decision of 23-09-2026. Four independent
answers can contradict each other -- "same cell" while denying "same row" is a
grid that means nothing -- and a rule bolted on afterwards to catch that is a
rule that fires where nobody looks. Classes that exclude one another cannot
disagree in the first place.

AND THEY ALMOST COVER THE DATA. A pair in two DIFFERENT cells that shares both
a row and a column has no class here, and that shape exists wherever a row span
crosses a column span. Counted over 4.2 million pairs on 400 tables: zero -- so
the first version of this note said the classes were exhaustive.

THEY ARE NOT, and the raise below is what found that out. On a different
population, the 4,000 fetched pages, the same count is ONE crossing pair in
20.3 million, in a single table. Vanishingly rare, and not impossible; "zero
on the sample I looked at" was a statement about the sample.

So `label` still raises rather than folding the pair into `neither` -- teaching
the model that a crossing span is nothing would be worse than stopping -- and a
caller that walks millions of pairs catches it and counts it, because a shape
that occurs once in twenty million should not end a training run either. The
count belongs in the run's output where somebody reads it.

EVERY FEATURE IS ONE WE WILL ALSO HAVE AT READING TIME. The words of the set
carry a line number, a block number and font flags; `pdfluent.features/1`
carries a block, a span, a size and bold and italic. Nothing here reads any of
them. The features are the two boxes and the two strings, because those are the
only things both sides are sure to hold, and a feature that exists only in
training is a feature that will be missing exactly when it is needed.
"""
from __future__ import annotations

def candidates(words: list[dict], nearest: int = 6) -> list[tuple[int, int]]:
    """The pairs worth asking about: neighbours, not every pair on the table.

    Every pair is O(n squared), and on a table of two hundred words that is
    twenty thousand questions of which almost all are a word in the top left
    and a word in the bottom right -- trivially `neither`, and in such numbers
    that they drown both the training and the score. The relations that build a
    grid are LOCAL; rows and columns come back out through the decoder's
    transitivity, so they need not be asked about directly.

    Three kinds, unioned:

      * the next word along the line -- the nearest to the right whose box
        overlaps this one vertically, which is what a same-cell relation is
        mostly made of;
      * the nearest word below whose box overlaps this one horizontally, which
        is what a column is made of;
      * the `nearest` closest by centre distance, so that nothing is invisible
        merely because a neighbour sits diagonally.
    """
    found: set[tuple[int, int]] = set()
    boxes = [w["bbox"] for w in words]

    def overlaps(a, b, vertical: bool) -> float:
        i, j = (1, 3) if vertical else (0, 2)
        wide = min(a[j], b[j]) - max(a[i], b[i])
        return wide / max(1e-6, min(a[j] - a[i], b[j] - b[i]))

    for i, here in enumerate(boxes):
        right = down = None
        right_gap = down_gap = float("inf")
        for j, there in enumerate(boxes):
            if i == j:
                continue
            if there[0] >= here[2] and overlaps(here, there, True) > 0.5:
                if there[0] - here[2] < right_gap:
                    right, right_gap = j, there[0] - here[2]
            if there[3] <= here[1] and overlaps(here, there, False) > 0.0:
                if here[1] - there[3] < down_gap:
                    down, down_gap = j, here[1] - there[3]
        for j in (right, down):
            if j is not None:
                found.add((min(i, j), max(i, j)))
        centre = sorted(
            (k for k in range(len(boxes)) if k != i),
            key=lambda k: ((boxes[k][0] + boxes[k][2] - here[0] - here[2]) ** 2
                           + (boxes[k][1] + boxes[k][3] - here[1] - here[3]) ** 2))
        for j in centre[:nearest]:
            found.add((min(i, j), max(i, j)))
    return sorted(found)


NAMES = [
    "gap_x", "gap_y", "centre_dx", "centre_dy",
    "overlap_x", "overlap_y", "left_aligned", "right_aligned", "centre_aligned",
    "height_ratio", "width_ratio", "gap_x_em", "gap_y_em",
    "same_band", "reading_distance",
    "both_numeric", "either_numeric", "length_ratio", "either_ends_colon",
    "either_all_caps",
    # WHAT OUR OWN HEURISTIC ALREADY THINKS (schema /2). Every word may carry
    # the table, row and column of the `table_cell` block it came from, which
    # is our current table reading. Given as a PRIOR and not as an answer: the
    # model is free to disagree, and on a page where the heuristic finds no
    # table at all these are simply "neither of us is in one".
    "both_in_a_table", "our_same_table", "our_same_row", "our_same_column",
    "our_same_cell",
]

CLASSES = ["neither", "same_column", "same_row", "same_cell"]


def _numeric(text: str) -> bool:
    body = text.strip().strip("()[]%*†‡§,.;:")
    if not body:
        return False
    body = body.replace(",", "").replace("±", "").replace("−", "-").replace("–", "-")
    try:
        float(body)
        return True
    except ValueError:
        return False


def features(a: dict, b: dict) -> list[float]:
    """One pair, in the order [`NAMES`], with the earlier word first.

    Ordered by reading order rather than by whichever the caller passed, so
    that a pair has one description and not two mirror images of it -- the
    model would otherwise have to learn the symmetry from examples instead of
    being handed it.
    """
    first, second = sorted((a, b), key=lambda w: (-w["bbox"][3], w["bbox"][0]))
    ax0, ay0, ax1, ay1 = first["bbox"]
    bx0, by0, bx1, by1 = second["bbox"]
    ah, aw = max(1e-6, ay1 - ay0), max(1e-6, ax1 - ax0)
    bh, bw = max(1e-6, by1 - by0), max(1e-6, bx1 - bx0)
    em = max(1e-6, (ah + bh) / 2.0)

    gap_x = max(0.0, max(ax0, bx0) - min(ax1, bx1))
    gap_y = max(0.0, max(ay0, by0) - min(ay1, by1))
    over_x = max(0.0, min(ax1, bx1) - max(ax0, bx0)) / min(aw, bw)
    over_y = max(0.0, min(ay1, by1) - max(ay0, by0)) / min(ah, bh)
    at, bt = first["text"], second["text"]
    return [
        gap_x, gap_y,
        (bx0 + bx1) / 2.0 - (ax0 + ax1) / 2.0,
        (by0 + by1) / 2.0 - (ay0 + ay1) / 2.0,
        over_x, over_y,
        abs(ax0 - bx0) / em, abs(ax1 - bx1) / em,
        abs((ax0 + ax1) - (bx0 + bx1)) / (2.0 * em),
        ah / bh, aw / bw,
        gap_x / em, gap_y / em,
        1.0 if over_y > 0.5 else 0.0,
        (gap_x + gap_y) / em,
        1.0 if (_numeric(at) and _numeric(bt)) else 0.0,
        1.0 if (_numeric(at) or _numeric(bt)) else 0.0,
        len(at) / max(1, len(bt)),
        1.0 if (at.endswith(":") or bt.endswith(":")) else 0.0,
        1.0 if (at.isupper() or bt.isupper()) else 0.0,
        *_what_we_think(first, second),
    ]


def _what_we_think(a: dict, b: dict) -> tuple[float, ...]:
    """Our own reading of the two words, as five flags.

    A word outside any table of ours carries nothing, and then every flag is
    zero -- which is the honest encoding: not "they disagree" but "we have no
    opinion". A model given -1 for absent would learn a magic number.
    """
    at, bt = a.get("table"), b.get("table")
    if at is None or bt is None:
        return (0.0, 0.0, 0.0, 0.0, 0.0)
    same_table = at == bt
    if not same_table:
        return (1.0, 0.0, 0.0, 0.0, 0.0)
    same_row = a.get("row") is not None and a.get("row") == b.get("row")
    same_column = a.get("column") is not None and a.get("column") == b.get("column")
    return (1.0, 1.0, float(same_row), float(same_column),
            float(same_row and same_column))


def label(rows_a: set, cols_a: set, rows_b: set, cols_b: set,
          same_cell: bool) -> int:
    """The one class of four. Raises on the pair the four cannot name."""
    if same_cell:
        return CLASSES.index("same_cell")
    shares_row = bool(rows_a & rows_b)
    shares_column = bool(cols_a & cols_b)
    if shares_row and shares_column:
        raise ValueError(
            "a pair in two cells sharing both a row and a column has no class "
            "here; counted zero times over 4.2 million pairs, so this is a new "
            "shape and not a rounding error")
    if shares_row:
        return CLASSES.index("same_row")
    if shares_column:
        return CLASSES.index("same_column")
    return CLASSES.index("neither")


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    left = {"text": "Region", "bbox": [10.0, 90.0, 60.0, 100.0]}
    right = {"text": "12,345", "bbox": [70.0, 90.0, 130.0, 100.0]}
    below = {"text": "North", "bbox": [10.0, 70.0, 60.0, 80.0]}

    check("a pair is described one way round",
          features(left, right), features(right, left),
          "the pair has one description and not two mirror images; the model "
          "should not have to learn the symmetry from examples")
    row = dict(zip(NAMES, features(left, right)))
    check("two words on a line overlap vertically", row["same_band"], 1.0, "")
    check("and are apart horizontally", row["gap_x"], 10.0, "")
    check("and not vertically", row["gap_y"], 0.0, "")
    column = dict(zip(NAMES, features(left, below)))
    check("two words in a column do not", column["same_band"], 0.0, "")
    check("and are apart vertically", column["gap_y"], 10.0, "")
    check("and share a left edge", column["left_aligned"], 0.0, "")
    check("a figure is read as one", row["both_numeric"], 0.0,
          "`Region` is not a number and `12,345` is, so only one of them")
    check("and a pair of them as two",
          dict(zip(NAMES, features(right, {"text": "23,456",
                                           "bbox": [70.0, 70.0, 130.0, 80.0]})))["both_numeric"],
          1.0, "a comma is a thousands mark and not a decimal point here")

    # --- candidates are neighbours ---------------------------------------
    line = [{"text": t, "bbox": [x, 90.0, x + 20.0, 100.0]}
            for t, x in (("a", 10.0), ("b", 40.0), ("c", 70.0))]
    line.append({"text": "d", "bbox": [10.0, 70.0, 30.0, 80.0]})
    near = candidates(line, nearest=1)
    check("the word along the line is a candidate", (0, 1) in near, True, "")
    check("and the word below it", (0, 3) in near, True, "")
    check("and the far end of the line is not, by these two rules alone",
          (0, 2) in candidates(line[:3], nearest=0), False,
          "a word two along is reached through its neighbour, and asking "
          "about it directly is the O(n squared) this avoids")

    check("the class of one cell", label({0}, {0}, {0}, {0}, True), 3, "")
    check("of a row", label({0}, {0}, {0}, {1}, False), 2, "")
    check("of a column", label({0}, {0}, {1}, {0}, False), 1, "")
    check("of neither", label({0}, {0}, {1}, {1}, False), 0, "")
    try:
        label({0}, {0, 1}, {0, 1}, {0}, False)
        failures.append("a pair sharing a row AND a column was given a class; "
                        "it has none, and folding it into `neither` would "
                        "teach the model that a crossing span is nothing")
    except ValueError:
        pass

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        print(f"\n{len(failures)} problem(s) in the pair features.")
        return 1
    print("OK pair features: a pair has one description, the four classes "
          "exclude each other, and the pair they cannot name raises")
    return 0


if __name__ == "__main__":
    raise SystemExit(self_test())


CELL_NAMES = [
    "words", "chars", "width_em", "height_em", "lines_guess",
    "x_frac", "y_frac", "width_frac", "height_frac",
    "height_over_median_line", "width_over_median_word",
    "numeric_share", "starts_upper", "ends_colon", "is_top_band",
    # THE SPAN, ALMOST DIRECTLY (owner, 23-09-2026). How many bands of the
    # words OUTSIDE this cell its vertical extent covers, and where its centre
    # falls among them: a cell centred between two neighbouring rows is a
    # two-row span by definition, and none of this needs a ruling line.
    "neighbour_bands_covered", "centre_between_bands", "band_of_centre_share",
]


def cell_features(words: list[dict], members: list[int],
                  page: dict) -> list[float]:
    """One CELL, for the questions that are about a cell and not about a pair.

    How many rows a cell spans is not a fact about any pair of its words -- the
    measurement of 23-09 showed a pairwise question cannot reach it at all --
    so it needs its own description. `page` carries what the cell has to be
    judged against: the table's box and its median line pitch, because a cell
    two lines tall is only tall relative to the lines around it.
    """
    boxes = [words[i]["bbox"] for i in members]
    left = min(b[0] for b in boxes)
    right = max(b[2] for b in boxes)
    bottom = min(b[1] for b in boxes)
    top = max(b[3] for b in boxes)
    em = max(1e-6, page["median_height"])
    texts = [words[i]["text"] for i in members]
    joined = " ".join(texts)
    tops = sorted({round(words[i]["bbox"][3], 1) for i in members})
    table_w = max(1e-6, page["right"] - page["left"])
    table_h = max(1e-6, page["top"] - page["bottom"])
    covered, between, share = _neighbour_bands(words, members, page)
    return [
        float(len(members)),
        float(len(joined)),
        (right - left) / em,
        (top - bottom) / em,
        float(len(tops)),
        (left - page["left"]) / table_w,
        (page["top"] - top) / table_h,
        (right - left) / table_w,
        (top - bottom) / table_h,
        (top - bottom) / max(1e-6, page["median_pitch"]),
        (right - left) / max(1e-6, page["median_width"]),
        sum(1 for t in texts if _numeric(t)) / max(1, len(texts)),
        1.0 if joined[:1].isupper() else 0.0,
        1.0 if joined.endswith(":") else 0.0,
        1.0 if top >= page["top"] - page["median_pitch"] else 0.0,
        covered, between, share,
    ]


def _neighbour_bands(words: list[dict], members: list[int],
                     page: dict) -> tuple[float, float, float]:
    """This cell's extent against the rows the OTHER words are written on.

    The bands are built from the words this cell does not hold, so they are the
    rows of its neighbours; a cell that covers two of them reaches over two
    rows whatever it says, and a cell whose centre falls in the gap BETWEEN two
    of them is the span that no single band explains.

    A cell holding every word on the page has no neighbours and gets zeroes,
    which is honest: there is nothing to be spanned.
    """
    mine = set(members)
    others = [words[i]["bbox"] for i in range(len(words)) if i not in mine]
    if not others:
        return 0.0, 0.0, 0.0
    tops = sorted({round(b[3], 1) for b in others}, reverse=True)
    pitch = max(1e-6, page["median_pitch"])
    bands: list[list[float]] = []
    for edge in tops:
        if bands and bands[-1][0] - edge < pitch * 0.5:
            bands[-1][1] = min(bands[-1][1], edge)
        else:
            bands.append([edge, edge])
    boxes = [words[i]["bbox"] for i in members]
    bottom = min(b[1] for b in boxes)
    top = max(b[3] for b in boxes)
    middle = (bottom + top) / 2.0
    covered = sum(1 for high, low in bands
                  if min(top, high) >= max(bottom, low - pitch * 0.5))
    inside = [n for n, (high, low) in enumerate(bands)
              if low - pitch * 0.5 <= middle <= high + pitch * 0.5]
    return (float(covered),
            0.0 if inside else 1.0,
            float(len(inside)) / max(1, len(bands)))


def page_stats(words: list[dict]) -> dict:
    """What a cell is measured against: the table's box and its own rhythm."""
    boxes = [w["bbox"] for w in words]
    heights = sorted(b[3] - b[1] for b in boxes)
    widths = sorted(b[2] - b[0] for b in boxes)
    tops = sorted({round(b[3], 1) for b in boxes}, reverse=True)
    steps = sorted(a - b for a, b in zip(tops, tops[1:])) or [1.0]
    middle = lambda xs: xs[len(xs) // 2] if xs else 1.0
    return {
        "left": min(b[0] for b in boxes), "right": max(b[2] for b in boxes),
        "bottom": min(b[1] for b in boxes), "top": max(b[3] for b in boxes),
        "median_height": middle(heights), "median_width": middle(widths),
        "median_pitch": middle(steps),
    }
