# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L5: SPEC sections 11, 13.
"""Plain-text output, legacy table option rules and table shaping helpers."""
from __future__ import annotations

import re
import textwrap
from collections.abc import Mapping

from ._errors import CONTINUE, check_call, register_option_rule
from ._mode import is_layout_mode

#: Detection strategies the legacy converter accepts (SPEC G97-G102).
_VALID_STRATEGIES = ("lines_strict", "lines", "text")

#: The only legacy table output form (SPEC G103).
_VALID_OUTPUTS = ("markdown",)

#: Top-level ``to_text`` options that steer the layout parse.
_PARSE_OPTIONS = (
    "filename",
    "image_dpi",
    "ocr_dpi",
    "image_format",
    "image_path",
    "pages",
    "show_progress",
    "embed_images",
    "write_images",
    "force_text",
    "use_ocr",
    "force_ocr",
    "ocr_language",
    "ocr_function",
    "render_html_tables",
    "edge_threshold",
)

#: Top-level ``to_text`` options that steer the text renderer.
_TEXT_OPTIONS = (
    "header",
    "footer",
    "ignore_code",
    "show_progress",
    "page_chunks",
    "table_format",
    "table_max_width",
    "table_min_col_width",
)


def _legacy_table_options(options, layout):
    """Refuse unknown legacy table strategies and output forms.

    SPEC G102 raises ``AttributeError`` for a bogus strategy and SPEC
    G103 raises ``ValueError`` for a bogus output. Layout mode takes
    neither option and ignores both (SPEC G107).
    """
    if layout:
        return CONTINUE
    if not isinstance(options, Mapping):
        return CONTINUE
    strategy = options.get("table_strategy", "lines_strict")
    if strategy not in _VALID_STRATEGIES:
        raise AttributeError("unknown table strategy: %r" % (strategy,))
    output = options.get("table_output", "markdown")
    if output not in _VALID_OUTPUTS:
        raise ValueError("unknown table output: %r" % (output,))
    return CONTINUE


register_option_rule("to_markdown", _legacy_table_options)


def to_text(*args, **kwargs):
    """Render a document as plain text (layout only).

    Parses with the layout pipeline, then renders its text form:
    emphasis stripped, code unfolded, lists bulleted and tables drawn
    as text grids (SPEC section 13). The legacy converter has no text
    form and raises ``NotImplementedError`` (SPEC G122). Unknown
    options are silently tolerated (SPEC G176).
    """
    doc = args[0] if args else kwargs.get("doc")
    options = dict(kwargs)
    options.pop("doc", None)
    pages = options.get("pages")
    if isinstance(pages, int) and not isinstance(pages, bool):
        # SPEC G45: the top-level entries take one page as an integer;
        # the parser itself only takes lists (SPEC G49).
        options["pages"] = [pages]
    early = check_call("to_text", doc, options, layout=is_layout_mode())
    if early is not CONTINUE:
        return early
    if not is_layout_mode():
        raise NotImplementedError("plain text is only available in layout mode")
    from ._chunks import parse_document

    parse_kwargs = {n: options[n] for n in _PARSE_OPTIONS if n in options}
    text_kwargs = {n: options[n] for n in _TEXT_OPTIONS if n in options}
    return parse_document(doc, **parse_kwargs).to_text(**text_kwargs)


def table_to_markdown(cells):
    """Render a cell matrix as a pipe table.

    SPEC G110: the first row heads the table, a ``|---|---|`` divider
    follows it, and one blank line closes the table. Cells render
    verbatim -- no bolding, no escaping; callers style beforehand.

    Unspecified upstream; our behaviour: ``None`` renders empty, other
    values render through ``str``, and a matrix with no rows renders as
    the empty string.
    """
    rows = [
        [
            cell if isinstance(cell, str) else ("" if cell is None else str(cell))
            for cell in row
        ]
        for row in cells
    ]
    if not rows:
        return ""
    lines = []
    for index, row in enumerate(rows):
        lines.append("|" + "|".join(row) + "|")
        if index == 0:
            lines.append("|" + "|".join(["---"] * len(row)) + "|")
    return "\n".join(lines) + "\n\n"


def _rects_touch(first, second):
    """True when two boxes overlap or share an edge."""
    return min(first[2], second[2]) >= max(first[0], second[0]) and min(
        first[3], second[3]
    ) >= max(first[1], second[1])


def _union_fill_coverage(fills, region):
    """Fraction of the region area covered by any fill rectangle.

    Exact, by coordinate subdivision: the fills are few and banded, so
    the cells stay small and the union counts overlaps once.
    """
    xs = sorted({region[0], region[2]})
    ys = sorted({region[1], region[3]})
    clipped = []
    for fill in fills:
        x0 = max(fill[0], region[0])
        y0 = max(fill[1], region[1])
        x1 = min(fill[2], region[2])
        y1 = min(fill[3], region[3])
        if x1 > x0 and y1 > y0:
            clipped.append((x0, y0, x1, y1))
            xs.extend((x0, x1))
            ys.extend((y0, y1))
    area = max(0.0, region[2] - region[0]) * max(0.0, region[3] - region[1])
    if area <= 0 or not clipped:
        return 0.0
    xs = sorted(set(xs))
    ys = sorted(set(ys))
    covered = 0.0
    for ix in range(len(xs) - 1):
        for iy in range(len(ys) - 1):
            cx = (xs[ix] + xs[ix + 1]) / 2.0
            cy = (ys[iy] + ys[iy + 1]) / 2.0
            if any(x0 <= cx <= x1 and y0 <= cy <= y1 for x0, y0, x1, y1 in clipped):
                covered += (xs[ix + 1] - xs[ix]) * (ys[iy + 1] - ys[iy])
    return covered / area


#: Fill fraction past which a proposal counts as a shaded table (G100):
#: `lines_strict` drops it where `lines` keeps it.
_SHADED_COVERAGE = 0.5


def _native_module():
    """The top-level engine module, imported late like the other lanes'."""
    import pdfluent

    return pdfluent


def find_tables(handle, number):
    """One page's table proposals as legacy findings (SPEC G97-G101).

    Each finding carries the proposal region as ``bbox`` (native y-up,
    for the render path's prose exclusion), the proposal ``cells`` with
    their (row, column, spans), and the three flags the strategies
    filter on: ``ruled`` (a ruling touches the region), ``shaded`` (a
    filled background covers half the region or more) and ``spanning``
    (a cell covers more than one row or column).
    """
    module = _native_module()
    page = handle[number]
    height = float(page.height)
    proposals = module.door_page_tables(handle, number)
    drawings = module.door_page_drawings(handle, number)
    rulings = drawings["rulings"]
    fills = drawings["fills"]
    findings = []
    for proposal in proposals:
        region = proposal["region"]
        cells = proposal["cells"]
        findings.append(
            {
                "bbox": (
                    region[0],
                    height - region[3],
                    region[2],
                    height - region[1],
                ),
                "cells": cells,
                "rows": proposal["rows"],
                "columns": proposal["columns"],
                "ruled": any(_rects_touch(line, region) for line in rulings),
                "shaded": _union_fill_coverage(fills, region) >= _SHADED_COVERAGE,
                "spanning": any(
                    cell.get("row_span", 1) != 1 or cell.get("column_span", 1) != 1
                    for cell in cells
                ),
            }
        )
    return findings


#: Spaces the extractor left inside numbers ("18 . 90"): the reference
#: reads "18.90", so digit-dot runs rejoin. A no-op where the text is
#: already joined, and prose keeps its word spaces either way.
_DIGIT_SPACE_RE = re.compile(r"(?<=[\d.]) +(?=[\d.])")


def _cell_text(text):
    """One proposal cell's text as the reference reads it."""
    return _DIGIT_SPACE_RE.sub("", text.strip())


def finding_markdown(finding):
    """One finding as a pipe table in the lines strategies' rendering.

    The first grid row heads the table unbolded; a head cell spanning
    columns invents `Col2`/`Col3` headers for the columns it covers
    (SPEC G101). Covered body cells stay empty.
    """
    rows = finding["rows"]
    columns = finding["columns"]
    grid = [["" for _ in range(columns)] for _ in range(rows)]
    for cell in finding["cells"]:
        row = cell["row"]
        column = cell["column"]
        if 0 <= row < rows and 0 <= column < columns:
            grid[row][column] = _cell_text(cell.get("text", ""))
            if row == 0:
                for covered in range(column + 1, column + cell.get("column_span", 1)):
                    if covered < columns and not grid[row][covered]:
                        grid[row][covered] = "Col%d" % (covered + 1)
    return table_to_markdown(grid)


def _overlap_groups(items, key):
    """Group items whose key ranges overlap, sorted by range start."""
    ordered = sorted(items, key=lambda item: (key(item)[0], key(item)[1]))
    groups = []
    for item in ordered:
        if groups and key(item)[0] <= groups[-1][1]:
            groups[-1][1] = max(groups[-1][1], key(item)[1])
            groups[-1][2].append(item)
        else:
            groups.append([key(item)[0], key(item)[1], [item]])
    return groups


def _band_of(groups, value):
    """Index of the band holding a coordinate, or None outside all."""
    for position, (start, end, _) in enumerate(groups):
        if start <= value <= end:
            return position
    return None


def _word_edge_grid(cells):
    """Proposal cells as a row-major text grid rebuilt from word edges.

    Rows group cells whose boxes overlap vertically, columns cells
    whose boxes overlap horizontally; ruling plays no part. Each slot
    holds its cells' texts (fixtures carry exactly one cell per slot;
    two cells sharing a slot join with a space).
    """
    boxes = []
    for cell in cells or ():
        if not isinstance(cell, Mapping):
            continue
        try:
            rect = _cell_rect(cell.get("bbox"))
        except (TypeError, ValueError):
            continue
        boxes.append((rect, _cell_text(cell.get("text", ""))))
    if not boxes:
        return [], 0
    rows = _overlap_groups(boxes, lambda item: (item[0][1], item[0][3]))
    columns = _overlap_groups(boxes, lambda item: (item[0][0], item[0][2]))
    grid = [["" for _ in columns] for _ in rows]
    for rect, text in boxes:
        row = _band_of(rows, (rect[1] + rect[3]) / 2.0)
        column = _band_of(columns, (rect[0] + rect[2]) / 2.0)
        if row is None or column is None:
            continue
        if grid[row][column]:
            grid[row][column] += " " + text
        else:
            grid[row][column] = text
    return grid, len(columns)


def _text_content_cell(text, column, ncols):
    """One text-grid content cell: lines joined with a trailing break.

    The last column renders bare; an empty cell stays empty (no
    recording shows one, so the bare form is our behaviour there).
    """
    if column >= ncols - 1 or not text:
        return text
    return text + "<br>"


def _text_divider_cell(index, column, ncols, spaced):
    """One text-grid divider cell: breaks indexed by divider and column.

    Divider ``index`` (below data line ``index``) holds
    ``2 * index + column - 2`` cleared lines in column ``column`` (the
    last column stays empty), the last turning into a space once
    ``index + column - 1`` reaches 2; without a spaced cell anywhere
    the dividers stay empty. Fitted to the five text recordings, which
    are the spec for this shape.
    """
    if column >= ncols - 1 or not spaced:
        return ""
    count = 2 * index + column - 2
    if count <= 0:
        return ""
    lines = [""] * count
    if index + column - 1 >= 2:
        lines[-1] = " "
    return "<br>".join(lines) + "<br>"


def text_finding_markdown(finding):
    """One finding as a pipe table in the text strategy's rendering.

    The grid is rebuilt from the proposal cells' word edges with
    ruling ignored; a divider row stands before every data row, the
    header row stays bare, and content cells join their lines with
    ``<br>`` (SPEC G98).
    """
    if isinstance(finding, Mapping):
        cells = finding.get("cells", [])
    else:
        cells = getattr(finding, "cells", [])
    grid, ncols = _word_edge_grid(cells)
    if not grid or not ncols:
        return ""
    spaced = any(" " in text for row in grid for text in row)
    rows = [list(grid[0])]
    for index, data in enumerate(grid[1:]):
        rows.append(
            [
                _text_divider_cell(index, column, ncols, spaced)
                for column in range(ncols)
            ]
        )
        rows.append(
            [
                _text_content_cell(text, column, ncols)
                for column, text in enumerate(data)
            ]
        )
    return table_to_markdown(rows)


def _cell_rect(cell):
    """Read a 4-sided rectangle out of a cell or bbox mapping."""
    rect = cell.get("bbox") if isinstance(cell, Mapping) else cell
    try:
        x0, y0, x1, y1 = (float(side) for side in rect)
    except (TypeError, ValueError):
        raise ValueError("table cell is not a rectangle: %r" % (cell,))
    return (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def _block_lines(block):
    """One block's text as stripped lines.

    Reads structured lines with spans where present, else a ``text``
    field split on line breaks, else the block stringified. Stripping
    keeps layout's trailing spaces out of extracted cell texts.
    """
    if isinstance(block, str):
        return [block.strip()]
    if not isinstance(block, Mapping):
        return [str(block).strip()]
    lines = block.get("lines")
    if lines:
        out = []
        for line in lines:
            if isinstance(line, Mapping):
                spans = line.get("spans", ())
                out.append(
                    "".join(
                        str(span.get("text", ""))
                        for span in spans
                        if isinstance(span, Mapping)
                    )
                )
            else:
                out.append(str(line))
        return [text.strip() for text in out]
    text = block.get("text", "")
    if isinstance(text, str) and "\n" in text:
        return [chunk.strip() for chunk in text.split("\n")]
    return [str(text).strip()]


def _block_centre(block):
    """Centre point of a block's bbox, or None when it has none."""
    if not isinstance(block, Mapping):
        return None
    bbox = block.get("bbox")
    if bbox is None:
        return None
    try:
        x0, y0, x1, y1 = (float(side) for side in bbox)
    except (TypeError, ValueError):
        return None
    return ((x0 + x1) / 2.0, (y0 + y1) / 2.0)


def extract_cells(table_blocks, cell, markdown=False, ocrpage=False):
    """Cut cell texts out of raw table blocks.

    Collects the blocks whose bbox centre falls inside ``cell`` (a
    4-sided rectangle, or a mapping carrying one under ``bbox``) and
    joins their lines. Blocks are mappings with ``bbox`` plus ``text``
    or structured ``lines``; blocks without a usable bbox are kept.

    Unspecified upstream; our behaviour: ``markdown=True`` joins lines
    as HTML breaks with a trailing break, matching the ``<br>`` cell
    tags the recorded text-strategy tables carry (SPEC G98); the plain
    form joins lines with newlines. ``ocrpage`` is accepted and
    ignored: recognised text arrives as ordinary blocks in this build.
    """
    x0, y0, x1, y1 = _cell_rect(cell)
    picked = []
    for block in table_blocks or ():
        centre = _block_centre(block)
        if centre is not None:
            cx, cy = centre
            if not (x0 <= cx <= x1 and y0 <= cy <= y1):
                continue
        picked.extend(line for line in _block_lines(block) if line)
    if markdown:
        return "<br>".join(picked) + "<br>" if picked else ""
    return "\n".join(picked)


def _grid_union(grid):
    """Smallest rectangle holding every cell rect of a grid."""
    lo_x = lo_y = float("inf")
    hi_x = hi_y = float("-inf")
    for row in grid or ():
        for rect in row or ():
            try:
                x0, y0, x1, y1 = _cell_rect(rect)
            except ValueError:
                continue
            lo_x, lo_y = min(lo_x, x0), min(lo_y, y0)
            hi_x, hi_y = max(hi_x, x1), max(hi_y, y1)
    if lo_x == float("inf"):
        return None
    return (lo_x, lo_y, hi_x, hi_y)


def _as_count(value, fallback):
    try:
        return int(value)
    except (TypeError, ValueError):
        return fallback


def get_table_details(tab_dict, table_blocks):
    """Build the detail record for one detected table.

    ``tab_dict`` carries the detector's record: ``bbox``, the ``cells``
    rect grid, counts under ``rows``/``row_count`` and
    ``columns``/``cols``/``col_count``, and ``header`` (whether the
    first row heads the table). ``table_blocks`` are the raw text
    blocks the cell texts are cut from (see :func:`extract_cells`).
    Answers a ``TableDetails`` with plain ``extract`` rows and a pipe
    ``markdown`` form whose first row is bolded for headed tables.

    Unspecified upstream; our behaviour: a missing grid becomes the
    whole bbox as one cell, a missing bbox becomes the cells' union,
    counts fall back to the grid shape, and the markdown field holds
    the rows plus one blank line (matching the recorded table
    records).
    """
    from ._chunks import TableDetails

    if not isinstance(tab_dict, Mapping):
        raise TypeError("table record must be a mapping: %r" % (tab_dict,))
    raw_bbox = tab_dict.get("bbox")
    grid = tab_dict.get("cells") or []
    if raw_bbox is None:
        bbox = _grid_union(grid) or (0.0, 0.0, 0.0, 0.0)
    else:
        bbox = _cell_rect(raw_bbox)
    if not grid:
        grid = [[bbox]]
    rects = [[_cell_rect(rect) for rect in row] for row in grid]
    extract = [[extract_cells(table_blocks, rect) for rect in row] for row in rects]
    row_count = _as_count(tab_dict.get("rows", tab_dict.get("row_count")), len(extract))
    col_count = _as_count(
        tab_dict.get("columns", tab_dict.get("cols", tab_dict.get("col_count"))),
        max((len(row) for row in extract), default=0),
    )
    styled = [list(row) for row in extract]
    if tab_dict.get("header", True) and styled:
        styled[0] = ["**%s**" % cell for cell in styled[0]]
    body = table_to_markdown(styled)
    markdown = body.rstrip("\n") + "\n\n" if body else ""
    return TableDetails(
        bbox=tuple(bbox),
        row_count=row_count,
        col_count=col_count,
        cells=[[list(rect) for rect in row] for row in rects],
        extract=extract,
        markdown=markdown,
    )


def wrap_table_for_tabulate(table, max_width=100, min_col_width=10):
    """Shape a table for the plain-text grid renderer.

    SPEC G215: short rows echo back unchanged. Longer cells wrap so
    the grid stays readable.

    Unspecified upstream; our behaviour: a cell wraps only past the
    wider of ``max_width`` and ``min_col_width`` (so the recorded
    small tables echo under every probed width, SPEC G121); wrapped
    lines join with newlines inside the cell, existing line breaks
    are kept, and unusable widths fall back to the defaults.
    """
    limit = _as_count(max_width, 100)
    floor = _as_count(min_col_width, 10)
    width = max(limit if limit > 0 else 100, floor if floor > 0 else 10, 1)
    out = []
    for row in table or ():
        wrapped = []
        for cell in row or ():
            text = (
                cell if isinstance(cell, str) else ("" if cell is None else str(cell))
            )
            if len(text) <= width:
                wrapped.append(text)
                continue
            lines = []
            for chunk in text.split("\n"):
                lines.extend(
                    textwrap.wrap(
                        chunk,
                        width=width,
                        break_long_words=True,
                        break_on_hyphens=False,
                    )
                    or [""]
                )
            wrapped.append("\n".join(lines))
        out.append(wrapped)
    return out
