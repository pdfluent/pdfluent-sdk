#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The line feature vector for list structure: does this line open an item (#599).

    list_lines.py --self-test

WHY LINES AND NOT BLOCKS

A list's worst defect lives inside one of our blocks: a continuation line that
swallows the next item, so two items come out as one block and the second is
gone (#589: the merge the join could not see until T3 counted it from the
region's side). A block classifier cannot repair that -- the block is already
wrong -- so the list lane asks per LINE: does a new item open here? Where the
answer is yes inside a block, the block is split there.

THE LINES ARE T3'S. `align_labels.lines_of` groups a block's words into lines,
and the label (`opens`) is computed on exactly those lines. Grouping them a
second way here would train on one set of lines and label another.

WHAT A LINE SAYS

The marker at its head (a bullet, a number, a figure sign that is NOT a marker),
the white after it, where the line starts against its block and against the
line above, how far short of the block's measure the line above stopped, and
whether the line above ended mid-clause. Those are the facts the heuristics in
`crates/pdf-engine/src/blocks.rs` and `lists.rs` read; the model decides how
they combine instead of a hand-set threshold.

Nesting is not learned: DocLayNet draws no nesting, so there is no label for it.
It stays the indent-step rule in the engine, measured on the lists axis.
"""
from __future__ import annotations

import collections
import math
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from align_labels import ink_of, lines_of  # noqa: E402

# The same bullets `lists.rs` reads (BULLET_CHARS), without the dashes, which
# need a space after them and are handled with it.
BULLETS = set("•‣⁃∙·▪▫■□●○◦◆◇❖✓✔")
DASHES = ("-", "–", "—")
ORDINAL = re.compile(
    r"^(?:\(?\d{1,3}[.)]|\(\d{1,3}\)|\[\d{1,3}\]|\(?[a-zA-Z][.)]|\([a-zA-Z]\)"
    r"|\(?[ivxlcIVXLC]{1,6}[.)]|\([ivxlcIVXLC]{1,6}\))$")
FIGURE_SIGNS = set("$€£¥¢%‰#+=")
RUNS_ON_ENDINGS = (",", ";", ":", "-", "–", "—")
RUNS_ON_WORDS = {"and", "or", "of", "the", "to"}

LINE_FEATURES: list[str] = [
    "marker_bullet", "marker_ordered", "marker_figure", "first_token_chars",
    "first_gap_em", "indent_em", "indent_vs_prev_em", "prev_short_em",
    "prev_ends_terminal", "prev_runs_on", "width_frac", "size_ratio", "bold",
    "line_index", "lines_in_block", "gap_above_em",
    "block_is_list", "block_size_ratio",
    "first_is_lone_symbol", "same_head_on_page", "block_is_table_cell",
    # The widest white between two words of the line, in ems: a row of table
    # cells is a line with gutters in it, and a bulleted cell looked like an
    # item without it (operator page `a_list_in_a_table_stays_text`).
    "max_gap_em",
    # Whether the block before this one on the page ends in a colon: a list
    # usually follows "as follows:", and without it the model read an item
    # after a finished sentence as prose.
    "prev_block_ends_colon",
]


def _box(line: list[dict]) -> list[float]:
    return [min(w["bbox"][0] for w in line), min(w["bbox"][1] for w in line),
            max(w["bbox"][2] for w in line), max(w["bbox"][3] for w in line)]


def _size(line: list[dict]) -> float:
    sizes = sorted(w.get("size") or 0.0 for w in line)
    return sizes[len(sizes) // 2] or 1.0


def marker_kind(token: str) -> str | None:
    """What the first token of a line is, as a list marker."""
    if not token:
        return None
    if len(token) == 1 and token in FIGURE_SIGNS or token[:1] in FIGURE_SIGNS:
        return "figure"
    if token in BULLETS or (len(token) <= 2 and token[:1] in BULLETS):
        return "bullet"
    if token in DASHES:
        return "bullet"
    if ORDINAL.match(token):
        return "ordered"
    return None


def runs_on(text: str) -> bool:
    text = text.rstrip()
    if text.endswith(RUNS_ON_ENDINGS):
        return True
    last = text.rsplit(None, 1)[-1] if text.split() else ""
    return last in RUNS_ON_WORDS


def heads_on_page(blocks: list[dict], words: list[dict]) -> dict:
    """How often each (first token, left edge in whole points) opens a line on the page.

    A bullet list says so by repetition: the same glyph at the same edge, line
    after line. That holds whatever the glyph decodes to -- a Wingdings bullet
    arrives as `l`, `n`, `_` or `z` -- which is exactly where a table of known
    bullet characters goes blind.
    """
    heads: collections.Counter = collections.Counter()
    for block in blocks:
        for line in lines_of(block, words):
            if line:
                heads[(line[0]["text"], round(line[0]["bbox"][0]))] += 1
    return heads


def line_vectors(block: dict, words: list[dict], body_size: float,
                 heads: dict | None = None, prev_ends_colon: bool | None = None) -> list[list[float]]:
    """One vector per line of the block, in the order `lines_of` returns them.

    `prev_ends_colon` is whether the page's previous block ends in a colon;
    None where there is no previous block."""
    lines = lines_of(block, words)
    if not lines:
        return []
    bx = block["bbox"]
    measure = max(bx[2] - bx[0], 1.0)
    body = body_size or 1.0
    out: list[list[float]] = []
    for at, line in enumerate(lines):
        box, size = _box(line), _size(line)
        first = line[0]["text"] if line else ""
        kind = marker_kind(first)
        gap = (line[1]["bbox"][0] - line[0]["bbox"][2]) / size if len(line) > 1 else math.nan
        prev = lines[at - 1] if at > 0 else None
        if prev is not None:
            pbox, ptext = _box(prev), " ".join(w["text"] for w in prev)
            indent_vs_prev = (box[0] - pbox[0]) / size
            prev_short = (bx[2] - pbox[2]) / size
            prev_terminal = 1.0 if ptext.rstrip().endswith((".", "!", "?")) else 0.0
            prev_runs = 1.0 if runs_on(ptext) else 0.0
            gap_above = (pbox[1] - box[3]) / size
        else:
            indent_vs_prev = prev_short = prev_terminal = prev_runs = gap_above = math.nan
        bold = sum(1 for w in line if w.get("bold")) / len(line)
        out.append([
            1.0 if kind == "bullet" else 0.0,
            1.0 if kind == "ordered" else 0.0,
            1.0 if kind == "figure" else 0.0,
            float(len(first)),
            gap,
            (box[0] - bx[0]) / size,
            indent_vs_prev,
            prev_short,
            prev_terminal,
            prev_runs,
            (box[2] - box[0]) / measure,
            size / body,
            bold,
            float(at),
            float(len(lines)),
            gap_above,
            1.0 if block.get("kind") == "listItem" or block.get("kind") == "list_item" else 0.0,
            (block.get("size") or size) / body,
            1.0 if len(first) == 1 and not first.isalnum() or (len(first) == 1 and gap >= 0.4) else 0.0,
            float(heads.get((first, round(line[0]["bbox"][0])), 0)) if heads is not None else math.nan,
            1.0 if block.get("kind") == "table_cell" else 0.0,
            max(((line[k + 1]["bbox"][0] - line[k]["bbox"][2]) / size for k in range(len(line) - 1)),
                default=math.nan),
            math.nan if prev_ends_colon is None else (1.0 if prev_ends_colon else 0.0),
        ])
    return out


def prev_colon(blocks: list[dict], at: int) -> bool | None:
    """Whether the block before `blocks[at]` on the page ends in a colon."""
    return blocks[at - 1]["text"].strip().endswith(":") if at > 0 else None


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("a bullet", marker_kind("•"), "bullet")
    check("a number with a stop", marker_kind("12."), "ordered")
    check("a letter in brackets", marker_kind("(c)"), "ordered")
    check("a roman numeral", marker_kind("(iv)"), "ordered")
    check("a reference in square brackets", marker_kind("[3]"), "ordered")
    check("a dollar sign is a figure, never a bullet", marker_kind("$"), "figure")
    check("an ordinary word", marker_kind("Apples"), None)
    check("a clause that runs on", runs_on("the roof of the original building,"), True)
    check("a finished sentence", runs_on("whichever is the greater."), False)

    def word(text, x0, y0, x1, y1, size=10.0):
        return {"text": text, "bbox": [x0, y0, x1, y1], "block": 0, "size": size, "bold": False}

    block = {"i": 0, "kind": "paragraph", "bbox": [72, 660, 540, 700], "size": 10.0}
    words = [word("1.", 72, 690, 82, 700), word("Certain", 90, 690, 540, 700),
             word("interexchange", 72, 676, 500, 686),
             word("2.", 72, 662, 82, 672), word("The", 90, 662, 300, 672)]
    vectors = line_vectors(block, words, 10.0)
    check("three lines", len(vectors), 3)
    col = LINE_FEATURES.index
    check("the first line opens with a number", vectors[0][col("marker_ordered")], 1.0)
    check("the continuation has no marker", vectors[1][col("marker_ordered")], 0.0)
    check("the third opens with a number", vectors[2][col("marker_ordered")], 1.0)
    check("the first line has no line above", math.isnan(vectors[0][col("prev_short_em")]), True)
    check("every vector is as long as the feature list",
          {len(v) for v in vectors}, {len(LINE_FEATURES)})
    check("the lines are T3's, the same ink grouped the same way",
          len(lines_of(block, words)), len(vectors))
    check("the words of a block are its own", len(ink_of(block, words)), 5)

    for failure in failures:
        print(f"  FAIL  {failure}")
    print(f"[list_lines] self-test: {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    print(__doc__)
    sys.exit(2)
