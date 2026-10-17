#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""PubTables-1M cells onto word boxes, and the three frames that meet here (#600).

The learned table structure of #600 is a decision about a PAIR OF WORDS -- same
cell, same row, same column, the table continues -- so every label starts as a
word placed in a cell. This file is that placement and nothing else: no
network, no files, so the arithmetic is reachable from the self-test.

THREE COORDINATE FRAMES, AND TWO OF THEM ARE NOT OURS
=====================================================
Measured on the set, not assumed, because a frame read wrongly does not throw:
it puts every word in the wrong cell and the model learns noise from labels
that look perfectly well-formed.

1. THE CELL ANNOTATIONS ARE Y-DOWN. `pdf_bbox` and `pdf_text_tight_bbox` are in
   PDF units on a page whose `pdf_full_page_bbox` is `[0, 0, 612, 792]`, but y
   is measured from the TOP. The proof is the header row: on PMC6170187 the
   column header's row box is `y0 = 72.9` and the last row's is `y0 = 460.0`,
   and the rows run monotonically downward in between. Under the PDF's own
   convention the top row would carry the LARGEST y.

2. OUR BOXES ARE Y-UP. `Block::bbox`, and the word boxes of the
   `pdfluent.features/1` dump built on it, are PDF user space with y up and
   `y0 < y1`. So a cell box becomes ours by `page_height - y`, WITH THE ENDS
   SWAPPED: their `[y0, y1]` is top-then-bottom and ours is bottom-then-top.
   `page_frame` below is that conversion and `a_cell_and_a_word_meet_after_the_flip`
   is what holds it.

3. THEIR WORD BOXES ARE IN THE TABLE'S CROP, not on the page. A words file is
   one table's crop, scaled: on PMC4688294 table 0 the map from cell box to
   word box is `1.25992 * v - 35` in x and `1.25992 * v - 96` in y, exact to
   the third decimal on all four coordinates over ten unambiguous words. The
   scale is per table, so it is FITTED per table rather than assumed --
   `fit_crop` does that from words whose text matches exactly one cell, and
   refuses rather than guesses when too few agree.

WHY FIT A FRAME WE WILL THROW AWAY. Their word boxes are the stand-in until our
own dump exists (#598); the labels are what we keep. Fitting the crop is also
the cheapest test that both readings above are right, because a wrong reading
of either frame makes the fit's residual explode instead of landing on zero.

WHAT IT PLACES, MEASURED OVER 400 TABLES
========================================
Streamed from both archives, the rig's 210 documents skipped because #598's
leakage guard splits by DOCUMENT. 398 tables aligned, 1 refused a crop fit --
a table of four cells, under the floor and rightly so -- 58,748 words judged:

    78.1 %  placed in a cell
    11.9 %  below the table -- its notes
     8.6 %  above the table -- its caption
     1.3 %  beside it
     0.1 %  INSIDE the table and in no cell

THE PROSE TABLES CAME IN AND BOTH FIGURES IMPROVED, which is the evidence that
matching a cell to a RUN of words fixed something rather than merely allowing
more through: sixty tables were added, placement went 77.3 to 78.1 per cent,
and the words inside a table yet in no cell FELL from 156 to 47. More anchor
points make a truer crop, so the frames land better on the tables that were
already in.

READ 77 % AS THE CROP, NOT AS A MISS. A words file is the table's crop, and the
crop carries the caption and the notes: 22.4 % of the words are outside the
table's own box and SHOULD be placed nowhere. The number that would mean a
defect is the last line, and it is 156 words -- most of them in the tables of
one article, PMC4999353. A caption beside a table is the false positive this
ticket is measured against, so a word left unplaced there is the tool working.

AND ONE THING THAT TURNED OUT NOT TO MATTER. `place` preferred
`pdf_text_tight_bbox` over `pdf_bbox`, on the reasoning that a tight box is the
truer cell. Measured, the two are the same to a tenth of a per cent (77.3
against 77.4), and dropping the overlap floor from a half to three tenths buys
another tenth. The preference is kept because a tight box cannot swallow a
neighbour's word, but nothing here rests on it.
"""
from __future__ import annotations

Box = list[float]


def page_frame(box: Box, page_height: float) -> Box:
    """One annotation box in OUR frame: y up, `y0 < y1`.

    The ends swap because the set measures y downward: their `y0` is the top
    edge, which is the larger y once the page is the right way up.
    """
    x0, top, x1, bottom = box
    return [x0, page_height - bottom, x1, page_height - top]


def _normalise(text: str) -> str:
    """Text as the two sides can be compared: spaces of every kind are one.

    The set's tokens keep a no-break space inside what it calls one word
    (`of\xa0different`), and `str.split` does not. Comparing on the squeezed
    characters lets a cell match the words that compose it whichever side drew
    the line.
    """
    return "".join(text.split())


def _runs(cells: list[dict], words: list[dict]) -> list[tuple[Box, Box]]:
    """Cell boxes against the box of the consecutive words that spell them.

    A cell of ONE word can only be matched by that word; a cell of a sentence
    is matched by the run of words that joins to the same characters. Both
    sides must be unambiguous -- the cell text occurs in exactly one cell, and
    the run occurs exactly once in the word list -- because an ambiguous match
    is a wrong match half the time and the fit cannot tell.
    """
    wanted: dict[str, dict] = {}
    seen: set[str] = set()
    for cell in cells:
        key = _normalise(cell.get("pdf_text_content") or "")
        if not key:
            continue
        if key in wanted:
            del wanted[key]
            seen.add(key)
        elif key not in seen:
            wanted[key] = cell

    joined = [_normalise(w.get("text") or "") for w in words]
    found: dict[str, list[tuple[int, int]]] = {}
    for start in range(len(words)):
        run = ""
        for end in range(start, len(words)):
            run += joined[end]
            if len(run) > 200:
                break
            if run in wanted:
                found.setdefault(run, []).append((start, end))
    pairs: list[tuple[Box, Box]] = []
    for key, where in found.items():
        if len(where) != 1:
            continue
        start, end = where[0]
        boxes = [words[i]["bbox"] for i in range(start, end + 1)]
        cell = wanted[key]
        pairs.append((
            cell.get("pdf_text_tight_bbox") or cell["pdf_bbox"],
            [min(b[0] for b in boxes), min(b[1] for b in boxes),
             max(b[2] for b in boxes), max(b[3] for b in boxes)],
        ))
    return pairs


def fit_crop(cells: list[dict], words: list[dict],
             floor: int = 4) -> tuple[float, float, float] | None:
    """The scale and offset from page units to one table's word crop.

    Returns `(scale, dx, dy)` with `word = scale * page - d`, or `None` when
    fewer than `floor` cells can be matched unambiguously: a fit on three
    points is a fit on noise, and a wrong crop is worse than no crop because it
    still produces labels.

    A CELL OF A SENTENCE COUNTS AS MUCH AS A CELL OF ONE WORD, and that is the
    whole of the change #600 made here. The first spelling matched only cells
    holding a single word, which refused 61 of 399 tables -- and those 61 were
    not the hard ones but the PROSE ones: median unique one-word cells 1
    against 21, with 53 of the 61 under this very floor. A table whose cells
    hold sentences is exactly the table our heuristics already read badly, so
    letting the fit drop it would have trained the model on the tables we are
    already good at.
    """
    pairs = _runs(cells, words)
    if len(pairs) < floor:
        return None

    def solve(here: int, there: int) -> tuple[float, float]:
        source = [p[0][here] for p in pairs] + [p[0][there] for p in pairs]
        target = [p[1][here] for p in pairs] + [p[1][there] for p in pairs]
        count = len(source)
        mean_s = sum(source) / count
        mean_t = sum(target) / count
        spread = sum((v - mean_s) ** 2 for v in source)
        if spread <= 0.0:
            return 1.0, 0.0
        slope = sum((a - mean_s) * (b - mean_t)
                    for a, b in zip(source, target)) / spread
        return slope, mean_t - slope * mean_s

    scale_x, shift_x = solve(0, 2)
    scale_y, shift_y = solve(1, 3)
    scale = (scale_x + scale_y) / 2.0
    return scale, -shift_x, -shift_y


def overlap(a: Box, b: Box) -> float:
    """Area of `a` inside `b`, as a fraction of `a`. Frame-agnostic."""
    wide = min(a[2], b[2]) - max(a[0], b[0])
    tall = min(a[3], b[3]) - max(a[1], b[1])
    if wide <= 0.0 or tall <= 0.0:
        return 0.0
    mine = (a[2] - a[0]) * (a[3] - a[1])
    return 0.0 if mine <= 0.0 else (wide * tall) / mine


def place(words: list[Box], cells: list[dict], page_height: float,
          floor: float = 0.5) -> list[int | None]:
    """Which cell each word sits in, by area, or `None` for none of them.

    `words` are OUR boxes, y up. The cells are converted rather than the words,
    so that the thing being labelled is never moved: a word that ends up
    unplaced is a word the annotation does not cover, which is a fact worth
    counting, and not an artefact of having pushed it somewhere.

    `floor` is the share of the WORD that has to lie in the cell. Half, because
    a word straddling two cells belongs to neither by any honest reading, and a
    cell box that clips its last glyph is the common case rather than the rare
    one.
    """
    boxes = [page_frame(cell.get("pdf_text_tight_bbox") or cell["pdf_bbox"],
                        page_height) for cell in cells]
    out: list[int | None] = []
    for word in words:
        best, best_share = None, floor
        for index, box in enumerate(boxes):
            share = overlap(word, box)
            if share > best_share:
                best, best_share = index, share
        out.append(best)
    return out


def relations(placed: list[int | None], cells: list[dict]) -> dict[tuple[int, int], dict]:
    """The four labels for every pair of placed words.

    A pair is labelled only when BOTH words are in a cell: a pair with an
    unplaced word has no truth here, and calling it "not the same cell" would
    teach the model that the annotation's silence means no.
    """
    rows = [set(cell.get("row_nums") or []) for cell in cells]
    columns = [set(cell.get("column_nums") or []) for cell in cells]
    out: dict[tuple[int, int], dict] = {}
    for i in range(len(placed)):
        for j in range(i + 1, len(placed)):
            left, right = placed[i], placed[j]
            if left is None or right is None:
                continue
            out[(i, j)] = {
                "same_cell": left == right,
                "same_row": bool(rows[left] & rows[right]),
                "same_column": bool(columns[left] & columns[right]),
            }
    return out


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    # --- the flip, on the numbers it was read from ------------------------
    # PMC6170187's header row, measured: the set puts the TOP row at the
    # SMALLEST y, so a reading that forgot the flip would put the header at the
    # foot of the page and every row in the wrong order.
    header = [38.15380859375, 72.908935546875, 575.0654907226562, 89.89229583740234]
    last = [38.15380859375, 459.950439453125, 575.0654907226562, 476.933837890625]
    ours_header = page_frame(header, 792.0)
    ours_last = page_frame(last, 792.0)
    check("the flip turns the page the right way up",
          ours_header[1] > ours_last[3], True,
          "the column header is above the last row on every page ever printed; "
          "if this is false the y axis was read upside down and every word "
          "lands in the wrong row")
    check("and it keeps y0 below y1", ours_header[1] < ours_header[3], True,
          "our frame is `y0 < y1` and a box that violates it overlaps nothing")
    check("and it does not touch x", [ours_header[0], ours_header[2]],
          [header[0], header[2]], "only the y axis disagrees between the frames")

    # --- the crop fit, on the table it was measured on --------------------
    # Ten words of PMC4688294 table 0, cell box against word box. The map is
    # `1.25992 * v - 35` in x and `- 96` in y, exact on all four coordinates.
    scale, dx, dy = 1.2599210498948732, 35.0, 96.0
    cells = []
    words = []
    for index, (text, box) in enumerate([
            ("Extraction", [56.7, 105.6, 92.2, 115.4]),
            ("Precipitation", [124.4, 105.6, 168.8, 115.4]),
            ("114", [321.1, 133.2, 332.8, 142.3]),
            ("73", [321.1, 145.3, 328.9, 154.4]),
            ("Difficult", [391.5, 145.3, 416.5, 154.4])]):
        cells.append({"pdf_text_content": text, "pdf_text_tight_bbox": box,
                      "pdf_bbox": box, "row_nums": [index], "column_nums": [0]})
        words.append({"text": text,
                      "bbox": [scale * box[0] - dx, scale * box[1] - dy,
                               scale * box[2] - dx, scale * box[3] - dy]})
    fitted = fit_crop(cells, words)
    check("the crop is recovered", fitted is not None, True,
          "five unambiguous words is over the floor and the map is exact")
    if fitted:
        got_scale, got_dx, got_dy = fitted
        check("the scale", round(got_scale, 4), round(scale, 4),
              "the crop is scaled per table, so it is fitted and not assumed")
        check("the x shift", round(got_dx, 2), dx, "")
        check("the y shift", round(got_dy, 2), dy, "")
    check("and it refuses a fit on too little",
          fit_crop(cells[:2], words[:2]), None,
          "a fit on two points always succeeds and always lies; a wrong crop "
          "still produces labels, which is why this refuses instead")

    # --- placing a word, and refusing to place one ------------------------
    grid = [
        {"pdf_text_tight_bbox": [10.0, 100.0, 50.0, 110.0], "row_nums": [0],
         "column_nums": [0]},
        {"pdf_text_tight_bbox": [60.0, 100.0, 100.0, 110.0], "row_nums": [0],
         "column_nums": [1]},
        {"pdf_text_tight_bbox": [10.0, 120.0, 50.0, 130.0], "row_nums": [1],
         "column_nums": [0]},
    ]
    height = 200.0
    inside_first = page_frame([12.0, 101.0, 30.0, 109.0], height)
    inside_third = page_frame([12.0, 121.0, 30.0, 129.0], height)
    straddling = page_frame([45.0, 101.0, 65.0, 109.0], height)
    away = page_frame([200.0, 101.0, 220.0, 109.0], height)
    placed = place([inside_first, inside_third, straddling, away], grid, height)
    check("a word in a cell is placed there", placed[0], 0, "")
    check("and a word one row down in the row below", placed[1], 2,
          "the flip has to survive being applied to both, or these two swap")
    check("a word across two cells is placed in neither", placed[2], None,
          "half of a word in a cell is not evidence about that cell, and "
          "picking the larger half would put the pair in one row that the "
          "annotation never claimed")
    check("and a word off the table is unplaced", placed[3], None,
          "running text beside a table is the false positive this whole "
          "ticket is measured against")

    # --- the labels -------------------------------------------------------
    pairs = relations(placed, grid)
    check("only placed pairs are labelled", sorted(pairs), [(0, 1)],
          "a pair with an unplaced word has no truth; labelling it `false` "
          "teaches the model that the annotation's silence means no")
    check("two cells in one column are not one cell",
          pairs[(0, 1)]["same_cell"], False, "")
    check("and they share their column", pairs[(0, 1)]["same_column"], True, "")
    check("and not their row", pairs[(0, 1)]["same_row"], False, "")

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        print(f"\n{len(failures)} problem(s) in the PubTables alignment.")
        return 1
    print("OK pubtables alignment: the annotation is y-down and ours is y-up, "
          "the crop is fitted per table and refused when thin, a word is "
          "placed only where it mostly lies, and only placed pairs carry "
          "labels")
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(self_test() if "--self-test" in sys.argv else self_test())
