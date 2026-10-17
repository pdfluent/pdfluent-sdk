# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L1: SPEC sections 2, 3, 10, 12, 18, 23, 24.
"""Markdown converters, header steering and line-level text helpers.

This module holds the legacy converter, the shared line grouping, the
header detectors and the pure box renderers. Sibling lanes own the
layout parser (``_chunks``), table shapes (``_tables``), image bytes
(``_images``), geometry boxes (``_geometry``) and input plumbing
(``_inputs``); every call into those modules is marked as a contract
assumption so the integration round can align shapes.
"""
from __future__ import annotations

import base64
import os
import re
import struct
from collections.abc import Mapping

from ._errors import CONTINUE, check_call
from ._mode import is_layout_mode

BULLETS = ("•", "·", "◦", "▪", "▸", "‣", "-", "–", "—", "*")
GRAPHICS_TEXT = "\n![](%s)\n"
REPLACEMENT_CHARACTER = "\ufffd"
TYPE3_FONT_NAME = "Type3"
WHITE_CHARS = {" ", "\t", "\n", "\r", "\f", "\v", "\xa0"}
FLAGS = 36420

_BOLD_BIT = 16
_ITALIC_BIT = 2
_MONO_BIT = 8
_HIDDEN_BIT = 32
_ROTATED_BIT = 64

#: Recorded span font metrics for wired (embedded-font) spans: ascender
#: 0.8 / descender -0.2 (as f32). Base-14 spans keep 0.0 until the owner
#: decides their known difference.
_SPAN_ASCENT = 0.8
_SPAN_DESCENT = -0.2


def _f32(value):
    """Round a float through single precision, like the text engine."""
    return struct.unpack("f", struct.pack("f", value))[0]

_ORDERED_ITEM_RE = re.compile(r"^(\d+)[.)]\s+(.*)$", re.DOTALL)
_PDF_VERSION_RE = re.compile(rb"%PDF-(\d+)\.(\d+)")

_LAYOUT_PARSE_KEYS = frozenset(
    {
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
    }
)
_LAYOUT_RENDER_KEYS = frozenset(
    {
        "header",
        "footer",
        "write_images",
        "embed_images",
        "ignore_code",
        "show_progress",
        "page_separators",
        "page_chunks",
    }
)


class Parameters:
    """Tunable layout thresholds for the legacy converter."""

    def __init__(self) -> None:
        self.line_tolerance = 3.0
        self.column_gap = 100.0
        self.code_indent_step = 10.0


def _native_pdfluent():
    """Import the compiled extension lazily so pure helpers stay importable."""
    import pdfluent

    return pdfluent


def _exact_page_size(page):
    """Exact page size; the native width/height round through f32."""
    return _native_pdfluent().door_page_size(page)


def _rich_geometry_by_id(spans, textpage):
    """Map span objects to rich geometry where the alignment verifies.

    The native entries run in extraction order, one per span, so a zip
    with the ungrouped spans aligns them; text and origin must match
    exactly or the entry stays out and the span keeps its point box.
    """
    try:
        entries = _native_pdfluent().door_page_span_geometry(textpage)
    except (AttributeError, TypeError):
        return {}
    if len(entries) != len(spans):
        return {}
    rich_by_id = {}
    for (_, span), entry in zip(spans, entries):
        if (
            entry[0] == span.text
            and entry[1] == float(span.x)
            and entry[2] == float(span.y)
        ):
            rich_by_id[id(span)] = entry
    return rich_by_id


def _is_path_like(doc):
    try:
        os.fspath(doc)
    except TypeError:
        return False
    return isinstance(doc, (str, bytes, os.PathLike))


class _opened_doc:
    """Open path inputs, pass open documents through untouched."""

    def __init__(self, doc):
        self._doc = doc
        self.handle = None
        self.owned = False

    def __enter__(self):
        if _is_path_like(self._doc):
            pdfluent = _native_pdfluent()
            source = self._doc
            if isinstance(source, os.PathLike):
                source = os.fspath(source)
            self.handle = pdfluent.Document(source)
            self.owned = True
        else:
            self.handle = self._doc
        return self.handle

    def __exit__(self, exc_type, exc, tb):
        if self.owned:
            close = getattr(self.handle, "close", None)
            if close is not None:
                close()
        return False


def _label_for(doc):
    if isinstance(doc, str):
        return doc
    if isinstance(doc, os.PathLike):
        return os.fspath(doc)
    # Unspecified upstream; our behaviour: non-path inputs log as "document".
    return "document"


def _rect4(rect):
    """Read [x0, y0, x1, y1] off a sequence or a rect-like object."""
    try:
        return (rect[0], rect[1], rect[2], rect[3])
    except (TypeError, KeyError, IndexError):
        return (rect.x0, rect.y0, rect.x1, rect.y1)


def _iter_native_spans(blocks):
    """Flatten native text blocks to (block_index, span) in draw order."""
    for block_no, block in enumerate(blocks):
        for span in block.spans:
            yield block_no, span


def _span_xy(span):
    return (span.x, span.y)


def _style_bits(flags):
    return int(flags) & (_BOLD_BIT | _ITALIC_BIT | _MONO_BIT)


def _group_spans(spans, tolerance):
    """Cluster spans into top-down lines; spans inside a line run left to right."""
    ordered = sorted(spans, key=lambda item: (-item[1].y, item[1].x))
    lines = []
    for block_no, span in ordered:
        if lines and (lines[-1][-1][1].y - span.y) <= tolerance:
            lines[-1].append((block_no, span))
        else:
            lines.append([(block_no, span)])
    for line in lines:
        line.sort(key=lambda item: item[1].x)
    return lines


def _line_size(line):
    return max(float(span.font_size) for _, span in line)


def _line_x0(line):
    return min(float(span.x) for _, span in line)


def _clean_span_text(text):
    """Map extractor NULs for missing glyphs to replacement characters."""
    if not text:
        return text
    return text.replace("\x00", REPLACEMENT_CHARACTER)


def _span_dict(block_no, span, line_no, page_no=0, page_height=None, rich=None):
    """Build the recorded span mapping from one native span.

    Recorded points use a top-left origin while native spans use a
    bottom-left one, so the height flips the vertical axis. The native
    span only carries a point, a size and style bits, so the box/origin
    pairs stay point-shaped and the font metrics stay zero; both
    limits are documented where layout consumes them. A span the rich
    geometry wires (embedded font, upright) instead reads its box from
    the advance-sum end and the tight ink band, with metrics as recorded.
    """
    flags = int(span.flags)
    color = span.color
    if color is None:
        color_int = 0
    else:
        try:
            red, green, blue = color
        except (TypeError, ValueError):
            red, green, blue = (0.0, 0.0, 0.0)
        color_int = (int(red * 255) << 16) | (int(green * 255) << 8) | int(blue * 255)
    x = float(span.x)
    y = float(span.y)
    if page_height is not None:
        y = float(page_height) - y
    text = _clean_span_text(span.text)
    box = [x, y, x, y]
    ascender = 0.0
    descender = 0.0
    if rich is not None and rich[6] and page_height is not None:
        _text, _rx, _ry, end_x, ink_y0, ink_y1, _wired = rich
        if ink_y0 is None or ink_y1 is None:
            box = [x, y, float(end_x), y]
        else:
            height = float(page_height)
            box = [x, height - float(ink_y1), float(end_x), height - float(ink_y0)]
        ascender = _f32(_SPAN_ASCENT)
        descender = _f32(_SPAN_DESCENT)
    return {
        "size": float(span.font_size),
        "flags": _style_bits(flags),
        "bidi": 0,
        "char_flags": 0,
        "font": span.font_name if span.font_name is not None else "",
        "color": color_int,
        "alpha": 255,
        "ascender": ascender,
        "descender": descender,
        "text": text,
        "origin": [x, y],
        "bbox": box,
        "line": line_no,
        "block": block_no,
        "dir": [1.0, 0.0],
    }


def _line_record(line, line_no, page_no=0, page_height=None, rich_by_id=None):
    dicts = [
        _span_dict(
            block_no,
            span,
            line_no,
            page_no,
            page_height,
            (rich_by_id or {}).get(id(span)),
        )
        for block_no, span in line
    ]
    xs = [part["origin"][0] for part in dicts]
    ys = [part["origin"][1] for part in dicts]
    bbox = [min(xs), min(ys), max(xs), max(ys)] if dicts else None
    return [bbox, dicts]


def _point_in_top_left_clip(x, y_bottom_left, page_height, clip):
    """Clip uses a top-left origin; spans use a bottom-left one."""
    x0, y0, x1, y1 = _rect4(clip)
    y_top_left = page_height - y_bottom_left
    return x0 <= x <= x1 and y0 <= y_top_left <= y1


def _filter_spans_by_clip(spans, clip, page_height):
    if clip is None:
        return list(spans)
    return [
        item
        for item in spans
        if _point_in_top_left_clip(item[1].x, item[1].y, page_height, clip)
    ]


def _split_cells(line, size):
    """Split one grouped line into cells at wide horizontal gaps.

    Native spans carry no widths, so the gap is measured between span
    starts; a gap wider than two ems opens a new cell. Unspecified
    upstream; our behaviour documents the two-em threshold.
    """
    cells = []
    for block_no, span in line:
        if cells and (float(span.x) - cells[-1][-1][1].x) > 2.0 * size:
            cells.append([(block_no, span)])
        elif cells:
            cells[-1].append((block_no, span))
        else:
            cells.append([(block_no, span)])
    return cells


def _join_span_texts(items, size):
    """Join x-ordered spans, spacing words the extractor left bare.

    Native spans may split a line into characters while keeping the
    word spaces as their own spans; those join back tightly. A gap
    wider than one em with no bordering whitespace opens a word.
    """
    text = ""
    prev_x = None
    for _, span in items:
        piece = _clean_span_text(span.text)
        if text and prev_x is not None:
            gap = float(span.x) - prev_x
            bordered = text[-1:].isspace() or piece[:1].isspace()
            if gap > size and not bordered:
                text += " "
        text += piece
        prev_x = float(span.x)
    return text.strip()


def get_text_lines(page, *, textpage=None, clip=None, sep="\t", tolerance=3, ocr=False):
    """Group one page's text into lines, optionally clipped."""
    if ocr:
        return ""
    src = textpage if textpage is not None else page
    spans = list(_iter_native_spans(src.extract_text_blocks()))
    spans = _filter_spans_by_clip(spans, clip, _exact_page_size(src)[1])
    # The line helper keeps every span it is given: unlike the raw
    # variant it has no visibility or rotation switches, so hidden and
    # turned spans stay in the output. Unspecified upstream; our
    # behaviour keeps them verbatim.
    if sep == "\t" or sep is None:
        # Unspecified upstream; our behaviour treats the default tab
        # and None as "no separator" because cells join tightly.
        effsep = ""
    else:
        effsep = sep
    out = "\n"
    for line in _group_spans(spans, tolerance):
        size = _line_size(line)
        cells = _split_cells(line, size)
        texts = []
        for cell in cells:
            joined = _join_span_texts(cell, size)
            if joined:
                texts.append(joined)
        if not texts:
            continue
        out += effsep.join(texts) + "\n\n"
    return out


def get_raw_lines(
    textpage=None,
    blocks=None,
    clip=None,
    tolerance=3,
    ignore_invisible=True,
    only_horizontal=True,
):
    """Return raw [bbox, spans] line pairs for a text page."""
    if blocks is not None:
        raise TypeError("get_raw_lines only accepts a text page, not blocks")
    if textpage is None:
        # Unspecified upstream; our behaviour returns no lines.
        return []
    spans = list(_iter_native_spans(textpage.extract_text_blocks()))
    rich_by_id = _rich_geometry_by_id(spans, textpage)
    if only_horizontal:
        spans = [item for item in spans if not (int(item[1].flags) & _ROTATED_BIT)]
    if ignore_invisible:
        visible = [
            item for item in spans if not (int(item[1].flags) & _HIDDEN_BIT)
        ]
        # Pages that hold visible text drop their hidden spans; a page
        # with nothing visible keeps them so scans still read.
        spans = visible if visible else spans
    height = _exact_page_size(textpage)[1]
    spans = _filter_spans_by_clip(spans, clip, height)
    grouped = _group_spans(spans, tolerance)
    page_no = int(getattr(textpage, "index", 0) or 0)
    return [
        _line_record(line, line_no, page_no, height, rich_by_id)
        for line_no, line in enumerate(grouped)
    ]


def is_white(text) -> bool:
    """Return True for blank or empty strings."""
    return text == "" or all(char in WHITE_CHARS for char in text)


def startswith_bullet(text):
    """Return True when the text opens with a bullet marker."""
    stripped = text.lstrip() if isinstance(text, str) else ""
    return stripped.startswith(BULLETS)


def _is_private_use(char):
    code = ord(char)
    return (
        0xE000 <= code <= 0xF8FF
        or 0xF0000 <= code <= 0xFFFFD
        or 0x100000 <= code <= 0x10FFFD
    )


def omit_if_pua_char(text):
    """Drop strings made only of private-use characters."""
    if text and all(_is_private_use(char) for char in text):
        return ""
    return text


def _span_markers(flags):
    """Open/close emphasis markers for one span's style bits."""
    opening = ""
    closing = ""
    if flags & _BOLD_BIT:
        opening += "**"
        closing = "**" + closing
    if flags & _ITALIC_BIT:
        opening += "_"
        closing = "_" + closing
    if flags & _MONO_BIT:
        opening += "`"
        closing = "`" + closing
    return opening, closing


def _with_trailing_space(text):
    if text == "" or text.endswith(" ") or text.endswith("\n"):
        return text
    return text + " "


def _flush_styled_run(opening, closing, text):
    """One style run with markers hugging the text.

    Inner spaces stay marked ('**bold word**'); trailing spaces break
    out past the closing marker ('**bold word** and'), and a run with
    no text at all renders bare.
    """
    if opening == "" and closing == "":
        return text
    core = text.rstrip()
    if core == "":
        return text
    return opening + core + closing + text[len(core) :]


def get_styled_text(spans):
    """Render spans with emphasis markers plus a pending marker.

    Adjacent spans in the same style share one run ('**Band**',
    not '**B****a**...'): markers wrap the joined text.
    """
    rendered = ""
    pending = ""
    run = ""
    opening = ""
    closing = ""
    for span in spans:
        text = omit_if_pua_char(span.get("text", ""))
        if text == "":
            continue
        markers = _span_markers(int(span.get("flags", 0)))
        if run != "" and markers == (opening, closing):
            run += text
            continue
        if run != "":
            rendered += _flush_styled_run(opening, closing, run)
        opening, closing = markers
        run = text
        pending = closing
    if run != "":
        rendered += _flush_styled_run(opening, closing, run)
    if rendered == "":
        return ["", ""]
    return [_with_trailing_space(rendered), pending]


def get_plain_text(spans):
    """Render spans as a bare string."""
    rendered = ""
    for span in spans:
        rendered += omit_if_pua_char(span.get("text", ""))
    return _with_trailing_space(rendered)


def _line_spans(line):
    if isinstance(line, dict):
        return line.get("spans", [])
    return getattr(line, "spans", [])


def is_monospaced(textlines):
    """Return True when the lines use a monospaced face."""
    seen = 0
    for line in textlines:
        for span in _line_spans(line):
            seen += 1
            flags = span.get("flags", 0) if isinstance(span, dict) else span.flags
            if not (int(flags) & _MONO_BIT):
                return False
    return seen > 0


def is_superscripted(line):
    """Return True when the line sits above the baseline.

    Unspecified upstream; our behaviour compares span heights inside
    the line and reports a raised smaller span.
    """
    spans = list(_line_spans(line))

    def _point(item):
        if isinstance(item, dict):
            origin = item.get("origin", [0.0, 0.0])
            return float(origin[1]), float(item.get("size", 0.0) or 0.0)
        return float(item.y), float(item.font_size)

    if len(spans) < 2:
        return False
    points = [_point(span) for span in spans]
    sizes = [size for _, size in points if size > 0]
    if not sizes:
        return False
    floor = min(sizes)
    top = min(y for y, _ in points)
    base = max(y for y, _ in points)
    return (base - top) > 0.5 * floor


def _styled_lines(textlines):
    return [get_styled_text(_line_spans(line))[0] for line in textlines]


def _plain_lines(textlines, strip=True):
    out = []
    for line in textlines:
        text = get_plain_text(_line_spans(line))
        out.append(text.strip() if strip else text)
    return out


def _drop_white_lines(textlines):
    return [line for line in textlines if get_plain_text(_line_spans(line)).strip()]


def _clip_textlines(textlines, clip):
    """Keep text lines whose box touches the clip rectangle, if any."""
    if clip is None:
        return list(textlines)
    x0, y0, x1, y1 = _rect4(clip)
    kept = []
    for line in textlines:
        if isinstance(line, dict):
            box = line.get("bbox")
        else:
            box = getattr(line, "bbox", None)
        if box is None:
            kept.append(line)
            continue
        bx0, by0, bx1, by1 = _rect4(box)
        if bx0 <= x1 and bx1 >= x0 and by0 <= y1 and by1 >= y0:
            kept.append(line)
    return kept


def section_hdr_to_md(header_level, textlines):
    """Render one header line as a markdown heading."""
    level = max(1, min(6, int(header_level)))
    lines = [text.rstrip() for text in _styled_lines(_drop_white_lines(textlines))]
    if not lines:
        return ""
    return "#" * level + " " + " ".join(lines) + " \n\n"


def title_to_md(header_level, textlines):
    """Render one title line as a markdown heading."""
    return section_hdr_to_md(header_level, textlines)


def _mono_body(textlines):
    return " ".join(text for text in _plain_lines(textlines) if text)


def text_to_md(textlines, ignore_code: bool = False):
    """Render prose lines as markdown."""
    lines = _drop_white_lines(textlines)
    if not lines:
        return ""
    if ignore_code and is_monospaced(lines):
        return "`" + _mono_body(lines) + "` \n\n"
    return "".join(_styled_lines(lines)) + "\n\n"


def text_to_text(textlines, ignore_code: bool = False):
    """Render prose lines as plain text."""
    lines = _drop_white_lines(textlines)
    if not lines:
        return ""
    if ignore_code and is_monospaced(lines):
        return _mono_body(lines) + " \n\n"
    joined = "".join(
        _with_trailing_space(get_plain_text(_line_spans(line))) for line in lines
    )
    return joined + "\n\n"


def _fallback_table(textlines):
    rows = []
    width = 1
    for line in textlines:
        cells = [
            omit_if_pua_char(span.get("text", "")).strip()
            if isinstance(span, dict)
            else omit_if_pua_char(span.text).strip()
            for span in _line_spans(line)
        ]
        width = max(width, len(cells) or 1)
        rows.append(cells)
    head = "|" + "|".join([""] * width) + "|"
    rule = "|" + "|".join(["---"] * width) + "|"
    out = head + "\n" + rule + "\n"
    for cells in rows:
        padded = list(cells) + [""] * (width - len(cells))
        out += "|" + "|".join(padded) + "|\n"
    return out


def fallback_text_to_md(textlines, ignore_code: bool = False, clip=None):
    """Render fallback prose lines as markdown."""
    lines = _clip_textlines(_drop_white_lines(textlines), clip)
    return (
        "<!-- Start of picture text -->\n"
        + _fallback_table(lines)
        + "\n<!-- End of picture text -->\n\n"
    )


def fallback_text_to_text(textlines, ignore_code: bool = False, clip=None):
    """Render fallback prose lines as plain text."""
    lines = _clip_textlines(_drop_white_lines(textlines), clip)
    rows = _plain_lines(lines)
    if not rows:
        return ""
    width = max(len(text) for text in rows)
    bar = "+" + "-" * (width + 2) + "+\n"
    out = bar
    for text in rows:
        out += "| " + text.ljust(width) + " |\n"
    return out + bar + "\n"


def picture_text_to_md(textlines, ignore_code: bool = False, clip=None):
    """Render picture-bound prose lines as markdown."""
    lines = _clip_textlines(_drop_white_lines(textlines), clip)
    body = "<br>".join(_plain_lines(lines))
    return (
        "<!-- Start of picture text -->\n"
        + body
        + "<br><!-- End of picture text -->\n\n"
    )


def picture_text_to_text(textlines, ignore_code: bool = False, clip=None):
    """Render picture-bound prose lines as plain text."""
    lines = _clip_textlines(_drop_white_lines(textlines), clip)
    body = "\n".join(_plain_lines(lines))
    return (
        "----- Start of picture text -----\n"
        + body
        + "\n----- End of picture text -----\n\n"
    )


def _code_lines(textlines):
    rows = []
    for line in textlines:
        text = ""
        for span in _line_spans(line):
            raw = span.get("text", "") if isinstance(span, dict) else span.text
            text += omit_if_pua_char(raw)
        rows.append(text)
    return rows


def code_block_to_md(textlines):
    """Render monospaced lines as a fenced markdown block."""
    return "```\n" + "\n".join(_code_lines(textlines)) + "\n```\n\n"


def code_block_to_text(textlines):
    """Render monospaced lines as raw plain-text lines."""
    rows = _code_lines(textlines)
    if not rows:
        return ""
    return "\n".join(rows) + "\n\n\n"


def _list_marker(text):
    """Split a list line into its marker kind, number and rest."""
    stripped = text.strip()
    ordered = _ORDERED_ITEM_RE.match(stripped)
    if ordered:
        return ("ordered", int(ordered.group(1)), ordered.group(2).strip())
    if startswith_bullet(stripped):
        rest = stripped.lstrip("".join(BULLETS)).strip()
        return ("bullet", None, rest)
    # Unspecified upstream; our behaviour renders marker-free lines
    # passed here as plain bullets.
    return ("bullet", None, stripped)


def list_item_to_md(textlines, level):
    """Render list lines as a markdown item at the given level."""
    indent = " " * (3 * max(0, int(level) - 1))
    parts = []
    marker = "- "
    for line in textlines:
        spans = _line_spans(line)
        plain = get_plain_text(spans).strip()
        if plain == "":
            continue
        kind, number, _ = _list_marker(plain)
        styled = get_styled_text(spans)[0].strip()
        _, _, rest = _list_marker(styled)
        if kind == "ordered":
            marker = "%d.  " % number
        parts.append(rest)
    body = " ".join(parts)
    return indent + marker + body + " \n\n"


def list_item_to_text(textlines, level) -> str:
    """Render list lines as a plain-text item at the given level."""
    indent = " " * (3 * max(0, int(level) - 1))
    parts = []
    marker = None
    for line in textlines:
        text = get_plain_text(_line_spans(line)).strip()
        if text == "":
            continue
        kind, number, rest = _list_marker(text)
        if marker is None:
            # The first line sets the marker: ordered numbers and the
            # source '-' survive, every other bullet reads '•'.
            if kind == "ordered":
                marker = "%d.  " % number
            elif text.startswith("-"):
                marker = "-  "
            else:
                marker = "•  "
        parts.append(rest)
    return indent + (marker or "•  ") + " ".join(parts) + "\n\n"


def footnote_to_md(textlines):
    """Render footnote lines as markdown."""
    lines = _drop_white_lines(textlines)
    if not lines:
        return ""
    return "> " + "".join(_styled_lines(lines)) + "\n\n"


def footnote_to_text(textlines) -> str:
    """Render footnote lines as plain text."""
    lines = _drop_white_lines(textlines)
    if not lines:
        return ""
    body = "".join(get_plain_text(_line_spans(line)) for line in lines).rstrip(" ")
    return "> " + body + "\n\n"


def _item_x0(item):
    if isinstance(item, dict):
        if "x0" in item:
            return float(item["x0"])
        box = item.get("bbox")
        if box is not None:
            return float(_rect4(box)[0])
        lines = item.get("textlines", [])
        if lines:
            first = lines[0]
            box = first.get("bbox") if isinstance(first, dict) else None
            if box is not None:
                return float(_rect4(box)[0])
            spans = _line_spans(first)
            if spans:
                first_span = spans[0]
                if isinstance(first_span, dict):
                    return float(first_span.get("origin", [0.0])[0])
                return float(first_span.x)
        return 0.0
    for attr in ("x0", "bbox"):
        value = getattr(item, attr, None)
        if value is not None:
            return float(value) if attr == "x0" else float(_rect4(value)[0])
    return 0.0


def create_list_item_levels(layout_info):
    """Derive nesting levels for list items in a layout.

    Unspecified upstream; our behaviour ranks the distinct
    left edges: the leftmost items sit at level 1 and each step
    right adds one level.
    """
    items = list(layout_info)
    edges = sorted({round(_item_x0(item)) for item in items})
    rank = {edge: number + 1 for number, edge in enumerate(edges)}
    return [rank[round(_item_x0(item))] for item in items]


def _page_boxes(page):
    if isinstance(page, dict):
        return page.get("boxes", [])
    return getattr(page, "boxes", [])


def _box_text_size(box):
    if isinstance(box, dict):
        lines = box.get("textlines", [])
    else:
        lines = getattr(box, "textlines", [])
    sizes = []
    for line in lines:
        for span in _line_spans(line):
            size = span.get("size", 0) if isinstance(span, dict) else span.font_size
            sizes.append(round(float(size), 1))
    return max(sizes) if sizes else None


def update_header_tags(pages, header_fontsizes):
    """Refresh heading levels from the observed header sizes.

    Unspecified upstream; our behaviour maps each text box whose
    largest size appears in ``header_fontsizes`` (largest first) to
    levels 1..6 and tags the box as a section header.
    """
    order = [round(float(size), 1) for size in header_fontsizes]
    for page in pages:
        for box in _page_boxes(page):
            size = _box_text_size(box)
            if size is None or size not in order:
                continue
            level = max(1, min(6, order.index(size) + 1))
            if isinstance(box, dict):
                box["header_level"] = level
                if box.get("boxclass") in (None, "text"):
                    box["boxclass"] = "section-header"
            else:
                try:
                    box.header_level = level
                except AttributeError:
                    pass
                if getattr(box, "boxclass", None) in (None, "text"):
                    try:
                        box.boxclass = "section-header"
                    except AttributeError:
                        pass


def normalize_layout_boxes(layout_boxes, html_tables, fulltext):
    """Reconcile layout boxes with tables and full text.

    Unspecified upstream; our behaviour keeps the given boxes in
    order and appends any table boxes the parser produced.
    """
    boxes = list(layout_boxes)
    for table in html_tables or []:
        boxes.append(table)
    return boxes


def _legacy_page_numbers(pages, page_count):
    """Normalise the legacy ``pages`` option to 0-based indices."""
    if pages is None:
        return list(range(page_count))
    if isinstance(pages, bool):
        raise TypeError("pages must be a list of page indices or None")
    if isinstance(pages, int):
        raise TypeError("pages must be a list of page indices or None")
    assert isinstance(pages, (list, tuple)), "pages must be a list of page indices"
    out = []
    for entry in pages:
        assert isinstance(entry, int) and not isinstance(entry, bool), (
            "pages must be a list of page indices"
        )
        number = entry + page_count if entry < 0 else entry
        if number < 0 or number >= page_count:
            raise IndexError("page index out of range: %r" % (entry,))
        out.append(number)
    return out


class IdentifyHeaders:
    """Header detector steered by body size and page selection."""

    def __init__(
        self, doc: str, pages: list = None, body_limit: float = 12, max_levels: int = 6
    ):
        self.body_limit = body_limit
        self.header_id = {}
        with _opened_doc(doc) as handle:
            count = handle.page_count
            if callable(count):
                count = count()
            numbers = _legacy_page_numbers(pages, int(count))
            census = {}
            for number in numbers:
                page = handle[number]
                for _, span in _iter_native_spans(page.extract_text_blocks()):
                    flags = int(span.flags)
                    if flags & (_HIDDEN_BIT | _ROTATED_BIT):
                        continue
                    size = round(float(span.font_size), 1)
                    census.setdefault(size, []).append(span.text)
            if not census:
                return
            candidates = [size for size in census if size <= body_limit]
            if not candidates:
                return
            body = max(candidates)
            body_chars = sum(len(text.strip()) for text in census[body])
            headers = []
            for size in sorted(census, reverse=True):
                if size <= body_limit:
                    continue
                chars = sum(len(text.strip()) for text in census[size])
                if chars < body_chars:
                    headers.append(size)
            headers = headers[: max(0, int(max_levels))]
            for rank, size in enumerate(headers):
                self.header_id[size] = "h%d" % (rank + 1)
            if headers:
                self.body_limit = int(min(headers) - 1)

    def get_header_id(self, span: dict, page=None) -> str:
        try:
            size = round(float(span.get("size", 0.0)), 1)
        except (TypeError, ValueError, AttributeError):
            return ""
        return self.header_id.get(size, "")


class TocHeaders:
    """Header detector steered by the document outline."""

    def __init__(self, doc: str):
        entries = doc.bookmarks
        self.header_id = {}

        def _visit(items, depth):
            for item in items:
                level = max(1, min(6, depth + 1))
                title = item.title
                text = title.strip() if isinstance(title, str) else title
                # Entries without a page cannot be placed; they stay silent.
                if text and item.page is not None:
                    key = (item.page, text)
                    if key not in self.header_id:
                        self.header_id[key] = "h%d" % level
                _visit(item.children, depth + 1)

        _visit(entries, 0)

    def get_header_id(self, span: dict, page=None) -> str:
        if page is None:
            page = span.get("page") if isinstance(span, dict) else None
        text = span.get("text", "") if isinstance(span, dict) else ""
        key_text = text.strip() if isinstance(text, str) else ""
        if not key_text or page is None:
            return ""
        for (stored_page, stored_text), ident in self.header_id.items():
            if stored_text != key_text:
                continue
            # The outline page may count from 0 or 1; accept either.
            if page == stored_page:
                return ident
            try:
                if page == stored_page - 1:
                    return ident
            except TypeError:
                pass
        return ""


def _normalize_margins(margins):
    """Normalise the legacy ``margins`` option to (left, top, right, bottom)."""
    if isinstance(margins, bool):
        margins = int(margins)
    if isinstance(margins, (int, float)):
        value = float(margins)
        return (value, value, value, value)
    try:
        parts = tuple(float(part) for part in margins)
    except (TypeError, ValueError):
        raise ValueError("margins must be a number or a 4-item sequence")
    if len(parts) != 4:
        raise ValueError("margins must be a number or a 4-item sequence")
    return parts


def _legacy_image_infos(native_page, limit):
    """Ask the image lane for this page's pictures (contract assumption).

    Contract: ``_images.extract_images_on_page_simple_drop`` takes the
    native page, a ``Parameters`` instance and the size limit, and
    returns a list of per-image mappings. When the lane is missing the
    converter falls back to native image boxes without bytes.
    """
    try:
        from ._images import extract_images_on_page_simple_drop

        infos = extract_images_on_page_simple_drop(native_page, Parameters(), limit)
        return list(infos)
    except Exception:
        return []


def _legacy_table_findings(handle, number, strategy):
    """This page's tables for rendering, plus its shaded regions.

    Returns ``(renderable, shaded_boxes, regions)``: the findings the
    strategy renders, the bboxes of every shaded proposal whether
    rendered or not -- prose standing on a shaded table is table text,
    so the render path excludes it even where `lines_strict` drops the
    table itself (SPEC G100: shaded output is empty) -- and the bboxes
    of every proposal, which the line grouping reads row by row. `lines` and
    `lines_strict` keep only ruled findings; `text` keeps everything
    but spanning tables, which it renders as prose (SPEC G101), and
    pre-renders its kept findings through the text grid (SPEC G98
    `<br>` tags), falling back to the lines form where that fails.
    """
    try:
        from ._tables import find_tables, text_finding_markdown
    except Exception:
        return [], [], []
    try:
        findings = list(find_tables(handle, number))
    except Exception:
        return [], [], []
    shaded = [item["bbox"] for item in findings if _field(item, "shaded", False)]
    regions = [item["bbox"] for item in findings]
    kept = []
    for item in findings:
        if strategy in ("lines_strict", "lines"):
            if not _field(item, "ruled", True):
                continue
            if strategy == "lines_strict" and _field(item, "shaded", False):
                continue
        elif strategy == "text":
            if _field(item, "spanning", False):
                continue
        kept.append(item)
    if strategy == "text":
        for item in kept:
            try:
                rendered = text_finding_markdown(item).rstrip("\n")
            except Exception:
                continue
            if rendered:
                item["markdown"] = rendered
    return kept, shaded, regions


def _table_markdown(item):
    if isinstance(item, dict):
        if item.get("markdown"):
            return item["markdown"]
        cells = item.get("cells")
    else:
        cells = getattr(item, "cells", None)
        if getattr(item, "markdown", None):
            return item.markdown
    if cells is None:
        return ""
    try:
        from ._tables import finding_markdown, table_to_markdown

        if cells and isinstance(cells[0], Mapping):
            text = finding_markdown(item)
        else:
            text = table_to_markdown(cells)
        # The page assembly owns paragraph separators; a table pushed
        # with its own trailing blank lines would double them.
        return text.rstrip("\n")
    except Exception:
        return ""


def _field(item, name, default=None):
    if isinstance(item, dict):
        return item.get(name, default)
    return getattr(item, name, default)


def _legacy_spans(doc, number, native_page):
    """All spans of one page in draw order, hidden ones included."""
    pdfluent = _native_pdfluent()
    page_spans = getattr(pdfluent, "page_spans", None)
    if callable(page_spans):
        try:
            return list(page_spans(doc, number))
        except Exception:
            pass
    return [span for _, span in _iter_native_spans(native_page.extract_text_blocks())]


def _image_boxes(doc, number):
    pdfluent = _native_pdfluent()
    page_images = getattr(pdfluent, "page_images", None)
    if not callable(page_images):
        return []
    try:
        return [tuple(box) for box in page_images(doc, number)]
    except Exception:
        return []


def _inside_box(x, y, box):
    x0, y0, x1, y1 = box[0], box[1], box[2], box[3]
    return x0 <= x <= x1 and y0 <= y <= y1


def _column_of_x(x, edges):
    for index in range(len(edges) - 1):
        if x < edges[index + 1]:
            return index
    return len(edges) - 2


def _column_edges(xs, gap):
    """Split sorted left edges into columns at wide gaps."""
    ordered = sorted(xs)
    edges = [ordered[0]]
    for prev, value in zip(ordered, ordered[1:]):
        if value - prev > gap:
            edges.append(value)
    edges.append(float("inf"))
    return edges


# Ink widths in ems for crowding detection (Helvetica-like measures).
_WIDTHS = {
    " ": 0.278,
    "!": 0.278,
    "'": 0.222,
    ",": 0.278,
    ".": 0.278,
    ":": 0.278,
    ";": 0.278,
    "I": 0.278,
    "f": 0.278,
    "i": 0.222,
    "j": 0.222,
    "l": 0.222,
    "r": 0.333,
    "t": 0.278,
    "-": 0.333,
    "|": 0.278,
    "m": 0.833,
    "w": 0.722,
    "M": 0.833,
    "W": 0.944,
    "\x00": 0.0,
}
_WIDTH_DEFAULT = 0.556


def _ink_end(x, text, size):
    """Right ink edge of a span placed at x (unspaced, no bordering)."""
    total = 0.0
    for char in text:
        total += _WIDTHS.get(char, _WIDTH_DEFAULT)
    return x + total * size


def _glyph_texts(line, size, use_glyphs):
    """Span texts with glyph-mode boundary spaces inserted.

    Unspecified upstream; our behaviour inserts one space where the
    previous span's ink overflows into the next span's cell, which is
    how a wide glyph on a narrow grid shows up, and nowhere else.
    """
    texts = [_clean_span_text(span.text) for _, span in line]
    if not use_glyphs or len(texts) < 2:
        return texts
    out = [texts[0]]
    for index in range(1, len(texts)):
        prev_x = float(line[index - 1][1].x)
        curr_x = float(line[index][1].x)
        left = texts[index - 1]
        right = texts[index]
        crowded = _ink_end(prev_x, left, size) > curr_x + 0.1 * size
        bordered = left[-1:].isspace() or right[:1].isspace()
        if crowded and not bordered:
            out.append(" " + right)
        else:
            out.append(right)
    return out


def _grouped_line_infos(bucket, page_no, use_glyphs, tolerance, column):
    """One column bucket's spans as line records, top line first."""
    ordered = sorted(bucket, key=lambda span: (-span.y, span.x))
    grouped = []
    for span in ordered:
        if grouped and (grouped[-1][-1].y - span.y) <= tolerance:
            grouped[-1].append(span)
        else:
            grouped.append([span])
    infos = []
    for line in grouped:
        line = sorted(line, key=lambda span: span.x)
        size = max(float(span.font_size) for span in line)
        texts = _glyph_texts([(0, span) for span in line], size, use_glyphs)
        dicts = []
        prev_x = None
        prev_text = None
        for span, text in zip(line, texts):
            entry = _span_dict(0, span, 0, page_no)
            entry["text"] = text
            if dicts and dicts[-1]["flags"] == entry["flags"]:
                # Same-style glyphs concatenate, so a wide gap restores
                # the word space the reference reads: fixtures without
                # ' ' glyphs leave it positional. Past half an em keeps
                # tracked display type joined while table word gaps
                # (ems wide) split; explicit spaces on either side win.
                # The space is its own unstyled dict, so emphasis
                # markers wrap each word ('**Band** **Units**') the way
                # a style boundary would.
                if (
                    prev_x is not None
                    and prev_text is not None
                    and float(span.x) - _ink_end(prev_x, prev_text, size)
                    > 0.5 * size
                    and not dicts[-1]["text"][-1:].isspace()
                    and not entry["text"][:1].isspace()
                ):
                    gap = dict(entry)
                    gap["text"] = " "
                    gap["flags"] = 0
                    dicts.append(gap)
                    dicts.append(entry)
                else:
                    dicts[-1]["text"] += entry["text"]
            else:
                # A style boundary can hide a word space the
                # extractor left positional; without glyph widths
                # the door restores it past a quarter em. This is
                # our behaviour for an unresolvable gap.
                if (
                    dicts
                    and prev_x is not None
                    and float(span.x) - prev_x > 0.25 * size
                    and not dicts[-1]["text"][-1:].isspace()
                    and not entry["text"][:1].isspace()
                ):
                    dicts[-1]["text"] += " "
                dicts.append(entry)
            prev_x = float(span.x)
            prev_text = text
        styled = get_styled_text(dicts)[0].rstrip(" ")
        plain = get_plain_text(dicts).strip()
        if plain == "" and styled == "":
            continue
        infos.append(
            {
                "column": column,
                "y": max(float(span.y) for span in line),
                "x0": min(float(span.x) for span in line),
                "size": size,
                "dicts": dicts,
                "styled": styled,
                "plain": plain,
                "mono": all(int(span.flags) & _MONO_BIT for span in line),
            }
        )
    return infos


def _legacy_line_infos(
    spans, page_no, use_glyphs, tolerance=3.0, column_gap=100.0, table_regions=()
):
    """Group filtered spans into plain/styled line records, column by column.

    Spans inside a table proposal region read row by row across the
    whole region instead: the region is one table bucket, so an unruled
    table dropped by the strategy still renders one joined line per row
    (SPEC G98). Table buckets come first; mixed pages are best effort.
    """
    if not spans:
        return []
    table_spans = []
    prose_spans = []
    for span in spans:
        if any(
            _inside_box(float(span.x), float(span.y), region)
            for region in table_regions
        ):
            table_spans.append(span)
        else:
            prose_spans.append(span)
    infos = []
    if table_spans:
        infos.extend(_grouped_line_infos(table_spans, page_no, use_glyphs, tolerance, -1))
    if prose_spans:
        edges = _column_edges([float(span.x) for span in prose_spans], column_gap)
        buckets = {}
        for span in prose_spans:
            buckets.setdefault(_column_of_x(float(span.x), edges), []).append(span)
        for column in sorted(buckets):
            infos.extend(
                _grouped_line_infos(buckets[column], page_no, use_glyphs, tolerance, column)
            )
    return infos


def _strip_leading_marker(text):
    kind, _, rest = _list_marker(text)
    return rest


def _legacy_progress(numbers, label, show_progress):
    """Yield page numbers, drawing the progress bar when asked.

    Contract: ``_inputs.ProgressBar`` takes the item list and a width
    and iterates it while drawing. The fallback below mirrors the
    recorded bar shape when that lane is missing.
    """
    if not show_progress:
        yield from numbers
        return
    print("Processing %s..." % label, flush=True)
    try:
        from ._inputs import ProgressBar

        items = list(ProgressBar(list(numbers), 40))
    except Exception:
        items = None
    if items is not None:
        yield from items
        return
    total = len(numbers)
    bar_len = 40

    def _bar(done):
        filled = bar_len if total == 0 else int(bar_len * done / total)
        return "[%s%s] (%d/%d)" % ("=" * filled, " " * (bar_len - filled), done, total)

    shown = _bar(0)
    print(shown, end="", flush=True)
    for done, number in enumerate(numbers, start=1):
        next_bar = _bar(done)
        print("\x08" * len(shown) + next_bar, end="", flush=True)
        shown = next_bar
        yield number
    print("", flush=True)


def _image_reference_name(stem, number, index, ext):
    return "%s-%d-%d.%s" % (stem, number, index, ext)


_IMAGE_MIME = {
    "png": "image/png",
    "jpg": "image/jpeg",
    "jpeg": "image/jpeg",
    "gif": "image/gif",
}


def _write_legacy_images(image_path, names, payloads):
    folder = image_path if image_path else "."
    os.makedirs(folder, exist_ok=True)
    for name, payload in zip(names, payloads):
        if payload is None:
            continue
        with open(os.path.join(folder, name), "wb") as handle:
            handle.write(payload)


def legacy_to_markdown(
    doc,
    *,
    pages=None,
    hdr_info=None,
    write_images=False,
    embed_images=False,
    ignore_images=False,
    ignore_graphics=False,
    detect_bg_color=True,
    image_path="",
    image_format="png",
    image_size_limit=0.05,
    filename=None,
    force_text=True,
    page_chunks=False,
    page_separators=False,
    margins=0,
    dpi=150,
    page_width=612,
    page_height=None,
    table_strategy="lines_strict",
    table_output="markdown",
    graphics_limit=None,
    fontsize_limit=3,
    ignore_code=False,
    extract_words=False,
    show_progress=False,
    use_glyphs=False,
    ignore_alpha=False,
    **kwargs,
) -> str:
    """Render a document with the legacy converter (re-exported as to_markdown)."""
    options = dict(kwargs)
    options.update(
        pages=pages,
        hdr_info=hdr_info,
        write_images=write_images,
        embed_images=embed_images,
        ignore_images=ignore_images,
        ignore_graphics=ignore_graphics,
        detect_bg_color=detect_bg_color,
        image_path=image_path,
        image_format=image_format,
        image_size_limit=image_size_limit,
        filename=filename,
        force_text=force_text,
        page_chunks=page_chunks,
        page_separators=page_separators,
        margins=margins,
        dpi=dpi,
        page_width=page_width,
        page_height=page_height,
        table_strategy=table_strategy,
        table_output=table_output,
        graphics_limit=graphics_limit,
        fontsize_limit=fontsize_limit,
        ignore_code=ignore_code,
        extract_words=extract_words,
        show_progress=show_progress,
        use_glyphs=use_glyphs,
        ignore_alpha=ignore_alpha,
    )
    early = check_call("to_markdown", doc, options, layout=False)
    if early is not CONTINUE:
        return early
    if hdr_info is None:
        raise AttributeError("the legacy converter needs an hdr_info detector")
    if not force_text:
        raise ValueError("the legacy converter needs force_text=True")
    if table_strategy not in ("lines_strict", "lines", "text"):
        raise AttributeError("unknown table_strategy: %r" % (table_strategy,))
    if table_output != "markdown":
        raise ValueError("unknown table_output: %r" % (table_output,))
    clip = _normalize_margins(margins)
    label = _label_for(doc)
    if _is_path_like(doc):
        stem = filename or os.path.basename(os.fspath(doc))
    else:
        # Unspecified upstream; our behaviour names image files after
        # the override, else "document", for open documents.
        stem = filename or "document"
    if not stem:
        stem = "document"
    ext = str(image_format).lower() if image_format else "png"

    with _opened_doc(doc) as handle:
        count = handle.page_count
        count = int(count() if callable(count) else count)
        numbers = _legacy_page_numbers(pages, count)
        # Table findings feed both passes (row-major line grouping and
        # rendering); computed once per page since detection walks it.
        findings_cache = {}

        def _cached_findings(number):
            if number not in findings_cache:
                findings_cache[number] = _legacy_table_findings(
                    handle, number, table_strategy
                )
            return findings_cache[number]

        # First pass: item left edges across the document set the levels.
        item_edges = set()
        for number in numbers:
            regions = _cached_findings(number)[2]
            for info in _legacy_infos(
                handle, number, clip, use_glyphs, ignore_alpha, regions
            ):
                stripped = info["plain"].strip() if info["plain"] else ""
                if _ORDERED_ITEM_RE.match(stripped) or startswith_bullet(stripped):
                    item_edges.add(round(info["x0"]))
        levels = {edge: rank for rank, edge in enumerate(sorted(item_edges))}
        if page_chunks:
            from ._chunks import make_page_chunk

            chunks = []
            for number in _legacy_progress(numbers, label, show_progress):
                text = _legacy_page_markdown(
                    handle,
                    number,
                    hdr_info,
                    clip,
                    use_glyphs,
                    ignore_alpha,
                    ignore_code,
                    levels,
                    _cached_findings(number),
                )
                page = handle[number]
                chunks.append(make_page_chunk(handle, page, text, {"text": len(text)}))
            return chunks
        parts = []
        for number in _legacy_progress(numbers, label, show_progress):
            page_text = _legacy_page_markdown(
                handle,
                number,
                hdr_info,
                clip,
                use_glyphs,
                ignore_alpha,
                ignore_code,
                levels,
                _cached_findings(number),
            )
            if not ignore_images and (write_images or embed_images):
                page = handle[number]
                infos = _legacy_image_infos(page, image_size_limit)
                if not infos:
                    infos = [None] * len(_image_boxes(handle, number))
                refs = []
                names = []
                payloads = []
                for index, info in enumerate(infos):
                    name = _image_reference_name(stem, number, index, ext)
                    names.append(name)
                    payload = None
                    if info is not None:
                        for key in ("bytes", "data", "image"):
                            payload = _field(info, key)
                            if payload is not None:
                                break
                    payloads.append(payload)
                    if embed_images and payload is not None:
                        mime = _IMAGE_MIME.get(ext, "image/" + ext)
                        refs.append(
                            "data:%s;base64,%s"
                            % (
                                mime,
                                base64.b64encode(payload).decode("ascii"),
                            )
                        )
                    else:
                        refs.append(
                            name
                            if not image_path
                            else image_path
                            + ("" if image_path.endswith("/") else "/")
                            + name
                        )
                if write_images:
                    _write_legacy_images(image_path, names, payloads)
                if refs:
                    page_text += "\n" + "".join(GRAPHICS_TEXT % ref for ref in refs)
            parts.append((number, page_text))
    if page_separators:
        out = ""
        for number, page_text in parts:
            out += page_text + "\n\n--- end of page=%d ---\n\n" % number
        return out
    paras = []
    for _, page_text in parts:
        paras.append(page_text)
    return "".join(paras)


def _legacy_infos(handle, number, clip, use_glyphs, ignore_alpha, table_regions=()):
    """Filtered line records for one page; shared by both passes."""
    native_page = handle[number]
    width, height = _exact_page_size(native_page)
    spans = _legacy_spans(handle, number, native_page)
    spans = [span for span in spans if not (int(span.flags) & _ROTATED_BIT)]
    if not ignore_alpha:
        visible = [span for span in spans if not (int(span.flags) & _HIDDEN_BIT)]
        spans = visible if visible else spans
    left, top, right, bottom = clip
    if (left, top, right, bottom) != (0, 0, 0, 0):
        spans = [
            span
            for span in spans
            if left <= float(span.x) <= width - right
            and top <= height - float(span.y) <= height - bottom
        ]
    boxes = _image_boxes(handle, number)
    if boxes:
        spans = [
            span
            for span in spans
            if not any(
                _inside_box(float(span.x), float(span.y), box) for box in boxes
            )
        ]
    return _legacy_line_infos(spans, number, use_glyphs, table_regions=table_regions)


def _legacy_page_markdown(
    handle,
    number,
    hdr_info,
    clip,
    use_glyphs,
    ignore_alpha,
    ignore_code,
    levels,
    findings_tuple,
):
    """Render one page's paragraphs with a trailing blank line."""
    native_page = handle[number]
    findings, shaded_boxes, regions = findings_tuple
    infos = _legacy_infos(handle, number, clip, use_glyphs, ignore_alpha, regions)
    tables = []
    for item in findings:
        box = _field(item, "bbox")
        text = _table_markdown(item)
        if box is None or not text:
            continue
        try:
            tables.append((_rect4(box), text))
        except (TypeError, AttributeError):
            continue
    # Prose standing on a table -- rendered or shaded away -- is table
    # text, not paragraphs.
    exclusion = [box for box, _ in tables]
    for shaded in shaded_boxes:
        try:
            exclusion.append(_rect4(shaded))
        except (TypeError, AttributeError):
            continue
    if exclusion:
        kept = []
        for info in infos:
            if any(_inside_box(info["x0"], info["y"], box) for box in exclusion):
                continue
            kept.append(info)
        infos = kept
    tables.sort(key=lambda entry: -entry[0][1])
    prose_left = min(
        [info["x0"] for info in infos if not info["mono"]]
        or [info["x0"] for info in infos]
        or [50.0]
    )
    paras = []
    pending_sep = "\n\n\n"
    fence = []

    def _push(para):
        nonlocal pending_sep
        if paras:
            paras.append(pending_sep)
        paras.append(para)
        pending_sep = "\n\n\n"

    current = []
    previous = None
    previous_is_header = False
    previous_is_item = False
    previous_is_fence = False
    ordered_count = 0
    table_pos = 0

    def _flush_current():
        if current:
            _push("\n\n".join(current))
            del current[:]

    def _break_before(rec):
        # A paragraph breaks past two ems of the smaller size; a
        # header glues to the prose below it, while an item after a
        # non-item always opens a new paragraph. A fenced member
        # glues to its neighbours on both sides.
        nonlocal pending_sep
        if previous is None:
            return
        if rec["column"] != previous["column"]:
            _flush_current()
            # Column breaks carry one extra blank line.
            pending_sep = "\n\n\n\n"
            return
        if rec["y"] > previous["y"]:
            _flush_current()
            return
        if rec.get("is_fence") or previous_is_fence:
            return
        if rec["is_item"] and not previous_is_item:
            _flush_current()
            return
        # Gap noise below a micropoint never breaks a paragraph.
        gap = round(previous["y"] - rec["y"], 3)
        if gap > 2.0 * min(previous["size"], rec["size"]):
            if previous_is_header and not rec["is_item"]:
                return
            _flush_current()

    def _close_fence():
        nonlocal previous, previous_is_header, previous_is_item, previous_is_fence
        if not fence:
            return
        gaps = [fence[pos]["y"] - fence[pos + 1]["y"] for pos in range(len(fence) - 1)]
        step = sorted(gaps)[len(gaps) // 2] if gaps else 0
        body = []
        for pos, info in enumerate(fence):
            if pos > 0 and step > 0:
                blanks = max(0, round(gaps[pos - 1] / step) - 1)
                body.extend([""] * blanks)
            indent = max(0, int(round((info["x0"] - prose_left) / 10.0)))
            body.append(" " * indent + info["plain"])
        first = fence[0]
        last = fence[-1]
        _break_before(
            {
                "y": first["y"],
                "size": first["size"],
                "column": first["column"],
                "is_item": False,
                "is_fence": True,
            }
        )
        current.append("```\n" + "\n".join(body) + "\n\n```")
        previous = {"y": last["y"], "size": last["size"], "column": last["column"]}
        previous_is_header = False
        previous_is_item = False
        previous_is_fence = True
        del fence[:]

    for info in infos:
        while table_pos < len(tables) and tables[table_pos][0][1] > info["y"]:
            _flush_current()
            _close_fence()
            _flush_current()
            _push(tables[table_pos][1])
            table_pos += 1
        if info["mono"] and not ignore_code:
            fence.append(info)
            continue
        _close_fence()
        is_header = False
        is_item_line = False
        if info["mono"] and ignore_code:
            text = "`" + info["plain"] + "`"
        else:
            header = hdr_info.get_header_id(
                {"text": info["plain"], "size": info["size"], "page": number},
                number,
            )
            if header:
                try:
                    level = max(1, min(6, int(str(header)[1:])))
                except (ValueError, IndexError):
                    level = 1
                ordered_count = 0
                is_header = True
                text = "#" * level + " " + info["styled"]
            else:
                stripped = info["plain"].strip()
                ordered = _ORDERED_ITEM_RE.match(stripped)
                if ordered and levels is not None:
                    ordered_count += 1
                    is_item_line = True
                    depth = levels.get(round(info["x0"]), 0)
                    pad = "    " * depth
                    text = (
                        pad
                        + "%d. " % ordered_count
                        + _strip_leading_marker(info["styled"])
                    )
                elif startswith_bullet(stripped) and levels is not None:
                    ordered_count = 0
                    is_item_line = True
                    depth = levels.get(round(info["x0"]), 0)
                    pad = " " if depth == 0 else "    " * depth
                    text = pad + "- " + _strip_leading_marker(info["styled"])
                else:
                    ordered_count = 0
                    text = info["styled"]
        _break_before(
            {
                "y": info["y"],
                "size": info["size"],
                "column": info["column"],
                "is_item": is_item_line,
            }
        )
        current.append(text)
        previous = info
        previous_is_header = is_header
        previous_is_item = is_item_line
        previous_is_fence = False
    while table_pos < len(tables):
        _flush_current()
        _close_fence()
        _flush_current()
        _push(tables[table_pos][1])
        table_pos += 1
    _close_fence()
    _flush_current()
    if not paras:
        return ""
    return "".join(paras) + "\n\n\n"


def legacy_to_text(*args, **kwargs):
    """Legacy plain-text converter (re-exported as to_text; always refuses)."""
    raise NotImplementedError("not available in this build")


def legacy_to_json(*args, **kwargs):
    """Legacy JSON converter (re-exported as to_json; always refuses)."""
    raise NotImplementedError("not available in this build")


def _layout_convert(doc, kwargs):
    """Run the layout pipeline through the chunks lane (contract assumption).

    Contract: ``_chunks.parse_document`` accepts the recorded parse
    options and returns a parsed document whose ``to_markdown`` accepts
    the recorded render options. Unknown options are dropped so stray
    keywords stay tolerated.
    """
    from ._chunks import parse_document

    parse_kwargs = {key: kwargs[key] for key in _LAYOUT_PARSE_KEYS if key in kwargs}
    if "filename" not in parse_kwargs:
        if isinstance(doc, str):
            parse_kwargs["filename"] = doc
        elif _is_path_like(doc):
            parse_kwargs["filename"] = os.fspath(doc)
        else:
            parse_kwargs["filename"] = ""
    render_kwargs = {key: kwargs[key] for key in _LAYOUT_RENDER_KEYS if key in kwargs}
    render_kwargs.setdefault("header", True)
    render_kwargs.setdefault("footer", True)
    parsed = parse_document(doc, **parse_kwargs)
    return parsed.to_markdown(**render_kwargs)


def to_markdown(*args, **kwargs):
    """Render a document as markdown, or per-page chunks on request."""
    doc = args[0] if args else kwargs.get("doc")
    early = check_call("to_markdown", doc, dict(kwargs), layout=is_layout_mode())
    if early is not CONTINUE:
        return early
    if is_layout_mode():
        return _layout_convert(doc, kwargs)
    options = dict(kwargs)
    options.pop("doc", None)
    if options.get("hdr_info") is None:
        options["hdr_info"] = IdentifyHeaders(doc)
    return legacy_to_markdown(doc, **options)
