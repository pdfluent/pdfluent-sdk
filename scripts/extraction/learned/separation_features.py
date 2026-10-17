#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A gap between two bands of words, and what a model may know about it (#574).

ONE IMPLEMENTATION, IMPORTED TWICE. The AUC probe and the trainer both build
their candidates here, because the alternative is two spellings of "what a gap
is" that agree until they do not -- which is the defect that cost this pair of
tickets seven instruments, the worst of them a harness whose baseline column
ran the model it was the baseline for.

WHY A SEPARATION AND NOT A PAIR. Our words in the annotation's grid score TEDS
0.9404 where our own grid scores 0.5996: the whole loss is the grid. The pair
model buys 0.0077 of that, because it asks "do these two words belong
together" and closes the answer transitively, so one wrong pair walks through
the closure. A gap is asked once -- a median of 11 row gaps per table against
thousands of pairs -- and a wrong answer stays one wrong row.

WHAT THE FEATURES ARE, and why the raw gap leads. Measured over 1,143 tables:
the raw gap separates row boundaries at AUC 0.803 pooled and 0.953 WITHIN a
table, and column boundaries at 0.911. Two attempts to fold the per-table
scale into one number both lost to it -- dividing by the table's median reads
0.755 on rows, its rank 0.678 -- because 83 per cent of row gaps ARE
boundaries, so the median gap is itself a boundary and dividing by it erases
the size that was doing the work. The table's own statistics are therefore
carried as SEPARATE features and left for the model to combine, rather than
baked into a ratio that has already thrown the scale away.
"""
from __future__ import annotations

import statistics

ROW_NAMES = [
    "gap", "gap_in_text_heights", "gap_over_table_median", "gap_over_table_tightest",
    "gap_rank_in_table", "table_median_gap", "table_tightest_gap", "gaps_in_table",
    "numeric_share_above", "numeric_share_below",
    "numeric_change", "words_above", "words_below", "left_edges_agree",
    "right_edges_agree", "text_height", "band_height_above", "band_height_below",
]

COLUMN_NAMES = [
    "gap", "gap_in_word_widths", "gap_over_table_median", "gap_over_table_tightest",
    "gap_rank_in_table", "table_median_gap", "table_tightest_gap", "gaps_in_table",
    "numeric_share_left", "numeric_share_right", "numeric_change",
    "words_left", "words_right", "top_edges_agree", "band_width_left",
    "band_width_right", "word_width",
]


def bands(items, low, high, order):
    """Group boxes that overlap along one axis into bands, in reading order."""
    items = sorted(items, key=order)
    out: list[list] = []
    for item in items:
        if out and any(low(item) <= high(x) and high(item) >= low(x) for x in out[-1]):
            out[-1].append(item)
        else:
            out.append([item])
    return out


def _scale(values: list[float]) -> tuple[float, float]:
    """The table's median gap and its tightest tenth, both never zero."""
    positive = sorted(v for v in values if v > 0)
    if not positive:
        return 1.0, 1.0
    median = statistics.median(positive) or 1.0
    tightest = positive[max(0, len(positive) // 10 - 1)] or 1.0
    return median, tightest


def _agree(values: list[float], slack: float) -> float:
    """Whether the edges of a band line up, as a share of its words."""
    if len(values) < 2:
        return 1.0
    first = values[0]
    return sum(1 for v in values if abs(v - first) <= slack) / len(values)


def a_digit_in(text: str) -> bool:
    """Whether the word holds a digit, by the ONE definition both sides use.

    ASCII AND NOT `str.isdigit()`. Python's `isdigit` is true for Arabic-Indic
    digits and for superscripts, and Rust has no builtin that means exactly the
    same set -- `is_ascii_digit` is narrower and `is_numeric` is wider. A
    feature whose definition differs between training and serving is the model
    reading input it never saw, and it announces itself as nothing at all.
    On the PubTables dump the two readings disagree on 9 words in 926,823
    (0.001 per cent), so agreeing on the narrow one costs nothing measurable
    and buys an exact port (codex, #574).
    """
    return any(c.isascii() and c.isdigit() for c in text)


def _numeric(band) -> float:
    return sum(1 for w in band if a_digit_in(w["text"])) / max(1, len(band))


def row_candidates(words, rules, text_height):
    # `rules` is kept in the signature and NOT read. The engine's learned pass
    # is handed blocks, not the page's rulings, and `refine_page` would have to
    # change shape for every caller to carry them. Serving a constant zero
    # where training saw a real flag is the train/serve skew this lane has
    # paid for repeatedly, so the feature is gone rather than faked: it
    # separates row boundaries at AUC 0.577 on PubTables and is not in the
    # model's top five by gain, so it is not what this turns on. Whether it
    # earns the plumbing on ruled financial tables is measured in its own
    # round (COS, #574).
    """Every gap between two bands down the page, with its features.

    Yields `(above, below, features)`, top band first, y up.
    """
    lines = bands(words, lambda w: w["bbox"][1], lambda w: w["bbox"][3],
                  lambda w: -w["bbox"][3])
    gaps = [min(w["bbox"][1] for w in a) - max(w["bbox"][3] for w in b)
            for a, b in zip(lines, lines[1:])]
    median, tightest = _scale(gaps)
    ordered = sorted(gaps)
    for (above, below), gap in zip(zip(lines, lines[1:]), gaps):
        up, down = _numeric(above), _numeric(below)
        yield above, below, [
            gap,
            gap / max(1e-6, text_height),
            gap / median,
            gap / tightest,
            sum(1 for g in ordered if g < gap) / max(1, len(ordered)),
            median,
            tightest,
            float(len(gaps)),
            up,
            down,
            abs(up - down),
            float(len(above)),
            float(len(below)),
            _agree([w["bbox"][0] for w in above], text_height / 2),
            _agree([w["bbox"][2] for w in above], text_height / 2),
            text_height,
            max(w["bbox"][3] for w in above) - min(w["bbox"][1] for w in above),
            max(w["bbox"][3] for w in below) - min(w["bbox"][1] for w in below),
        ]


def column_candidates(words, word_width):
    """Every gap between two bands across the page, with its features."""
    stripes = bands(words, lambda w: w["bbox"][0], lambda w: w["bbox"][2],
                    lambda w: w["bbox"][0])
    gaps = [min(w["bbox"][0] for w in b) - max(w["bbox"][2] for w in a)
            for a, b in zip(stripes, stripes[1:])]
    median, tightest = _scale(gaps)
    ordered = sorted(gaps)
    for (left, right), gap in zip(zip(stripes, stripes[1:]), gaps):
        a, b = _numeric(left), _numeric(right)
        yield left, right, [
            gap,
            gap / max(1e-6, word_width),
            gap / median,
            gap / tightest,
            sum(1 for g in ordered if g < gap) / max(1, len(ordered)),
            median,
            tightest,
            float(len(gaps)),
            a,
            b,
            abs(a - b),
            float(len(left)),
            float(len(right)),
            _agree([w["bbox"][3] for w in left], word_width / 2),
            max(w["bbox"][2] for w in left) - min(w["bbox"][0] for w in left),
            max(w["bbox"][2] for w in right) - min(w["bbox"][0] for w in right),
            word_width,
        ]


# A WORD THAT BRIDGES TWO COLUMNS, and why it is asked about before the
# columns are (#574).
#
# A stripe is every word whose extent overlaps another's across the page, so
# ONE header drawn over two columns joins them into one stripe -- and with it
# every body cell below, in every row. On the training half of PubTables that
# happens to 47 per cent of the spanning header cells, and it is where the
# merged cells cost: leaving the annotation's spanning words out of the stripes
# and giving them the columns they cover lifts the grid from 0.7999 to 0.8379,
# where every post-hoc merge rule tried on the finished grid bought 0.007 at
# most. So the question is asked per word, before the stripes are built.
BRIDGE_NAMES = [
    "width_in_word_widths", "width_over_stripe", "lines_in_stripe",
    "words_in_stripe", "line_rank", "line_index", "words_in_line", "numeric",
    "bands_below_overlapped", "bands_below", "bands_other_overlapped",
    "share_of_stripe_lines_below", "stripes_in_table", "lines_below_overlapped_share",
]


def _across(items):
    return bands(items, lambda w: w["bbox"][0], lambda w: w["bbox"][2],
                 lambda w: w["bbox"][0])


def _overlaps(word, band) -> bool:
    return any(word["bbox"][0] < x["bbox"][2] and word["bbox"][2] > x["bbox"][0]
               for x in band)


def bridge_candidates(words, word_width):
    """Every word with its features, in the order of `words`.

    THE LINES BELOW ARE THE EVIDENCE. A header sits above the body it spans,
    so the bands the words UNDER a word form -- within its own stripe -- say
    whether the stripe is really two columns joined by it. The bands of every
    other line are carried too, for the header whose body is above it.
    """
    lines = bands(words, lambda w: w["bbox"][1], lambda w: w["bbox"][3],
                  lambda w: -w["bbox"][3])
    line_of = {id(w): i for i, band in enumerate(lines) for w in band}
    stripes = _across(words)
    out = {}
    for stripe in stripes:
        mine = sorted({line_of[id(w)] for w in stripe})
        left = min(w["bbox"][0] for w in stripe)
        right = max(w["bbox"][2] for w in stripe)
        # Per LINE and not per word: every word of one line in one stripe
        # has the same words below it and the same others beside it.
        by_line = {}
        for own in mine:
            below = [x for x in stripe if line_of[id(x)] > own]
            by_line[own] = (below, _across(below),
                            _across([x for x in stripe if line_of[id(x)] != own]),
                            len({line_of[id(x)] for x in below}))
        for w in stripe:
            own = line_of[id(w)]
            below, under, beside, lines_below = by_line[own]
            hit_lines = {line_of[id(x)] for x in below
                         if w["bbox"][0] < x["bbox"][2] and w["bbox"][2] > x["bbox"][0]}
            width = w["bbox"][2] - w["bbox"][0]
            out[id(w)] = [
                width / max(1e-6, word_width),
                width / max(1e-6, right - left),
                float(len(mine)),
                float(len(stripe)),
                own / max(1, len(lines) - 1),
                float(own),
                float(len(lines[own])),
                1.0 if a_digit_in(w["text"]) else 0.0,
                float(sum(1 for band in under if _overlaps(w, band))),
                float(len(under)),
                float(sum(1 for band in beside if _overlaps(w, band))),
                lines_below / max(1, len(mine)),
                float(len(stripes)),
                len(hit_lines) / max(1, lines_below),
            ]
    return [(w, out[id(w)]) for w in words]
