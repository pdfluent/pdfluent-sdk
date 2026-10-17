# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L2: SPEC sections 4, 5, 19.
"""Column detection, rectangle math and reading-order helpers."""
from __future__ import annotations

import math
import struct

from ._errors import CONTINUE, register_option_rule

__all__ = [
    "Rect",
    "almost_in_bbox",
    "are_disjoint",
    "bbox_in_bbox",
    "bbox_is_empty",
    "cluster_columns_in_stripe",
    "cluster_stripes",
    "column_boxes",
    "compute_reading_order",
    "expand_bbox_by_points",
    "find_reading_order",
    "find_virtual_lines",
    "intersect_rects",
    "iou",
    "is_significant",
    "join_rect_pair",
    "join_rects",
    "refine_boxes",
    "resolve_margins",
    "resolve_pages",
    "simplify_vectors",
]


class Rect:
    """A four-edge rectangle, readable by index and by attribute.

    Helpers in this module take any four-number sequence (or an object
    with ``x0``/``y0``/``x1``/``y1`` attributes) and hand back this type,
    so callers can use ``box[0]`` and ``box.x0`` interchangeably.
    """

    __slots__ = ("x0", "y0", "x1", "y1")

    def __init__(self, x0, y0, x1, y1):
        self.x0 = x0
        self.y0 = y0
        self.x1 = x1
        self.y1 = y1

    def __len__(self):
        return 4

    def __getitem__(self, index):
        return (self.x0, self.y0, self.x1, self.y1)[index]

    def __iter__(self):
        yield self.x0
        yield self.y0
        yield self.x1
        yield self.y1

    def __repr__(self):
        return "Rect(%r, %r, %r, %r)" % (self.x0, self.y0, self.x1, self.y1)

    def __eq__(self, other):
        try:
            x0, y0, x1, y1 = other
        except TypeError:
            return NotImplemented
        except ValueError:
            return False
        return (self.x0, self.y0, self.x1, self.y1) == (x0, y0, x1, y1)


def _coords(box):
    """Unpack any rectangle into a plain ``(x0, y0, x1, y1)`` tuple."""
    if box is None:
        raise TypeError("expected a four-edge rectangle, got None")
    try:
        x0, y0, x1, y1 = box
    except TypeError:
        try:
            return (box.x0, box.y0, box.x1, box.y1)
        except AttributeError:
            raise TypeError(
                "expected a four-edge rectangle, got %r" % type(box).__name__
            )
    return (x0, y0, x1, y1)


def _area(box):
    x0, y0, x1, y1 = _coords(box)
    return max(0.0, x1 - x0) * max(0.0, y1 - y0)


def iou(r1, r2):
    """Return the overlap-over-union fraction of two boxes."""
    ax0, ay0, ax1, ay1 = _coords(r1)
    bx0, by0, bx1, by1 = _coords(r2)
    inter = max(0.0, min(ax1, bx1) - max(ax0, bx0)) * max(
        0.0, min(ay1, by1) - max(ay0, by0)
    )
    union = _area(r1) + _area(r2) - inter
    if union <= 0.0:
        return 0.0
    return inter / union


def bbox_in_bbox(inner, outer) -> bool:
    """Return True when the inner box sits inside the outer box."""
    ix0, iy0, ix1, iy1 = _coords(inner)
    ox0, oy0, ox1, oy1 = _coords(outer)
    return ox0 <= ix0 and iy0 >= oy0 and ix1 <= ox1 and iy1 <= oy1


def are_disjoint(bbox, cell, strict=False) -> bool:
    """Return True when the two boxes do not overlap.

    Boxes that only share an edge count as disjoint; with ``strict``
    they must stand fully apart (unspecified upstream; our behaviour).
    """
    ax0, ay0, ax1, ay1 = _coords(bbox)
    bx0, by0, bx1, by1 = _coords(cell)
    if strict:
        return ax1 < bx0 or bx1 < ax0 or ay1 < by0 or by1 < ay0
    return ax1 <= bx0 or bx1 <= ax0 or ay1 <= by0 or by1 <= ay0


def bbox_is_empty(bbox) -> bool:
    """Return True when the box covers no area."""
    x0, y0, x1, y1 = _coords(bbox)
    return not (x1 > x0 and y1 > y0)


def almost_in_bbox(bbox, clip, portion=0.6):
    """Return True when enough of the box falls inside the clip.

    "Enough" is the fraction of the *box* covered by the overlap
    reaching ``portion``; an empty box never qualifies (unspecified
    upstream; our behaviour).
    """
    inner = _area(bbox)
    if inner <= 0.0:
        return False
    ax0, ay0, ax1, ay1 = _coords(bbox)
    bx0, by0, bx1, by1 = _coords(clip)
    inter = max(0.0, min(ax1, bx1) - max(ax0, bx0)) * max(
        0.0, min(ay1, by1) - max(ay0, by0)
    )
    return inter / inner >= portion


def join_rects(rects, bbox_only=False):
    """Return the bounding rectangle of a rectangle list.

    With ``bbox_only`` the answer is a plain ``[x0, y0, x1, y1]``
    list, otherwise a :class:`Rect` (unspecified upstream; our
    behaviour).
    """
    items = [_coords(box) for box in rects]
    if not items:
        raise ValueError("join_rects needs at least one rectangle")
    x0 = min(item[0] for item in items)
    y0 = min(item[1] for item in items)
    x1 = max(item[2] for item in items)
    y1 = max(item[3] for item in items)
    if bbox_only:
        return [x0, y0, x1, y1]
    return Rect(x0, y0, x1, y1)


def join_rect_pair(r1, r2, bbox_only=False):
    """Return the bounding rectangle of two rectangles (re-exported as join_rects)."""
    return join_rects([r1, r2], bbox_only=bbox_only)


def intersect_rects(r1, r2, bbox_only=False):
    """Intersect two rectangles.

    Boxes that miss each other give an inverted (empty) rectangle
    rather than an error (unspecified upstream; our behaviour).
    """
    ax0, ay0, ax1, ay1 = _coords(r1)
    bx0, by0, bx1, by1 = _coords(r2)
    x0, y0, x1, y1 = max(ax0, bx0), max(ay0, by0), min(ax1, bx1), min(ay1, by1)
    if bbox_only:
        return [x0, y0, x1, y1]
    return Rect(x0, y0, x1, y1)


def _is_point(value):
    try:
        x, y = value
    except (TypeError, ValueError):
        return False
    return isinstance(x, (int, float)) and isinstance(y, (int, float))


def expand_bbox_by_points(bbox, points):
    """Grow a box so it covers the given points.

    ``points`` is an iterable of ``[x, y]`` pairs; a single pair is
    accepted too. The answer is a plain list.
    """
    x0, y0, x1, y1 = _coords(bbox)
    if _is_point(points):
        pairs = [points]
    else:
        pairs = list(points)
    for point in pairs:
        px, py = point
        x0 = min(x0, px)
        y0 = min(y0, py)
        x1 = max(x1, px)
        y1 = max(y1, py)
    return [x0, y0, x1, y1]


def refine_boxes(boxes, enlarge=0):
    """Merge and tidy a list of detected boxes.

    Empty boxes are dropped, boxes that overlap are fused into their
    union (transitively), and every survivor is then padded by
    ``enlarge`` points on each side (unspecified upstream; our
    behaviour).
    """
    kept = [box for box in boxes if not bbox_is_empty(box)]
    merged = []
    for box in kept:
        x0, y0, x1, y1 = _coords(box)
        placed = False
        for index, other in enumerate(merged):
            ox0, oy0, ox1, oy1 = _coords(other)
            if x0 <= ox1 and ox0 <= x1 and y0 <= oy1 and oy0 <= y1:
                merged[index] = Rect(
                    min(x0, ox0), min(y0, oy0), max(x1, ox1), max(y1, oy1)
                )
                placed = True
                break
        if not placed:
            merged.append(Rect(x0, y0, x1, y1))
    if enlarge:
        merged = [
            Rect(b.x0 - enlarge, b.y0 - enlarge, b.x1 + enlarge, b.y1 + enlarge)
            for b in merged
        ]
    return merged


def is_significant(box, paths):
    """Return True when vector paths mark the box as meaningful.

    True when any path rectangle overlaps the box interior with
    positive area (unspecified upstream; our behaviour).
    """
    x0, y0, x1, y1 = _coords(box)
    for path in paths or []:
        try:
            px0, py0, px1, py1 = _coords(path)
        except TypeError:
            continue
        if min(x1, px1) > max(x0, px0) and min(y1, py1) > max(y0, py0):
            return True
    return False


def _f32(value):
    """Round a float through single precision, like the text engine."""
    return struct.unpack("f", struct.pack("f", value))[0]


# Line-box rise above the baseline and drop below it, as a fraction of
# the font size, measured from the recorded column and span boxes: the
# 12pt body line sits 8.784pt above and 2.616pt below its baseline, the
# 11pt column lines 8.052pt above. Lines without a descender end
# 0.023 of an em below the baseline (one recorded sample). Content
# dependent ink tops seen on two span boxes are approximated by the
# fixed rise (unspecified upstream; our behaviour).
_LINE_RISE = 0.732
_LINE_DROP_DESCENDER = 0.218
_LINE_DROP_PLAIN = 0.023

_DESCENDERS = frozenset("gjpqyJ")


def _line_edges(baseline, size, page_height, has_descender):
    """Top-left ``(y0, y1)`` of one text line at the PDF baseline."""
    # Both operands quantize first: the recorded edges were computed in
    # float32 throughout, so a full-precision operand on either side
    # drifts ~3e-5 off them (SPEC G34, helper-pinned).
    top = _f32(_f32(page_height) - _f32(baseline))
    rise = _f32(_f32(_LINE_RISE) * _f32(size))
    factor = _LINE_DROP_DESCENDER if has_descender else _LINE_DROP_PLAIN
    drop = _f32(_f32(factor) * _f32(size))
    return _f32(top - rise), _f32(top + drop)


def _accumulate(x0, advances):
    """Run the pen over per-glyph advances with float rounding per step."""
    pen = _f32(x0)
    for advance in advances:
        pen = _f32(float(pen) + advance)
    return pen


def _native_spans(page):
    """Read one page's spans with per-glyph boxes through the native door."""
    try:
        from pdfluent._native import door_page_spans as helper
    except ImportError:
        raise ImportError(
            "column_boxes needs the native pdfluent module for page input"
        )
    return helper(page)


def _zone_list(avoid):
    """Normalise the ``avoid`` option to a list of coordinate tuples."""
    if avoid is None:
        return []
    if isinstance(avoid, Rect):
        return [_coords(avoid)]
    if (
        isinstance(avoid, (list, tuple))
        and len(avoid) == 4
        and all(isinstance(value, (int, float)) for value in avoid)
    ):
        return [_coords(avoid)]
    try:
        return [_coords(zone) for zone in avoid]
    except TypeError:
        return [_coords(avoid)]


def _upright_lines(spans):
    """Group span glyph runs into lines: one baseline, x-sorted, gap-split."""
    by_baseline = {}
    for span in spans:
        text, x, y, size, glyphs, upright = span
        if not upright or not glyphs:
            continue
        by_baseline.setdefault(y, []).append((x, size, text, glyphs))
    lines = []
    for baseline in sorted(by_baseline):
        runs = sorted(by_baseline[baseline], key=lambda run: run[0])
        current = []
        end = 0.0
        for x, size, text, glyphs in runs:
            if current and x - end > 0.5 * size:
                lines.append((baseline, current))
                current = []
                end = 0.0
            current.append((x, size, text, glyphs))
            end = max(end, max(glyph[2] for glyph in glyphs))
        if current:
            lines.append((baseline, current))
    return lines


def column_boxes(
    page,
    *,
    footer_margin=50,
    header_margin=50,
    no_image_text=True,
    textpage=None,
    paths=None,
    avoid=None,
    ignore_images=False,
):
    """Report the detected column rectangles of one page.

    Spans group into lines by baseline, lines at the same height
    split into segments across wide gaps, and segments chain into
    columns by horizontal overlap. A line whose baseline falls in
    the head or foot margin band is left out, as is a line whose
    horizontal centre and baseline fall in an ``avoid`` zone.
    ``textpage``, ``paths``, ``no_image_text`` and
    ``ignore_images`` are accepted and ignored: detection reads the
    native page's own text runs (unspecified upstream; our
    behaviour).
    """
    if page is None:
        raise TypeError("column_boxes needs a native page object")
    height = float(page.height)
    zones = _zone_list(avoid)
    boxes = []
    for baseline, runs in _upright_lines(_native_spans(page)):
        top = _f32(height - _f32(baseline))
        if top <= header_margin or top >= height - footer_margin:
            continue
        size = max(run[1] for run in runs)
        x0 = _f32(min(run[0] for run in runs))
        # The pen, not the ink: each step runs to the next glyph's
        # origin, so kerned and overlapping neighbours are not
        # double-counted the way summed ink widths are; only the last
        # glyph contributes its ink (SPEC G33/G34, pinned to the
        # recorded edges bit for bit).
        glyphs = [glyph for _, _, _, run in runs for glyph in run]
        steps = [
            glyphs[i + 1][0] - glyphs[i][0] for i in range(len(glyphs) - 1)
        ]
        steps.append(glyphs[-1][2] - glyphs[-1][0])
        x1 = _accumulate(min(run[0] for run in runs), steps)
        downward = any(
            any(char in _DESCENDERS for char in text) for _, _, text, _ in runs
        )
        y0, y1 = _line_edges(baseline, size, height, downward)
        if zones and any(
            zx0 <= (x0 + x1) / 2 <= zx1 and zy0 <= top <= zy1
            for zx0, zy0, zx1, zy1 in zones
        ):
            continue
        boxes.append(Rect(x0, y0, x1, y1))
    boxes.sort(key=lambda box: box.x0)
    columns = []
    for box in boxes:
        if columns and box.x0 <= columns[-1].x1:
            first = columns[-1]
            columns[-1] = Rect(
                min(first.x0, box.x0),
                min(first.y0, box.y0),
                max(first.x1, box.x1),
                max(first.y1, box.y1),
            )
        else:
            columns.append(box)
    return columns


def cluster_stripes(boxes, joined_boxes, vectors, vertical_gap=12):
    """Group boxes into horizontal stripes.

    Boxes whose vertical ranges overlap (or come within
    ``vertical_gap``) share a stripe; stripes come back top first
    (unspecified upstream; our behaviour).
    """
    ordered = sorted((_coords(box) for box in boxes), key=lambda item: item[1])
    stripes = []
    for item in ordered:
        if stripes and item[1] <= stripes[-1][1] + vertical_gap:
            stripe = stripes[-1]
            stripe[0].append(item)
            stripe[1] = max(stripe[1], item[3])
        else:
            stripes.append([[item], item[3]])
    return [stripe for stripe, _ in stripes]


def cluster_columns_in_stripe(stripe):
    """Split one stripe into column clusters.

    Boxes chain into a cluster by horizontal overlap; a gap starts a
    new cluster (unspecified upstream; our behaviour).
    """
    ordered = sorted((_coords(box) for box in stripe), key=lambda item: item[0])
    clusters = []
    for item in ordered:
        if clusters and item[0] <= clusters[-1][1]:
            cluster, edge = clusters[-1]
            cluster.append(item)
            clusters[-1] = (cluster, max(edge, item[2]))
        else:
            clusters.append(([item], item[2]))
    return [cluster for cluster, _ in clusters]


def compute_reading_order(boxes, joined_boxes, vectors, vertical_gap=12):
    """Compute the full reading order for a box set.

    Boxes run stripe by stripe, top to bottom, and inside each
    stripe cluster by cluster, left to right. The answer is the
    input indices in that order (unspecified upstream; our
    behaviour).
    """
    indexed = [(index, _coords(box)) for index, box in enumerate(boxes)]
    indexed.sort(key=lambda pair: pair[1][1])
    stripes = []
    for index, item in indexed:
        if stripes and item[1] <= stripes[-1][1] + vertical_gap:
            stripe = stripes[-1]
            stripe[0].append((index, item))
            stripe[1] = max(stripe[1], item[3])
        else:
            stripes.append([[(index, item)], item[3]])
    order = []
    for stripe, _ in stripes:
        stripe.sort(key=lambda pair: pair[1][0])
        clusters = []
        for index, item in stripe:
            if clusters and item[0] <= clusters[-1][1]:
                cluster, edge = clusters[-1]
                cluster.append(index)
                clusters[-1] = (cluster, max(edge, item[2]))
            else:
                clusters.append(([index], item[2]))
        for cluster, _ in clusters:
            order.extend(cluster)
    return order


def _overlap_area(first, second):
    ax0, ay0, ax1, ay1 = _coords(first)
    bx0, by0, bx1, by1 = _coords(second)
    return max(0.0, min(ax1, bx1) - max(ax0, bx0)) * max(
        0.0, min(ay1, by1) - max(ay0, by0)
    )


def find_reading_order(page_rect, blocks, boxes, vertical_gap: float = 12) -> list:
    """Order boxes the way a reader would scan the page.

    Each block joins the column box it overlaps most (ties go to the
    leftmost); blocks outside every box form their own column in
    position order. The answer is the block indices, column by
    column in reading order (unspecified upstream; our behaviour).
    """
    columns = [_coords(box) for box in boxes]
    order = compute_reading_order(
        columns, columns, [], vertical_gap=vertical_gap
    )
    ranked = {column: rank for rank, column in enumerate(order)}
    assigned = []
    stray = []
    for index, block in enumerate(blocks):
        best, best_area = None, 0.0
        for column, rect in enumerate(columns):
            area = _overlap_area(block, rect)
            if area > best_area:
                best, best_area = column, area
        if best is None:
            stray.append(index)
        else:
            x0, y0, _, _ = _coords(block)
            assigned.append((ranked[best], y0, x0, index))
    assigned.sort()
    stray.sort(key=lambda index: (_coords(blocks[index])[1], _coords(blocks[index])[0]))
    return [index for _, _, _, index in assigned] + stray


def find_virtual_lines(page, table_bbox, words, vectors, link_rects):
    """Locate implied grid lines inside a table region.

    Gaps between word runs become column boundaries and gaps
    between word rows become row boundaries, each spanning the
    table box (unspecified upstream; our behaviour).
    """
    tx0, ty0, tx1, ty1 = _coords(table_bbox)
    runs = []
    for word in words or []:
        try:
            runs.append(_coords(word))
        except TypeError:
            continue
    if not runs:
        return []
    runs.sort(key=lambda run: (run[1], run[0]))
    rows = []
    for run in runs:
        if rows and run[1] <= rows[-1][1]:
            rows[-1][0].append(run)
            rows[-1][1] = max(rows[-1][1], run[3])
        else:
            rows.append([[run], run[3]])
    bounds = []
    for index in range(1, len(rows)):
        gap = rows[index][0][0][1] - rows[index - 1][1]
        if gap > 0:
            edge = (rows[index - 1][1] + rows[index][0][0][1]) / 2
            bounds.append(Rect(tx0, edge, tx1, edge))
    columns = sorted((run[0], run[2]) for run in runs)
    for index in range(1, len(columns)):
        if columns[index][0] > columns[index - 1][1]:
            edge = (columns[index - 1][1] + columns[index][0]) / 2
            bounds.append(Rect(edge, ty0, edge, ty1))
    return bounds


def simplify_vectors(vectors):
    """Reduce a vector list to its salient strokes.

    Zero-length strokes are dropped and the survivors come back in
    reading order (unspecified upstream; our behaviour).
    """
    kept = []
    for vector in vectors or []:
        x0, y0, x1, y1 = _coords(vector)
        if x0 == x1 and y0 == y1:
            continue
        kept.append(Rect(x0, y0, x1, y1))
    kept.sort(key=lambda box: (box.y0, box.x0))
    return kept


def _pages_shape(pages):
    """Classify the ``pages`` option: none, int, sequence, or other."""
    if pages is None:
        return "none"
    if isinstance(pages, bool):
        return "int"
    if isinstance(pages, int):
        return "int"
    if isinstance(pages, (list, tuple, range)):
        return "seq"
    return "other"


def _check_page_elements(items, layout):
    """Refuse non-integer page numbers with the mode's error."""
    for item in items:
        if isinstance(item, bool) or isinstance(item, int):
            continue
        if layout:
            raise ValueError("page numbers must be integers, got %r" % (item,))
        raise TypeError("page numbers must be integers, got %r" % (item,))


def _resolve_index(value, page_count, layout):
    """Fold one (possibly negative) page number into range."""
    index = int(value) + page_count if int(value) < 0 else int(value)
    if 0 <= index < page_count:
        return index
    if layout:
        raise ValueError(
            "page %r out of range for a %d-page document" % (value, page_count)
        )
    raise IndexError(
        "page %r out of range for a %d-page document" % (value, page_count)
    )


def resolve_pages(pages, page_count, *, layout):
    """Normalise the ``pages`` option to a sorted or ordered index list.

    Layout mode sorts and de-duplicates (a bare integer names one
    page); legacy mode keeps order and repeats. Tuples and ranges
    are accepted like lists in both modes (unspecified upstream;
    our behaviour). Errors follow the recorded contract:
    ``TypeError``/``AssertionError`` for mistyped legacy input,
    ``ValueError`` for mistyped or out-of-range layout input, and
    ``IndexError`` for out-of-range legacy input.
    """
    shape = _pages_shape(pages)
    if shape == "none":
        return list(range(page_count))
    if layout:
        if shape == "int":
            return [_resolve_index(pages, page_count, True)]
        if shape == "seq":
            items = list(pages)
            if not items:
                raise IndexError("pages is empty")
            _check_page_elements(items, True)
            return sorted(
                {_resolve_index(item, page_count, True) for item in items}
            )
        raise ValueError("pages must be an integer or a list, got %r" % (pages,))
    if shape == "int":
        raise TypeError("pages must be a list, got %r" % (pages,))
    if shape == "seq":
        items = list(pages)
        _check_page_elements(items, False)
        return [_resolve_index(item, page_count, False) for item in items]
    raise AssertionError("pages must be a list, got %r" % (pages,))


def _is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def resolve_margins(margins, *, layout):
    """Normalise the ``margins`` option to ``[left, top, right, bottom]``.

    Layout mode ignores margins entirely (no validation). Legacy
    mode takes a scalar or a four-number sequence; the sequence
    runs left, top, right, bottom like the engine's clip margins, a
    bare ``None`` means no margins, and anything else raises
    ``ValueError`` (unspecified upstream; our behaviour).
    """
    if layout:
        return None
    if margins is None:
        return None
    if _is_number(margins) or isinstance(margins, bool):
        value = float(margins)
        if not math.isfinite(value):
            raise ValueError("margins must be finite, got %r" % (margins,))
        if value < 0:
            raise ValueError("margins must not be negative, got %r" % (margins,))
        return [value, value, value, value]
    if isinstance(margins, (list, tuple, range)):
        items = list(margins)
        if len(items) != 4 or not all(
            _is_number(item) or isinstance(item, bool) for item in items
        ):
            raise ValueError(
                "margins must be a number or four numbers, got %r" % (margins,)
            )
        values = [float(item) for item in items]
        if any(not math.isfinite(value) for value in values):
            raise ValueError("margins must be finite, got %r" % (margins,))
        if any(value < 0 for value in values):
            raise ValueError(
                "margins must not be negative, got %r" % (margins,)
            )
        return values
    raise ValueError("margins must be a number or four numbers, got %r" % (margins,))


def _pages_rule_for(entry):
    """Build the page-option rule for one entry point.

    Rules see options without the document, so they pin only the
    document-free checks (mistyped input, empty lists, the
    parse-level integer refusal); range checks live in
    :func:`resolve_pages`, which the converters call with the page
    count.
    """

    def rule(options, layout=False):
        pages = options.get("pages")
        shape = _pages_shape(pages)
        if shape == "none":
            return CONTINUE
        if entry == "parse_document" and shape == "int":
            raise TypeError("pages must be a list, got %r" % (pages,))
        if layout:
            if shape == "int":
                return CONTINUE
            if shape == "seq":
                items = list(pages)
                if not items:
                    raise IndexError("pages is empty")
                _check_page_elements(items, True)
                return CONTINUE
            raise ValueError(
                "pages must be an integer or a list, got %r" % (pages,)
            )
        if shape == "int":
            raise TypeError("pages must be a list, got %r" % (pages,))
        if shape == "seq":
            items = list(pages)
            if not items:
                if entry == "to_markdown" and not (
                    options.get("page_chunks") or options.get("extract_words")
                ):
                    return ""
                return CONTINUE
            _check_page_elements(items, False)
            return CONTINUE
        raise AssertionError("pages must be a list, got %r" % (pages,))

    return rule


for _entry in ("to_markdown", "to_text", "to_json", "parse_document"):
    register_option_rule(_entry, _pages_rule_for(_entry))


def _margins_rule(options, layout=False):
    """Refuse mistyped legacy margins; layout mode ignores them."""
    resolve_margins(options.get("margins"), layout=layout)
    return CONTINUE


register_option_rule("to_markdown", _margins_rule)
