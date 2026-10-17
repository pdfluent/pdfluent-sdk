# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The block feature vector, read from a `pdfluent.features/1` record (#598).

One list, in one order, shared by training and by the parity fixtures: the
exported Rust model takes its input in this order and names it in its header,
so a reordering is a different model and not a silent shift.

A missing value (a neighbour on the first block, a field the driver has not
filled) is NaN, which LightGBM and the exported code route the same way.
"""
from __future__ import annotations

import math
import sys

# /18 adds `final_tables` -- every table the pass would emit, with `bbox`
# (user space, y up), the proposer kind, the feature vector in
# `final_table_names` order, and the engine/decisions stamp -- for the
# final-table judge (#574, #638), and is otherwise /17.
SCHEMA = "pdfluent.features/18"
# /2 adds table/row/column/row_span/column_span on table cells (#600) and is
# otherwise /1, so a /1 dump still trains every block model. /3 adds the page's
# horizontal rules (`rules`: left, right, y) beside the blocks and is otherwise
# /2; no block feature reads them yet. /4 adds `pairs` -- the pair features
# the ENGINE computed, `[i, j, f0 .. f24]` over `pair_names`, for the word
# pairs in a table region -- and is otherwise /3. /6 adds `table_proposals`
# -- every table region a detector of the engine accepted, as the indices of
# the page's lines it holds (`lines_of` order, block by block), with its
# detector and hull -- for the table judge (#574), and is otherwise /5.
# /7 adds `list_signal` -- per line, in that same order, the larger of the list
# models' `list_item` and running `list_opens` probabilities, NaN on a line
# of a kind they do not read -- for the judge as well (#574), and is
# otherwise /6. A refined record does not carry it. /8 adds `line_class_probs`
# -- per line, in that same order, the line model's probabilities of other,
# heading, list and table (`heading_lines.LINE_CLASSES` in the engine) -- for
# the list model and the judge (#574), and is otherwise /7. /9 adds `images`
# -- the boxes the page draws its images into, `[left, bottom, right, top]` in
# the frame of `bbox` -- for the caption lane (#636), and is otherwise /8.
# /10 adds `rulings` -- every thin line and rectangle edge as `[x0, y0, x1, y1]`
# -- and `fills` -- every solid, non-white, axis-aligned fill -- for the line
# network's vector inputs (#574), and is otherwise /9.
# /11 is T3's retrain record, on its own branch. /12 adds `line_image` -- per
# line, in the same order as `line_vector`, the six image features the raster
# reads (`learned::raster::IMAGE_FEATURES`), or null where the page did not
# render -- for the line network's image arm (#574), and is otherwise /10.
# THE LINEAGE, from /12 on: numbers follow LANDING ORDER, and each one is a
# superset of the master number before it (COS, 01-10-2026).
#   /11 -- /10 plus each word's `font` and `color` and each block's `judged`;
#          T3's retrain data (features-v11), made on its own branch, never a
#          master number. A /11 record may lack /10's `p_heading_pass`.
#   /12 -- /10 plus `line_image` (T5).
#   /13 -- reserved, parked (the gutter work).
#   /14 -- lapsed: claimed by the page-confidence work, never landed under it.
#   /15 -- lapsed: claimed for the detector votes, which take the next free
#          number when they land.
#   /16 -- /12 plus each word's `font` and `color` and each block's `judged`
#          (T3, the retrained line network).
#   /17 -- /16 plus `confidence` -- the learned pass's doubt about the page
#          (`doubt`, `heading_doubt`, `link_doubt`, `class_entropy`; the last
#          two null where the network was off) -- for the router that hands
#          doubtful pages to the teacher.
#   /18 -- /17 plus `final_tables` -- every table the pass would emit,
#          with its final-table vector -- for the final-table judge
#          (#574, #638).
#
# A SET AND NOT A MINIMUM, because every version so far only ADDS keys: a
# reader of /1 reads a /4 dump. The set is what says so out loud, and a version
# that ever removed or changed a key would be left out of it.
READABLE = {"pdfluent.features/1", "pdfluent.features/2", "pdfluent.features/3",
            "pdfluent.features/4", "pdfluent.features/5", "pdfluent.features/6",
            "pdfluent.features/7", "pdfluent.features/8",
            "pdfluent.features/9",
            "pdfluent.features/10",
            "pdfluent.features/11",
            "pdfluent.features/12",
            "pdfluent.features/16",
            "pdfluent.features/17",
            "pdfluent.features/18",
            # The alias of /18: the name every v2 training dump carries, from
            # before landing assigned the number. A retrain must still read
            # those dumps; nothing writes this name any more (#638).
            "pdfluent.features/final-tables-next"}

BLOCK_FEATURES: list[str] = [
    "size_ratio", "bold", "italic", "all_caps",
    "n_chars", "n_words", "n_lines",
    "x_frac", "width_frac", "y_frac",
    "gap_above_em", "gap_below_em",
    "ends_terminal", "ends_colon", "section_number",
    "has_marker", "has_caption",
    "first_on_page", "last_on_page",
    "prev_ends_terminal", "prev_size_ratio", "next_x_frac", "next_size_ratio",
    "repeats_in_document",
]


def _num(value) -> float:
    if value is None:
        return math.nan
    if isinstance(value, bool):
        return 1.0 if value else 0.0
    return float(value)


def block_vector(block: dict) -> list[float]:
    derived = {
        "has_marker": block.get("marker") is not None,
        "has_caption": block.get("caption") is not None,
    }
    return [_num(derived[name] if name in derived else block.get(name)) for name in BLOCK_FEATURES]


def line_image_rows(record: dict) -> list[list[float]]:
    """The page's `line_image` rows, one per line in `lines_of` order.

    A page whose render failed carries `line_image: null` -- never a row of
    nulls, never a row of defaults -- and reads here as a NaN row for every
    line of the page (#574). NaN, not zero: zeros would read as a blank page,
    and NaN is the missing value every trainer already routes the way the
    exported code does. A record from before /12 has no key at all and reads
    the same way.
    """
    import pathlib
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
    from align_labels import lines_of  # noqa: E402
    from raster import IMAGE_FEATURES  # noqa: E402
    words = record.get("words") or []
    n = sum(1 for block in record.get("blocks") or []
            for line in lines_of(block, words) if line)
    image = record.get("line_image")
    if image is None:
        return [[math.nan] * len(IMAGE_FEATURES) for _ in range(n)]
    return [[_num(v) for v in row] for row in image]


def self_test() -> int:
    """A null page reads as NaN on every line; a rendered one reads as dumped."""
    failures = []

    def check(name, got, want):
        ok = got == want
        print(f"  {'ok' if ok else 'FAIL'}  {name}: {got!r}")
        if not ok:
            failures.append(name)

    def isnan_row(row):
        return len(row) == 6 and all(isinstance(v, float) and math.isnan(v) for v in row)

    # Three lines: two words side by side on block 0, two stacked on block 1.
    record = {
        "blocks": [{"i": 0}, {"i": 1}],
        "words": [
            {"block": 0, "bbox": [0, 0, 50, 10]},
            {"block": 0, "bbox": [60, 0, 110, 10]},
            {"block": 1, "bbox": [0, 0, 50, 10]},
            {"block": 1, "bbox": [0, 20, 50, 30]},
        ],
        "line_image": None,
    }
    got = line_image_rows(record)
    check("a null page has one row per line", len(got), 3)
    check("and every row is six NaNs", all(isnan_row(r) for r in got), True)
    check("a record from before /12 reads the same",
          all(isnan_row(r) for r in line_image_rows({k: v for k, v in record.items()
                                                     if k != "line_image"})),
          True)
    check("a page with no lines has no rows",
          line_image_rows({"blocks": [], "words": [], "line_image": None}), [])

    dumped = [[0.5, 0.4, 1.0, 1.0, 0.0, 0.1],
              [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
              [0.2, 0.3, 0.9, 0.0, 1.0, 0.0]]
    check("a rendered page reads as dumped",
          line_image_rows({**record, "line_image": dumped}), dumped)
    with_hole = [row[:] for row in dumped]
    with_hole[1][2] = None
    hole = line_image_rows({**record, "line_image": with_hole})[1]
    check("a null inside a row is NaN", math.isnan(hole[2]) and hole[:2] == [0.0, 1.0], True)

    check("the schema is /18", SCHEMA, "pdfluent.features/18")
    check("/16 stays readable", "pdfluent.features/16" in READABLE, True)
    check("/17 stays readable", "pdfluent.features/17" in READABLE, True)
    check("/18 reads too", "pdfluent.features/18" in READABLE, True)
    check("the v2 dumps' name reads as the alias of /18",
          "pdfluent.features/final-tables-next" in READABLE, True)
    check("nothing writes the alias any more",
          SCHEMA == "pdfluent.features/final-tables-next", False)
    # /17 is /16 plus `confidence`, which no feature reader looks at: a
    # record of each shape reads the same rows.
    shaped = {**record, "line_image": dumped}
    before = {"schema": "pdfluent.features/16", **shaped}
    after = {**before, "schema": "pdfluent.features/17",
             "confidence": {"doubt": 0.1, "heading_doubt": 0.1,
                            "link_doubt": None, "class_entropy": None}}
    check("a /16 and a /17 record read the same rows",
          line_image_rows(after), line_image_rows(before))
    # /18 is /17 plus `final_tables`, which no feature reader looks at
    # either: the same rows again.
    latest = {**after, "schema": "pdfluent.features/18", "final_tables": []}
    check("a /17 and a /18 record read the same rows",
          line_image_rows(latest), line_image_rows(after))

    print(f"features: {'FAIL' if failures else 'OK'} ({len(failures)} failure(s))")
    return 1 if failures else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    print(__doc__)
