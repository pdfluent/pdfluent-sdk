#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""PubTables-1M's cell annotations as the HTML a TEDS axis reads (#575).

DocLayNet says where a table is. This set says what is IN it: every cell with
its row numbers, its column numbers, whether it is a column header, and the
text the PDF holds there. That is a grid, and a grid is what
`structured_axes.teds` compares.

This file is the translation and nothing else -- no network, no files -- so
that the shape of the ground truth is arithmetic the self-test can reach.
`scripts/ci/loop/structured_bench.py` is the half that fetches.

WHAT A ROW OF THE SET LOOKS LIKE
================================
One JSON per article, named `PMC1234567_tables.json`, holding a list of tables.
Each table carries `pdf_page_index`, `pdf_table_bbox` and `cells`; each cell
carries `row_nums`, `column_nums`, `is_column_header`, `pdf_text_content` and
the XML text it was derived from. The fields are Microsoft's
`scripts/process_pubmed.py`, which is how the set was built.

WHY THE PDF TEXT AND NOT THE XML TEXT
=====================================
Both are in the annotation. The XML text is what the publisher's markup says
the cell holds; `pdf_text_content` is what the words on the PAGE say, aligned
to that cell. A tool reads the page, so the page is what it is graded against
-- and where the two differ it is usually a superscript or a footnote marker
that is in the markup and not in the rendering, which would charge every tool
for a character no reader can see. The XML text is the fallback for a cell
whose alignment produced nothing, and the count of those is reported.
"""
from __future__ import annotations

import html as html_module
import re
import sys

PMCID = re.compile(r"(PMC\d+)")


def pmcid_of(name: str) -> str | None:
    """The article an annotation file is about, from its name.

    `PMC1234567_tables.json`. A file that does not name one is skipped rather
    than guessed at: the name is the only key the documents can be fetched by,
    and fetching the wrong article would grade a tool on somebody else's page.
    """
    found = PMCID.search(name or "")
    return found.group(1) if found else None


def _span(numbers) -> int:
    """How many rows or columns a cell covers. At least one."""
    try:
        values = sorted({int(n) for n in numbers})
    except (TypeError, ValueError):
        return 1
    if not values:
        return 1
    # The set writes the numbers a cell occupies, so the span is how many there
    # are -- not max - min, which would count a hole in a malformed annotation
    # as coverage.
    return len(values)


def _first(numbers) -> int:
    try:
        values = [int(n) for n in numbers]
    except (TypeError, ValueError):
        return 0
    return min(values) if values else 0


def n_columns(cells, key: str = "column_nums") -> int:
    """How many columns a cell list covers: the highest number, plus one.

    The numbers are zero-based -- columns [0, 1] are two columns -- so the
    count is the maximum plus one, over every cell with a readable number.
    A cell without one covers nothing countable and is skipped, the way
    `html_of` drops a cell with no numbers at all. The shard rows read
    through `key="cols"`: the dump copies the numbers unchanged.
    """
    covered: set[int] = set()
    for cell in cells or []:
        try:
            covered.update(int(n) for n in cell.get(key) or [])
        except (TypeError, ValueError, AttributeError):
            continue
    return max(covered) + 1 if covered else 0


def _text_of(cell: dict) -> tuple[str, bool]:
    """The cell's text, and whether it had to fall back to the markup."""
    pdf_text = (cell.get("pdf_text_content") or "").strip()
    if pdf_text:
        return pdf_text, False
    return (cell.get("xml_text_content") or "").strip(), True


def html_of(table: dict) -> tuple[str, dict]:
    """One annotated table as `<table><tr><td>`, and what that cost.

    The cells are laid out by the row they START in and ordered by the column
    they start in, which is how a reading of the grid recovers the document
    order of a table with spans. A cell with no row or column numbers at all is
    dropped and counted: it is a cell the annotation cannot place, and putting
    it at (0, 0) would move every other cell one along.
    """
    rows: dict[int, list[tuple[int, str]]] = {}
    fell_back = placed = dropped = 0
    for cell in table.get("cells") or []:
        if not cell.get("row_nums") or not cell.get("column_nums"):
            dropped += 1
            continue
        text, fallback = _text_of(cell)
        fell_back += int(fallback)
        tag = "th" if cell.get("is_column_header") else "td"
        attrs = ""
        colspan, rowspan = _span(cell["column_nums"]), _span(cell["row_nums"])
        if colspan > 1:
            attrs += f' colspan="{colspan}"'
        if rowspan > 1:
            attrs += f' rowspan="{rowspan}"'
        markup = f"<{tag}{attrs}>{html_module.escape(text)}</{tag}>"
        rows.setdefault(_first(cell["row_nums"]), []).append(
            (_first(cell["column_nums"]), markup))
        placed += 1
    if not placed:
        return "", {"cells": 0, "dropped": dropped, "text_from_markup": fell_back}
    out = ["<table>"]
    for index in sorted(rows):
        out.append("<tr>" + "".join(
            markup for _column, markup in sorted(rows[index],
                                                 key=lambda c: c[0])) + "</tr>")
    out.append("</table>")
    return "".join(out), {"cells": placed, "dropped": dropped,
                          "text_from_markup": fell_back}


def tables_by_page(annotation: list[dict], floor: int = 4) -> dict[int, list[dict]]:
    """The article's tables, grouped by the page they are on.

    `floor` is the smallest table that is worth grading: a "table" of three
    cells is a caption or a fragment of one, and a page full of them would fill
    the axis with pages where every tool scores 1.0 or 0.0 for reasons that are
    not table structure. The same shape of rule as the floors in
    `structured_axes.py`, and it is applied to the GROUND TRUTH rather than to
    anything a tool emitted, so it cannot favour a column.
    """
    pages: dict[int, list[dict]] = {}
    for table in annotation or []:
        index = table.get("pdf_page_index")
        if index is None:
            continue
        markup, counted = html_of(table)
        if counted["cells"] < floor:
            continue
        pages.setdefault(int(index), []).append({
            "html": markup,
            "cells": counted["cells"],
            "dropped_cells": counted["dropped"],
            "text_from_markup": counted["text_from_markup"],
            "bbox": table.get("pdf_table_bbox") or [],
            "rows": table.get("num_rows"),
            "columns": table.get("num_columns"),
        })
    return pages


def text_of(table_html: str) -> str:
    """The words a table holds, for the text a block model is matched on."""
    return " ".join(html_module.unescape(
        re.sub(r"<[^>]+>", " ", table_html or "")).split())


# ---------------------------------------------------------------------------
def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    check("the article is read off the file name",
          pmcid_of("PMC1234567_tables.json"), "PMC1234567",
          "the name is the only key the documents can be fetched by")
    check("and a file that names none is skipped",
          pmcid_of("tables.json"), None,
          "fetching the wrong article grades a tool on somebody else's page")

    plain = {"pdf_page_index": 2, "cells": [
        {"row_nums": [0], "column_nums": [0], "is_column_header": True,
         "pdf_text_content": "Year"},
        {"row_nums": [0], "column_nums": [1], "is_column_header": True,
         "pdf_text_content": "Total"},
        {"row_nums": [1], "column_nums": [0], "pdf_text_content": "2024"},
        {"row_nums": [1], "column_nums": [1], "pdf_text_content": "12"},
    ]}
    markup, counted = html_of(plain)
    check("a plain grid comes out as one",
          markup,
          "<table><tr><th>Year</th><th>Total</th></tr>"
          "<tr><td>2024</td><td>12</td></tr></table>",
          "a header cell is a th, and TEDS grades the tag")
    check("and every cell is placed", counted["cells"], 4, "")

    spanned = {"pdf_page_index": 0, "cells": [
        {"row_nums": [0], "column_nums": [0, 1], "is_column_header": True,
         "pdf_text_content": "Results"},
        {"row_nums": [1], "column_nums": [0], "pdf_text_content": "a"},
        {"row_nums": [1, 2], "column_nums": [1], "pdf_text_content": "b"},
        {"row_nums": [2], "column_nums": [0], "pdf_text_content": "c"},
    ]}
    markup, _ = html_of(spanned)
    check("a spanning cell carries its span",
          '<th colspan="2">Results</th>' in markup
          and '<td rowspan="2">b</td>' in markup, True,
          "a table whose cells hold the right text under the wrong spans is a "
          "table a pipeline reads wrongly, and TEDS exists to say so")
    check("and a spanning cell appears once",
          markup.count("<td rowspan=\"2\">b</td>"), 1,
          "writing it into both rows would make the grid say something the "
          "annotation does not")

    out_of_order = {"pdf_page_index": 0, "cells": [
        {"row_nums": [1], "column_nums": [1], "pdf_text_content": "d"},
        {"row_nums": [0], "column_nums": [1], "pdf_text_content": "b"},
        {"row_nums": [1], "column_nums": [0], "pdf_text_content": "c"},
        {"row_nums": [0], "column_nums": [0], "pdf_text_content": "a"},
    ]}
    markup, _ = html_of(out_of_order)
    check("the cells are laid out by their numbers, not by their order in the file",
          markup, "<table><tr><td>a</td><td>b</td></tr>"
                  "<tr><td>c</td><td>d</td></tr></table>",
          "the annotation is a list and the grid is a grid")

    unplaceable = {"pdf_page_index": 0, "cells": [
        {"row_nums": [], "column_nums": [], "pdf_text_content": "?"},
        {"row_nums": [0], "column_nums": [0], "pdf_text_content": "a"},
    ]}
    _markup, counted = html_of(unplaceable)
    check("a cell the annotation cannot place is dropped and counted",
          (counted["cells"], counted["dropped"]), (1, 1),
          "putting it at (0, 0) would move every other cell one along")

    from_markup = {"pdf_page_index": 0, "cells": [
        {"row_nums": [0], "column_nums": [0], "pdf_text_content": "",
         "xml_text_content": "n = 12"},
    ]}
    markup, counted = html_of(from_markup)
    check("a cell with no aligned text falls back to the markup",
          ("n = 12" in markup, counted["text_from_markup"]), (True, 1),
          "and the count is reported, because the two texts are two readings")

    escaped = html_of({"pdf_page_index": 0, "cells": [
        {"row_nums": [0], "column_nums": [0], "pdf_text_content": "a < b & c"}]})[0]
    check("cell text is escaped", "a &lt; b &amp; c" in escaped,
          True, "a cell holding a tag would otherwise become one")
    check("and reads back", text_of(escaped), "a < b & c",
          "the block model is matched on the words, not on the markup")

    grouped = tables_by_page([plain, spanned])
    check("the tables are grouped by the page they are on",
          sorted(grouped), [0, 2],
          "a page is the unit every other axis is scored on")
    check("a table smaller than the floor is not ground truth",
          tables_by_page([{"pdf_page_index": 0, "cells": [
              {"row_nums": [0], "column_nums": [0], "pdf_text_content": "x"}]}]),
          {},
          "a table of one cell is a caption, and a page full of them fills the "
          "axis with free ones and free zeroes")

    if failures:
        print(f"[pubtables-cells] {len(failures)} check(s) failed\n",
              file=sys.stderr)
        for line in failures:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("✓ pubtables cells: a grid with spans, headers and unplaceable cells "
          "comes out as the HTML a TEDS axis reads")
    return 0


if __name__ == "__main__":
    sys.exit(self_test() if "--self-test" in sys.argv else self_test())
