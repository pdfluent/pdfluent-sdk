# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L3: SPEC sections 6, 14, 22.
"""Page chunks, JSON output and form fields.

This module owns the layout parse (`parse_document`), the JSON shape
(`to_json`), the per-page chunk records (`make_page_chunk` plus the legacy
assembly), the key/value form views and the word-binding adapter. Box
rendering, table plumbing beyond the record, image bytes and OCR machinery
belong to sibling lanes; they are imported lazily at call time so this module
imports standalone. Geometry throughout is y-down, origin top-left, matching
the recorded door outputs; every float crossing into an output is rounded to
32-bit like the recorder's own numbers.
"""
from __future__ import annotations

import base64
import functools
import json
import os
import re
import shutil
import string
import struct
import subprocess
import sys

from ._errors import CONTINUE, check_call
from ._mode import is_layout_mode

try:
    from ._inputs import OCRMode
except (ImportError, AttributeError):  # pragma: no cover - standalone import
    import enum

    class OCRMode(enum.IntEnum):
        """Standalone fallback until the inputs lane arrives at merge."""

        NEVER = 0
        SELECT_DROP_OLD = 1
        SELECT_KEEP_OLD = 2
        FORCE_DROP_OLD = 3
        FORCE_KEEP_OLD = 4


def _f32(value):
    """Round one float the way the recorded door numbers are stored."""
    return struct.unpack("f", struct.pack("f", value))[0]


def _f32_list(values):
    """Round a coordinate list to 32-bit and return it as a list."""
    return [_f32(v) for v in values]


def _union(first, second):
    """Bounding union of two [x0, y0, x1, y1] boxes."""
    return [
        min(first[0], second[0]),
        min(first[1], second[1]),
        max(first[2], second[2]),
        max(first[3], second[3]),
    ]


def _unions(boxes):
    """Bounding union of a non-empty box list."""
    out = list(boxes[0])
    for box in boxes[1:]:
        out = _union(out, box)
    return out


def _strict_overlap(first, second):
    """True when two boxes share interior area (edge touches miss)."""
    return min(first[2], second[2]) > max(first[0], second[0]) and min(
        first[3], second[3]
    ) > max(first[1], second[1])


def _y_overlap(first, second):
    """True when two boxes share interior vertical range."""
    return min(first[3], second[3]) > max(first[1], second[1])


def _contains_x0(box, x):
    """True when the box spans the x coordinate (edges count)."""
    return box[0] <= x <= box[2]


def _fill_coverage(fill, box):
    """Fraction of the box area covered by the fill rectangle."""
    width = max(0.0, min(fill[2], box[2]) - max(fill[0], box[0]))
    tall = max(0.0, min(fill[3], box[3]) - max(fill[1], box[1]))
    area = max(0.0, box[2] - box[0]) * max(0.0, box[3] - box[1])
    if area <= 0:
        return 0.0
    return (width * tall) / area


class LayoutBox:
    """One classified rectangle on a parsed page."""

    def __init__(
        self,
        x0,
        y0,
        x1,
        y1,
        boxclass,
        image=None,
        table=None,
        max_fontsize=None,
        header_level=0,
        textlines=None,
    ):
        self.x0 = x0
        self.y0 = y0
        self.x1 = x1
        self.y1 = y1
        self.boxclass = boxclass
        self.image = image
        self.table = table
        self.max_fontsize = max_fontsize
        self.header_level = header_level
        self.textlines = textlines


class PageLayout:
    """Everything the layout pass kept for one page."""

    def __init__(
        self,
        page_number,
        width,
        height,
        boxes,
        full_ocred=False,
        fulltext=None,
        words=None,
        links=None,
    ):
        self.page_number = page_number
        self.width = width
        self.height = height
        self.boxes = boxes
        self.full_ocred = full_ocred
        self.fulltext = fulltext
        self.words = words
        self.links = links


class ParsedDocument:
    """A document after the layout pass, ready to render."""

    def __init__(
        self,
        filename=None,
        page_count=None,
        toc=None,
        pages=None,
        metadata=None,
        from_bytes=False,
        image_dpi=150,
        image_format="png",
        image_path="",
        use_ocr=OCRMode.SELECT_KEEP_OLD,
    ):
        self.filename = filename
        self.page_count = page_count
        self.toc = toc
        self.pages = pages
        self.metadata = metadata
        self.from_bytes = from_bytes
        self.image_dpi = image_dpi
        self.image_format = image_format
        self.image_path = image_path
        self.use_ocr = use_ocr
        self.force_text = False
        self.embed_images = False
        self.write_images = False
        self.form_fields = {}

    def to_markdown(
        self,
        header=True,
        footer=True,
        write_images=False,
        embed_images=False,
        ignore_code=False,
        show_progress=False,
        page_separators=False,
        page_chunks=False,
        **kwargs,
    ):
        """Render the parsed document as markdown, or as page chunks."""
        return _render_document(
            self,
            "md",
            header=header,
            footer=footer,
            write_images=write_images,
            embed_images=embed_images,
            ignore_code=ignore_code,
            page_separators=page_separators,
            page_chunks=page_chunks,
            options=kwargs,
        )

    def to_text(
        self,
        header=True,
        footer=True,
        write_images=False,
        embed_images=False,
        ignore_code=False,
        show_progress=False,
        page_separators=False,
        page_chunks=False,
        **kwargs,
    ):
        """Render the parsed document as plain text, or as page chunks."""
        return _render_document(
            self,
            "text",
            header=header,
            footer=footer,
            write_images=write_images,
            embed_images=embed_images,
            ignore_code=ignore_code,
            page_separators=page_separators,
            page_chunks=page_chunks,
            options=kwargs,
        )

    def to_json(self, show_progress=False):
        """Serialise the parsed document to its JSON string."""
        del show_progress  # Parsing already happened; nothing to report.
        document = {
            "filename": self.filename,
            "page_count": self.page_count,
            "toc": self.toc or [],
            "pages": [_page_to_json(page) for page in self.pages or []],
            "metadata": self.metadata or {},
            "from_bytes": self.from_bytes,
            "image_dpi": self.image_dpi,
            "image_format": self.image_format,
            "image_path": self.image_path,
            "use_ocr": self.use_ocr,
            "form_fields": self.form_fields or {},
            "force_text": self.force_text,
            "embed_images": self.embed_images,
            "write_images": self.write_images,
        }
        return json.dumps(document)


class TableDetails:
    """Grid, texts and markdown of one detected table."""

    def __init__(
        self,
        bbox=None,
        row_count=None,
        col_count=None,
        cells=None,
        extract=None,
        markdown=None,
    ):
        self.bbox = bbox
        self.row_count = row_count
        self.col_count = col_count
        self.cells = cells
        self.extract = extract
        self.markdown = markdown


def _page_to_json(page):
    """Serialise one page layout to its JSON-ready mapping."""
    boxes = []
    for box in page.boxes or []:
        table = box.table
        boxes.append(
            {
                "x0": box.x0,
                "y0": box.y0,
                "x1": box.x1,
                "y1": box.y1,
                "boxclass": box.boxclass,
                "image": box.image,
                "table": None
                if table is None
                else {
                    "bbox": table.bbox,
                    "row_count": table.row_count,
                    "col_count": table.col_count,
                    "cells": table.cells,
                    "extract": table.extract,
                    "markdown": table.markdown,
                },
                "max_fontsize": box.max_fontsize,
                "header_level": box.header_level,
                "textlines": box.textlines,
            }
        )
    return {
        "page_number": page.page_number,
        "width": page.width,
        "height": page.height,
        "boxes": boxes,
        "full_ocred": page.full_ocred,
        "fulltext": page.fulltext if page.fulltext is not None else [],
        "words": page.words if page.words is not None else [],
        "links": page.links if page.links is not None else [],
    }


def make_page_chunk(doc, page, text, string_lengths):
    """Build one layout page-chunk record from a rendered page.

    `string_lengths` carries each box's markdown length in box order; the
    running totals become the `pos` offsets into `text`. Only the documented
    pipeline path is specified; direct calls with mismatched shapes fail the
    way plain Python fails (length checks, attribute access, an assertion on
    the box count).
    """
    count = len(string_lengths)
    assert count == len(page.boxes)
    metadata = dict(doc.metadata or {})
    metadata["file_path"] = doc.filename
    metadata["page_count"] = doc.page_count
    metadata["page_number"] = page.page_number
    toc_items = [list(row) for row in doc.toc or [] if row[2] == page.page_number]
    boxes = []
    offset = 0
    for index, box in enumerate(page.boxes):
        length = string_lengths[index]
        boxes.append(
            {
                "index": index,
                "class": box.boxclass,
                "bbox": [box.x0, box.y0, box.x1, box.y1],
                "pos": [offset, offset + length],
            }
        )
        offset += length
    return {
        "metadata": metadata,
        "toc_items": toc_items,
        "page_boxes": boxes,
        "text": text,
    }


def _native_module():
    """The native engine module, imported late so this file stands alone."""
    import pdfluent

    return pdfluent


def _exact_page_size(page):
    """Exact page size; the native width/height round through f32."""
    return _native_module().door_page_size(page)


def _open_doc(doc):
    """Open a path or reuse a native document; returns (doc, display path).

    Byte strings are refused outright and `None` raises IndexError, keeping
    the pipeline errors identical to the top-level entries' own.
    """
    if doc is None:
        raise IndexError("no document was given")
    if isinstance(doc, (bytes, bytearray)):
        raise TypeError("bytes input is not supported: pass a file path or Document")
    if isinstance(doc, (str, os.PathLike)):
        display = doc if isinstance(doc, str) else str(doc)
        return _native_module().Document(display), display
    if hasattr(doc, "page_count"):
        return doc, None
    raise TypeError("expected a file path or Document, got %r" % type(doc).__name__)


def _normalize_pages(pages, count):
    """Normalise the layout `pages` option to a sorted, deduplicated list.

    Indices count from 0 with negatives from the end. An integer is refused
    (only the top-level entry takes one); anything else outside a list or
    tuple, and any index outside the document, is a ValueError.
    """
    if pages is None:
        return list(range(count))
    if isinstance(pages, bool) or isinstance(pages, int):
        raise TypeError("pages takes a list of indices, not a single integer")
    if isinstance(pages, str) or not isinstance(pages, (list, tuple)):
        raise ValueError("pages takes a list of indices")
    out = []
    for entry in pages:
        if isinstance(entry, float):
            if not entry.is_integer():
                raise ValueError("page indices must be integers")
            entry = int(entry)
        elif isinstance(entry, bool) or not isinstance(entry, int):
            raise ValueError("page indices must be integers")
        if entry < 0:
            entry += count
        out.append(entry)
    out = sorted(set(out))
    for entry in out:
        if entry < 0 or entry >= count:
            raise ValueError("page %d out of range" % entry)
    return out


def _normalize_legacy_pages(pages, count):
    """Normalise the legacy `pages` option, keeping order and repeats.

    Lists, tuples, ranges and other index sequences are honoured as given;
    negatives resolve from the end and out-of-range indices fail naturally
    on access. An integer is a TypeError, a string an AssertionError.
    Unspecified upstream; our behaviour for unlisted sequence types.
    """
    if pages is None:
        return list(range(count))
    if isinstance(pages, bool) or isinstance(pages, int):
        raise TypeError("pages takes a list of indices, not a single integer")
    assert not isinstance(
        pages, (str, bytes, bytearray)
    ), "pages takes a list of indices"
    out = []
    for entry in pages:
        if isinstance(entry, float) and entry.is_integer():
            entry = int(entry)
        if isinstance(entry, bool) or not isinstance(entry, int):
            raise ValueError("page indices must be integers")
        if entry < 0:
            entry += count
        out.append(entry)
    return out


def _stored_use_ocr(use_ocr):
    """Normalise a use_ocr option to its stored JSON value.

    True stays True (the top-level default); modes and integers keep their
    number; anything else falls back to True. Unspecified upstream; our
    behaviour for the tolerated odd values.
    """
    if use_ocr is True:
        return True
    try:
        return int(use_ocr)
    except (TypeError, ValueError):
        return True


def _ocr_trigger_mode(stored):
    """Reduce a stored use_ocr value to never/select/force for printing."""
    if stored is True:
        return "select"
    if stored == 0:
        return "never"
    if stored in (3, 4):
        return "force"
    return "select"


def _tesseract_binary():
    """The recogniser binary: the override when set, else tesseract off PATH."""
    override = os.environ.get("PDFLUENT_TESSERACT", "")
    if override:
        return override
    return shutil.which("tesseract")


def _tesseract_version():
    """The recogniser version (5.5.3), or None where no binary answers."""
    binary = _tesseract_binary()
    if not binary:
        return None
    try:
        out = subprocess.run(
            [binary, "--version"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, ValueError):
        return None
    if out.returncode != 0:
        return None
    words = (out.stdout or "").split()
    if len(words) >= 2 and words[0] == "tesseract":
        return words[1]
    return None


#: Sentinel for a recogniser version not probed yet.
_UNPROBED = object()


def _tesseract_banner_line(version=_UNPROBED):
    """The parser-messages OCR line, versioned where a binary answers."""
    if version is _UNPROBED:
        version = _tesseract_version()
    if version is None:
        return "Using Tesseract for OCR processing.\n"
    return "Using Tesseract %s for OCR processing.\n" % version


def _page_has_text(layout):
    """Whether the layout already carries readable text (OCR adds none)."""
    for box in layout.boxes or []:
        if box.boxclass == "table" and box.table is not None:
            return True
        for line in box.textlines or []:
            if isinstance(line, dict):
                spans = line.get("spans", [])
            else:
                spans = getattr(line, "spans", [])
            for span in spans:
                if isinstance(span, dict):
                    text = span.get("text", "")
                else:
                    text = getattr(span, "text", "")
                if (text or "").strip():
                    return True
    return False


def _recognize_paragraphs(native, index, forced, ocr_language, ocr_dpi):
    """One page's recognised paragraphs through the native OCR path.

    Recognition failures (unknown language, unusable dpi, no engine)
    read as no text: selection stays silent and force keeps its print
    without injecting anything.
    """
    try:
        language = ocr_language if isinstance(ocr_language, str) else "eng"
        try:
            dpi = int(ocr_dpi)
        except (TypeError, ValueError):
            dpi = 150
        if dpi <= 0:
            dpi = 150
        out = native.extract_markdown(
            ocr="force" if forced else "auto",
            pages=[index],
            ocr_language=language,
            ocr_dpi=dpi,
        )
    except Exception:
        return []
    paras = []
    for chunk in out.split("\n\n"):
        text = chunk.strip()
        if not text or text.startswith("!["):
            continue
        paras.append(" ".join(text.split()))
    return paras


def _inject_ocr_paragraphs(layout, paras):
    """Append recognised paragraphs as text boxes to a textless layout."""
    from ._inputs import TESSERACT_FONT_NAME

    width = layout.width
    height = layout.height
    box = [0.0, 0.0, width, height]
    boxes = layout.boxes or []
    for number, para in enumerate(paras):
        span = {
            "size": 10.0,
            "flags": 0,
            "bidi": 0,
            "char_flags": 0,
            "font": TESSERACT_FONT_NAME,
            "color": 0,
            "alpha": 255,
            "ascender": 0.0,
            "descender": 0.0,
            "text": para,
            "origin": [0.0, 0.0],
            "bbox": list(box),
            "line": 0,
            "block": number,
            "dir": [1.0, 0.0],
        }
        boxes.append(
            LayoutBox(
                box[0],
                box[1],
                box[2],
                box[3],
                "text",
                textlines=[{"bbox": list(box), "spans": [span]}],
            )
        )
    layout.boxes = boxes


def _iter_page_spans(pairs):
    """Yield every span dict of raw-line pairs in order."""
    for _bbox, spans in pairs:
        for span in spans or []:
            yield span


def _normalise_span(span):
    """Round one span's floats to 32-bit and list its coordinates."""
    out = dict(span)
    if out.get("bbox") is not None:
        out["bbox"] = _f32_list(out["bbox"])
    if out.get("origin") is not None:
        out["origin"] = _f32_list(out["origin"])
    for key in ("size", "ascender", "descender"):
        if out.get(key) is not None:
            out[key] = _f32(out[key])
    direction = out.get("dir")
    if direction is not None:
        out["dir"] = _f32_list(direction)
    return out


def _line_is_horizontal(spans):
    """True when at least one span runs along +x (missing dirs pass)."""
    for span in spans:
        direction = span.get("dir", [1.0, 0.0])
        if direction is None:
            continue
        if list(direction) == [1.0, 0.0]:
            return True
    return False


def _line_direction(spans):
    """Fulltext line direction: +x, else -y.

    Unspecified upstream; our behaviour: any horizontal span keeps the line
    horizontal, otherwise it reads top-down like the recorded sideways line.
    """
    if _line_is_horizontal(spans):
        return [1.0, 0.0]
    return [0.0, -1.0]


def _visible_spans(pairs):
    """Split raw-line spans into visible ones and a hidden flag.

    Zero-alpha spans are hidden; a page without any visible span keeps them
    all instead of going empty.
    """
    seen = [span for span in _iter_page_spans(pairs)]
    visible = [span for span in seen if span.get("alpha", 255) != 0]
    hidden = [span for span in seen if span.get("alpha", 255) == 0]
    if visible:
        return visible, True
    return seen, False


def _normalise_pairs(pairs):
    """Normalise every span of raw-line pairs once, keeping pair order."""
    return [
        (bbox, [_normalise_span(span) for span in spans or []]) for bbox, spans in pairs
    ]


def _fulltext_blocks(pairs):
    """Group normalised raw-line pairs into blocks in first-seen order."""
    blocks = []
    index_of = {}
    for _bbox, spans in pairs:
        if not spans:
            continue
        key = spans[0].get("block", 0)
        if key not in index_of:
            index_of[key] = len(blocks)
            blocks.append([])
        blocks[index_of[key]].append(spans)
    return blocks


def _connected_groups(rects):
    """Union-find rects that touch or overlap into groups (exact edges)."""
    parent = list(range(len(rects)))

    def find(item):
        while parent[item] != item:
            parent[item] = parent[parent[item]]
            item = parent[item]
        return item

    def touches(first, second):
        return min(first[2], second[2]) >= max(first[0], second[0]) and min(
            first[3], second[3]
        ) >= max(first[1], second[1])

    for left in range(len(rects)):
        for right in range(left + 1, len(rects)):
            if touches(rects[left], rects[right]):
                parent[find(left)] = find(right)
    groups = {}
    for position, rect in enumerate(rects):
        groups.setdefault(find(position), []).append(rect)
    return [_unions(boxes) for boxes in groups.values()]


def _number_fulltext(blocks, image_tops, group_tops):
    """Assign fulltext numbers: sequence plus drawings above each block.

    Numbering counts sequence order plus every raster image and vector group
    starting above the block. Unspecified upstream; our behaviour for the
    vector half, which only untested fixtures distinguish.
    """
    numbers = []
    for position, lines in enumerate(blocks):
        top = min(min(span["bbox"][1] for span in line) for line in lines)
        above = sum(1 for y in image_tops if y < top)
        above += sum(1 for y in group_tops if y < top)
        numbers.append(position + above)
    return numbers


def _number_images(blocks, images):
    """Assign legacy image numbers: images so far plus text blocks above.

    Mirrors `_number_fulltext` with the roles swapped: the image at
    drawings-order position counts the images before it plus every fulltext
    text block starting strictly above its own top edge.
    """
    tops = [
        min(min(span["bbox"][1] for span in line) for line in block)
        for block in blocks
    ]
    return [
        position + sum(1 for top in tops if top < _f32(image["bbox"][1]))
        for position, image in enumerate(images)
    ]


def parse_document(
    doc,
    filename="",
    image_dpi=150,
    ocr_dpi=150,
    image_format="png",
    image_path="",
    pages=None,
    show_progress=False,
    embed_images=False,
    write_images=False,
    force_text=False,
    use_ocr=OCRMode.SELECT_KEEP_OLD,
    force_ocr=False,
    ocr_language="eng",
    ocr_function=None,
    render_html_tables=None,
    edge_threshold=None,
):
    """Parse a document into boxes, fulltext, tables and links per page."""
    del render_html_tables, edge_threshold
    # The pages *type* is refused before the document opens: on an
    # unreadable file the open error would otherwise pre-empt it, and the
    # refusal must not depend on whether the bytes parse. Same wording as
    # _normalize_pages, which still owns the per-index checks below.
    if isinstance(pages, bool) or isinstance(pages, int):
        raise TypeError("pages takes a list of indices, not a single integer")
    if pages is not None and (
        isinstance(pages, str) or not isinstance(pages, (list, tuple))
    ):
        raise ValueError("pages takes a list of indices")
    native, display = _open_doc(doc)
    count = native.page_count
    indices = _normalize_pages(pages, count)
    stored_ocr = _stored_use_ocr(use_ocr)
    if display is not None:
        shown = display
    else:
        shown = getattr(native, "name", None) or filename or ""
    if show_progress:
        sys.stdout.write("Parsing %d pages of '%s'...\n" % (len(indices), shown))
        from ._inputs import ProgressBar

        iterator = ProgressBar(list(indices))
    else:
        iterator = list(indices)
    file_meta = _native_module().door_file_metadata(native)
    toc = [list(row) for row in _native_module().door_toc(native)]
    form_fields = _form_fields(native, False)
    parsed_pages = []
    ocr_pages = []
    mode = _ocr_trigger_mode(stored_ocr)
    if mode != "never" or force_ocr:
        recognizer = _tesseract_version()
    else:
        recognizer = None
    for position, index in enumerate(iterator):
        page = _parse_page(
            native,
            index,
            image_format=image_format,
            image_path=image_path,
            stem=filename or (os.path.basename(shown) if shown else "document"),
            embed_images=embed_images,
            write_images=write_images,
        )
        parsed_pages.append(page)
        visible = any(
            span.get("alpha", 255) != 0 for span in _iter_page_spans(page["_pairs"])
        )
        hidden_words = any(
            span.get("alpha", 255) == 0 and (span.get("text") or "").strip()
            for span in _iter_page_spans(page["_pairs"])
        )
        forced = bool(force_ocr) or mode == "force"
        selected = mode == "select"
        # Selection keys on hidden text spans. Unspecified upstream; our
        # behaviour for the unobserved trigger mechanism.
        attempt = (forced and not visible) or (
            selected and not visible and hidden_words
        )
        # A textless raster page under selection wants recognition too;
        # whether it needs the print is decided by what recognition reads.
        boxes = page["layout"].boxes or []
        has_images = any(
            box.boxclass == "picture" and getattr(box, "_raster", False)
            for box in boxes
        )
        recognize = (
            selected
            and not visible
            and not hidden_words
            and has_images
            and recognizer is not None
        )
        if attempt or recognize:
            if ocr_function is not None:
                ocr_function(native[index])
                ocr_pages.append(position)
            elif recognizer is None or _page_has_text(page["layout"]):
                # No recogniser, or the page already reads: print-only.
                ocr_pages.append(position)
            else:
                paras = _recognize_paragraphs(
                    native, index, forced, ocr_language, ocr_dpi
                )
                if paras:
                    _inject_ocr_paragraphs(page["layout"], paras)
                # Force records every attempt; selection only the pages
                # where recognition read text, so blank scans stay quiet.
                if attempt or paras:
                    ocr_pages.append(position)
        del page["_pairs"]
    if mode != "never" or force_ocr:
        sys.stdout.write("\n=== Document parser messages ===\n")
        sys.stdout.write(_tesseract_banner_line(recognizer))
        for position in ocr_pages:
            sys.stdout.write("OCR on page.number=%d/%d.\n" % (position, len(indices)))
    parsed = ParsedDocument(
        filename=shown,
        page_count=count,
        toc=toc,
        pages=[page["layout"] for page in parsed_pages],
        metadata=file_meta,
        from_bytes=False,
        image_dpi=image_dpi,
        image_format=image_format,
        image_path=image_path,
        use_ocr=stored_ocr,
    )
    parsed.force_text = force_text
    parsed.embed_images = embed_images
    parsed.write_images = write_images
    parsed.form_fields = form_fields
    _assign_header_levels(parsed)
    return parsed


def _parse_page(
    native, index, image_format, image_path, stem, embed_images, write_images
):
    """Build one page layout plus its raw pairs for OCR bookkeeping."""
    from . import _markdown as siblings

    module = _native_module()
    page_handle = native[index]
    width = _f32(page_handle.width)
    height = _f32(page_handle.height)
    pairs = _normalise_pairs(
        siblings.get_raw_lines(
            textpage=page_handle, ignore_invisible=False, only_horizontal=False
        )
    )
    drawings = module.door_page_drawings(native, index)
    proposals = module.door_page_tables(native, index)
    links = module.door_page_links(native, index)
    for link in links:
        link["from"] = _f32_list(link["from"])
    images = drawings["images"]
    for image in images:
        image["bbox"] = _f32_list(image["bbox"])
        image["transform"] = _f32_list(image["transform"])
    rulings = [_f32_list(line) for line in drawings["rulings"]]
    fills = [_f32_list(area) for area in drawings["fills"]]
    clusters = [_f32_list(box) for box in drawings["graphics"]]
    blocks = _fulltext_blocks(pairs)
    image_tops = sorted(image["bbox"][1] for image in images)
    group_tops = sorted(group[1] for group in _connected_groups(rulings + fills))
    numbers = _number_fulltext(blocks, image_tops, group_tops)
    fulltext = []
    for number, lines in zip(numbers, blocks):
        entries = []
        for spans in lines:
            entries.append(
                {
                    "bbox": _unions([span["bbox"] for span in spans]),
                    "dir": _line_direction(spans),
                    "spans": spans,
                    "wmode": 0,
                }
            )
        fulltext.append(
            {
                "type": 0,
                "number": number,
                "flags": 0,
                "bbox": _unions([entry["bbox"] for entry in entries]),
                "lines": entries,
            }
        )
    visible, _has_visible = _visible_spans(pairs)
    visible_ids = set(map(id, visible))
    table_boxes, consumed, skipped = _structure_tables(
        native, index, proposals, rulings, fills, visible, siblings
    )
    rows, carried, leftover = _collect_rows(
        pairs, visible_ids, consumed, rulings, fills, siblings, skipped
    )
    boxes = _merge_rows(rows)
    kept_images = [
        image
        for image in images
        if not any(_strict_overlap(image["bbox"], row["bbox"]) for row in rows)
    ]
    table_areas = [
        [box["box"].x0, box["box"].y0, box["box"].x1, box["box"].y1]
        for box in table_boxes
    ] + skipped
    kept_clusters = [
        cluster
        for cluster in clusters
        if not any(_strict_overlap(cluster, area) for area in table_areas)
    ]
    ordered = _order_boxes(
        boxes, table_boxes, kept_images, kept_clusters, carried, leftover
    )
    classified = _classify_boxes(ordered, width, height, pairs, visible, siblings)
    if embed_images or write_images:
        _attach_images(
            classified,
            kept_images,
            native,
            index,
            stem=stem,
            image_format=image_format,
            image_path=image_path,
            embed_images=embed_images,
            write_images=write_images,
        )
    layout = PageLayout(
        page_number=index + 1,
        width=width,
        height=height,
        boxes=classified,
        full_ocred=False,
        fulltext=fulltext,
        words=[],
        links=links,
    )
    return {"layout": layout, "_pairs": pairs}


def _rects_touch(first, second):
    """True when two boxes overlap or share an edge."""
    return min(first[2], second[2]) >= max(first[0], second[0]) and min(
        first[3], second[3]
    ) >= max(first[1], second[1])


def _band_index(edges, value):
    """Grid band holding a coordinate: rightmost edge at or below it."""
    if value < edges[0] or value > edges[-1]:
        return None
    for position in range(len(edges) - 2, -1, -1):
        if edges[position] <= value:
            return position
    return None


def _ruling_grid(rulings, region):
    """Cell edges from rulings crossing the region, or None without a grid.

    The proposal region runs tighter than the drawn grid (engine cells
    end where the border rulings begin), so the region's own edges
    close the grid once inner rulings exist on both axes; without
    inner rulings there is no ruled grid and the text grid decides.
    """
    inner_xs = sorted(
        {
            line[0]
            for line in rulings
            if line[0] == line[2] and _rects_touch(line, region)
        }
    )
    inner_ys = sorted(
        {
            line[1]
            for line in rulings
            if line[1] == line[3] and _rects_touch(line, region)
        }
    )
    if not inner_xs or not inner_ys:
        return None
    xs = sorted(set([region[0], region[2]] + inner_xs))
    ys = sorted(set([region[1], region[3]] + inner_ys))
    return xs, ys


def _span_fallback_words(spans, region):
    """Word boxes guessed by slicing spans evenly per word.

    Only used when the word binding is absent; with it, real word boxes
    drive the unruled grid. Unspecified upstream; our behaviour.
    """
    out = []
    for span in spans:
        box = span["bbox"]
        if not _rects_touch(box, region):
            continue
        parts = (span.get("text") or "").split()
        if not parts:
            continue
        total = sum(len(part) for part in parts)
        cursor = box[0]
        for part in parts:
            share = (box[2] - box[0]) * len(part) / total if total else 0
            out.append([cursor, box[1], cursor + share, box[3], part])
            cursor += share
    return out


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


def _alignment_grid(native, index, region, visible):
    """Cell edges from word columns and row bands, or None when too thin.

    Unspecified upstream; our behaviour: column edges sit midway between
    neighbouring word columns, row edges midway between row bands.
    """
    try:
        words = [
            word[:5]
            for word in _words_for_page(native, index)
            if _rects_touch(word[:4], region)
        ]
    except NotImplementedError:
        words = _span_fallback_words(visible, region)
    if not words:
        return None
    bands = _overlap_groups(words, lambda word: (word[1], word[3]))
    columns = _overlap_groups(words, lambda word: (word[0], word[2]))
    if len(bands) < 1 or len(columns) < 2:
        return None
    xs = [region[0]]
    for left, right in zip(columns, columns[1:]):
        xs.append((left[1] + right[0]) / 2)
    xs.append(region[2])
    ys = [region[1]]
    for upper, lower in zip(bands, bands[1:]):
        ys.append((upper[1] + lower[0]) / 2)
    ys.append(region[3])
    return xs, ys


def _pipe_table(rows):
    """Render string rows as a pipe table with a header separator.

    Two blank lines close a layout table: the recorded block ends
    ``|\\n\\n\\n\\n``.
    """
    if not rows:
        return ""
    out = ["|" + "|".join(rows[0]) + "|\n"]
    out.append("|" + "|".join(["---"] * len(rows[0])) + "|\n")
    for row in rows[1:]:
        out.append("|" + "|".join(row) + "|\n")
    out.append("\n\n\n")
    return "".join(out)


#: Spaces the extractor left inside numbers ('18 . 90' reads '18.90').
#: Shared with the legacy cell text, which documents the reference.
_DIGIT_SPACE_RE = re.compile(r"(?<=[\d.]) +(?=[\d.])")


def _structure_tables(native, index, proposals, rulings, fills, visible, siblings):
    """Turn table proposals into table boxes; returns boxes plus eaten spans.

    Regions mostly covered by fills read as text instead of tables.
    Unspecified upstream; our behaviour for the shaded-table miss.
    """
    boxes = []
    from . import _tables as table_lane

    consumed = set()
    skipped = []
    for proposal in proposals:
        # Same float width as the rulings, fills and span boxes: a
        # span starting exactly on the region edge must classify.
        region = _f32_list(proposal["region"])
        # Shaded through and through: layout runs it together (G105).
        # The union counts banded fills once; a single-fill guard
        # would miss cell-by-cell shading.
        if (
            table_lane._union_fill_coverage(fills, region)
            >= table_lane._SHADED_COVERAGE
        ):
            skipped.append(region)
            continue
        grid = _ruling_grid(rulings, region)
        if grid is None:
            grid = _alignment_grid(native, index, region, visible)
        if grid is None:
            continue
        xs, ys = grid
        row_count, col_count = len(ys) - 1, len(xs) - 1
        cells = [
            [[xs[c], ys[r], xs[c + 1], ys[r + 1]] for c in range(col_count)]
            for r in range(row_count)
        ]
        buckets = [[[] for _ in range(col_count)] for _ in range(row_count)]
        for span in visible:
            if id(span) in consumed:
                continue
            box = span["bbox"]
            column = _band_index(xs, box[0])
            row = _band_index(ys, (box[1] + box[3]) / 2)
            if row is None or column is None:
                continue
            buckets[row][column].append(span)
        extract = []
        styled = []
        for r in range(row_count):
            plain_row = []
            styled_row = []
            for c in range(col_count):
                cell = sorted(buckets[r][c], key=lambda span: span["bbox"][0])
                for span in cell:
                    consumed.add(id(span))
                # Numbers read joined ('18.90', not '18 . 90'), the way
                # the legacy cell text normalises digit-dot runs; word
                # spaces survive.
                plain_row.append(
                    _DIGIT_SPACE_RE.sub("", siblings.get_plain_text(cell)).strip()
                    if cell
                    else ""
                )
                styled_row.append(
                    _DIGIT_SPACE_RE.sub("", siblings.get_styled_text(cell)[0]).strip()
                    if cell
                    else ""
                )
            extract.append(plain_row)
            styled.append(styled_row)
        bbox = _unions([cell for row in cells for cell in row])
        details = TableDetails(
            bbox=bbox,
            row_count=row_count,
            col_count=col_count,
            cells=cells,
            extract=extract,
            markdown=_pipe_table(styled),
        )
        boxes.append(
            {
                "kind": "table",
                "box": LayoutBox(
                    bbox[0], bbox[1], bbox[2], bbox[3], "table", table=details
                ),
                "y0": bbox[1],
            }
        )
    return boxes, consumed, skipped


def _touches_vector(bbox, rulings, fills):
    """True when a ruling crosses the box or a fill strictly overlaps it."""
    for line in rulings:
        if line[0] == line[2]:
            if (
                bbox[0] <= line[0] <= bbox[2]
                and line[1] <= bbox[3]
                and line[3] >= bbox[1]
            ):
                return True
        elif line[1] == line[3]:
            if (
                bbox[1] <= line[1] <= bbox[3]
                and line[0] <= bbox[2]
                and line[2] >= bbox[0]
            ):
                return True
    return any(_strict_overlap(bbox, fill) for fill in fills)


def _labels_fill(box, fills, size):
    """True when the box reads as a filled graphic's label.

    Column-overlapping a fill whose bottom sits less than two sizes
    above the line (the compat chart's '2001'/'2005' under the bars).
    Text inside fills -- form values, shaded rows -- is prose, so only
    lines strictly below a fill carry.
    """
    for fill in fills:
        if min(box[2], fill[2]) <= max(box[0], fill[0]):
            continue
        if fill[3] < box[1] and box[1] - fill[3] < 2 * size:
            return True
    return False


def _ink_gap(prev_raw, span):
    """Positional gap past the previous span's ink, plus its size."""
    from ._markdown import _MONO_BIT, _ink_end

    size = max(span.get("size") or 0, prev_raw.get("size") or 0)
    if size <= 0:
        return 0.0, 0.0
    if int(prev_raw.get("flags") or 0) & _MONO_BIT:
        ink_end = prev_raw["bbox"][0] + 0.6 * size
    else:
        ink_end = _ink_end(
            prev_raw["bbox"][0], prev_raw.get("text") or "", size
        )
    return span["bbox"][0] - ink_end, size


def _join_row_spans(spans):
    """Join same-style spans into word runs; space glyphs carry spaces.

    Layout textlines read whole words ('Compat Frame Fixture: running
    head' is one span), so same-style spans concatenate and style
    boundaries open new spans. Word spaces come from ' ' glyphs, plus
    two inferences: across a style boundary a wide gap hides a word
    space the extractor left positional ('line here.' + 'bold bit'),
    and inside one style a gap past a full em does too -- column gaps
    in run-together tables ('Band Rate'). Narrower same-style gaps
    never infer: tracked display type ('PAGE 1 OF 5') stays joined.
    (The legacy line grouping restores positional spaces separately;
    its fixtures have none to carry.)
    """
    from ._markdown import _MONO_BIT, _ink_end

    if not spans:
        return spans
    joined = [dict(spans[0])]
    prev_raw = spans[0]
    for span in spans[1:]:
        prev = joined[-1]
        if span.get("flags") == prev.get("flags"):
            gap, size = _ink_gap(prev_raw, span)
            if (
                size > 0
                and gap > size
                and not (prev.get("text") or "")[-1:].isspace()
                and not (span.get("text") or "")[:1].isspace()
            ):
                prev["text"] = (prev.get("text") or "") + " "
            prev["text"] = (prev.get("text") or "") + (span.get("text") or "")
            prev["bbox"] = _unions([prev["bbox"], span["bbox"]])
            prev_raw = span
            continue
        size = max(span.get("size") or 0, prev.get("size") or 0)
        if size > 0:
            if int(prev_raw.get("flags") or 0) & _MONO_BIT:
                ink_end = prev_raw["bbox"][0] + 0.6 * size
            else:
                ink_end = _ink_end(
                    prev_raw["bbox"][0], prev_raw.get("text") or "", size
                )
        else:
            ink_end = prev["bbox"][2]
        gap = span["bbox"][0] - ink_end
        if (
            size > 0
            and gap > 0.2 * size
            and not (prev.get("text") or "")[-1:].isspace()
            and not (span.get("text") or "")[:1].isspace()
        ):
            prev["text"] = (prev.get("text") or "") + " "
        joined.append(dict(span))
        prev_raw = span
    return joined


def _split_wide_gaps(spans):
    """Split spans into segments at gaps past a full em.

    Picture labels share the extractor's line but sit pages apart
    ('2001' ... '2005' on the compat chart); each segment becomes its
    own carried line. Prose rows never split -- they join with a space
    instead (see the same-style inference in `_join_row_spans`).
    """
    if not spans:
        return []
    segments = [[spans[0]]]
    for previous, span in zip(spans, spans[1:]):
        gap, size = _ink_gap(previous, span)
        if size > 0 and gap > size:
            segments.append([])
        segments[-1].append(span)
    return segments


def _collect_rows(pairs, visible_ids, consumed, rulings, fills, siblings, skipped):
    """Turn raw lines into text rows, carried pictures and leftovers.

    Whitespace-only spans vanish, punctuation-only lines vanish (bullet
    markers survive), lines touching vector ink become carried pictures and
    non-horizontal lines are left over for empty header boxes. Lines
    centred in a skipped table region stay prose: a shaded table's rows
    cross their own cell borders, but they are still body text (G105).
    """
    lines = []
    leftover = []
    for _bbox, spans in pairs:
        kept = [
            span
            for span in spans
            if id(span) in visible_ids and id(span) not in consumed
        ]
        kept = [
            span
            for span in kept
            if (span.get("text") or "").strip()
            or (span.get("text") or "") in (" ", "\xa0", "\t")
        ]
        if not kept:
            continue
        if not any((span.get("text") or "").strip() for span in kept):
            continue
        box = _unions([span["bbox"] for span in kept])
        if not _line_is_horizontal(kept):
            leftover.append({"bbox": box})
            continue
        text = "".join(span.get("text") or "" for span in kept)
        stripped = text.strip()
        if stripped and all(char in string.punctuation for char in stripped):
            from ._markdown import _MONO_BIT

            mono = all(
                int(span.get("flags") or 0) & _MONO_BIT for span in kept
            )
            # Braces and punctuation lines are code in a monospaced
            # line; elsewhere stray punctuation still drops.
            if not siblings.startswith_bullet(stripped) and not mono:
                continue
        lines.append({"bbox": box, "spans": kept})
    carried = []
    survivors = []
    for line in lines:
        box = line["bbox"]
        centre = ((box[0] + box[2]) / 2, (box[1] + box[3]) / 2)
        in_skipped = any(
            region[0] <= centre[0] <= region[2]
            and region[1] <= centre[1] <= region[3]
            for region in skipped
        )
        size = max(span.get("size") or 0 for span in line["spans"])
        if (
            _touches_vector(box, rulings, fills)
            or _labels_fill(box, fills, size)
        ) and not in_skipped:
            ordered = sorted(line["spans"], key=lambda span: span["bbox"][0])
            for segment in _split_wide_gaps(ordered):
                carried.append(
                    {
                        "bbox": _unions([span["bbox"] for span in segment]),
                        "spans": _join_row_spans(segment),
                    }
                )
        else:
            survivors.append(line)
    survivors.sort(key=lambda line: (line["bbox"][1], line["bbox"][0]))
    rows = []
    for line in survivors:
        if rows and _y_overlap(rows[-1]["bbox"], line["bbox"]):
            rows[-1]["spans"].extend(line["spans"])
            rows[-1]["spans"].sort(key=lambda span: span["bbox"][0])
            rows[-1]["spans"] = _join_row_spans(rows[-1]["spans"])
            rows[-1]["bbox"] = _unions([span["bbox"] for span in rows[-1]["spans"]])
        else:
            rows.append(
                {
                    "bbox": line["bbox"],
                    "spans": _join_row_spans(
                        sorted(line["spans"], key=lambda span: span["bbox"][0])
                    ),
                }
            )
    for row in rows:
        row["size"] = max(span.get("size") or 0 for span in row["spans"])
    return rows, carried, leftover


def _merge_rows(rows):
    """Merge consecutive rows split by less than a blank line, same size."""
    from ._markdown import _ORDERED_ITEM_RE, startswith_bullet

    boxes = []
    for row in rows:
        # Native spans only carry origins, so row boxes are flat and the
        # line height comes from the size: under two line heights the
        # next row continues the box (wrapped prose, code lines), past
        # it a new box opens (paragraph breaks, separated blocks). A row
        # opening with a list marker always opens its own box.
        tall = max(row["bbox"][3] - row["bbox"][1], 2 * row["size"])
        opens_item = startswith_bullet(
            "".join(span.get("text") or "" for span in row["spans"]).lstrip()
        ) or _ORDERED_ITEM_RE.match(
            "".join(span.get("text") or "" for span in row["spans"]).lstrip()
        )
        if (
            boxes
            and row["bbox"][1] - boxes[-1]["bbox"][3] < tall
            and row["size"] == boxes[-1]["size"]
            and not opens_item
        ):
            boxes[-1]["rows"].append(row)
            boxes[-1]["bbox"] = _unions([boxes[-1]["bbox"], row["bbox"]])
        else:
            boxes.append(
                {"rows": [row], "bbox": list(row["bbox"]), "size": row["size"]}
            )
    return boxes


def _order_boxes(texts, tables, images, clusters, carried, leftover):
    """Interleave text, tables and pictures by top edge.

    Raster pictures travel as one detection-ordered group keyed at their
    highest top; every other picture keeps its own top. Unspecified upstream;
    our behaviour for the group key, which no fixture distinguishes.
    """
    entries = []
    for box in texts:
        entries.append((box["bbox"][1], 0, ("text", box)))
    for table in tables:
        entries.append((table["y0"], 1, ("table", table)))
    if images:
        entries.append(
            (min(image["bbox"][1] for image in images), 2, ("images", images))
        )
    for cluster in clusters:
        entries.append((cluster[1], 3, ("graphic", cluster)))
    for line in carried:
        entries.append((line["bbox"][1], 4, ("carried", line)))
    for line in leftover:
        entries.append((line["bbox"][1], 5, ("leftover", line)))
    entries.sort(key=lambda entry: (entry[0], entry[1]))
    ordered = []
    for _, _, (kind, payload) in entries:
        if kind == "text":
            ordered.append({"kind": "text", **payload})
        elif kind == "table":
            ordered.append(payload)
        elif kind == "images":
            for image in payload:
                ordered.append(
                    {
                        "kind": "picture",
                        "raster": True,
                        "bbox": image["bbox"],
                        "details": image,
                    }
                )
        elif kind == "graphic":
            ordered.append(
                {"kind": "picture", "raster": False, "bbox": payload, "details": None}
            )
        elif kind == "carried":
            ordered.append(
                {
                    "kind": "picture",
                    "raster": False,
                    "bbox": payload["bbox"],
                    "details": None,
                    "textlines": [{"bbox": payload["bbox"], "spans": payload["spans"]}],
                }
            )
        else:
            ordered.append({"kind": "leftover", "bbox": payload["bbox"]})
    return ordered


def _classify_boxes(ordered, width, height, pairs, visible, siblings):
    """Classify ordered boxes; headers and levels follow the recorded data.

    Running heads are the first single-line box in the top inch, on
    single-box pages, or beside hidden text; big type makes section headers;
    the line after raster pictures is the caption. Several of these splits
    are inferred from the recordings rather than specified; see the report.
    """
    from ._inputs import is_ocr_text
    from ._markdown import _ORDERED_ITEM_RE

    del width
    has_hidden = any(span.get("alpha", 255) == 0 for span in _iter_page_spans(pairs))
    has_visible = any(span.get("alpha", 255) != 0 for span in _iter_page_spans(pairs))
    ocr_sizes = [span.get("size") or 0 for span in visible if is_ocr_text(span)]
    ocr_floor = min(ocr_sizes) if ocr_sizes else None
    total = len(ordered)
    out = []
    for position, entry in enumerate(ordered):
        kind = entry["kind"]
        if kind == "table":
            out.append(entry["box"])
            continue
        if kind == "picture":
            bbox = entry["bbox"]
            box = LayoutBox(
                bbox[0],
                bbox[1],
                bbox[2],
                bbox[3],
                "picture",
                textlines=entry.get("textlines", []),
            )
            box._raster = entry.get("raster", False)
            out.append(box)
            continue
        if kind == "leftover":
            bbox = entry["bbox"]
            out.append(
                LayoutBox(
                    bbox[0], bbox[1], bbox[2], bbox[3], "page-header", textlines=[]
                )
            )
            continue
        rows = entry["rows"]
        bbox = entry["bbox"]
        size = entry["size"]
        textlines = [{"bbox": row["bbox"], "spans": row["spans"]} for row in rows]
        single = len(textlines) == 1
        first = position == 0
        last = position == total - 1
        boxclass = "text"
        if last and single and (height - bbox[3]) < 72:
            plain = "".join(
                span.get("text") or "" for span in textlines[0]["spans"]
            ).lstrip()
            boxclass = "footnote" if plain[:1].isdigit() else "page-footer"
        elif size >= 16:
            boxclass = "section-header"
        elif (
            single
            and first
            and size < 16
            and (bbox[1] < 72 or total == 1 or (has_hidden and has_visible))
        ):
            boxclass = "page-header"
        elif size >= 12 or (
            ocr_floor is not None
            and size > ocr_floor
            and all(is_ocr_text(span) for row in rows for span in row["spans"])
        ):
            boxclass = "section-header"
        elif (
            position > 0
            and out[-1].boxclass == "picture"
            and getattr(out[-1], "_raster", False)
        ):
            boxclass = "caption"
        elif siblings.startswith_bullet(
            "".join(span.get("text") or "" for span in textlines[0]["spans"]).lstrip()
        ) or _ORDERED_ITEM_RE.match(
            "".join(span.get("text") or "" for span in textlines[0]["spans"]).lstrip()
        ):
            boxclass = "list-item"
        out.append(
            LayoutBox(bbox[0], bbox[1], bbox[2], bbox[3], boxclass, textlines=textlines)
        )
    return out


def _box_max_size(box):
    """Largest span size in a box, for header levels."""
    sizes = [
        span.get("size") or 0 for line in box.textlines or [] for span in line["spans"]
    ]
    return max(sizes) if sizes else 0


def _assign_header_levels(parsed):
    """Rank section headers per document by size for levels and maxima."""
    sizes = set()
    for page in parsed.pages or []:
        for box in page.boxes or []:
            if box.boxclass == "section-header":
                sizes.add(_box_max_size(box))
    ranked = sorted(sizes, reverse=True)
    for page in parsed.pages or []:
        for box in page.boxes or []:
            if box.boxclass == "section-header":
                top = _box_max_size(box)
                box.header_level = ranked.index(top) + 1
                box.max_fontsize = int(top)
            else:
                box.header_level = 0
                box.max_fontsize = None


def _record_bytes(record):
    """Pull raw image bytes out of an image-lane record, or None.

    The image lane owns the record shape; bytes ride under the first
    bytes-valued entry found. See the integration report.
    """
    if isinstance(record, (bytes, bytearray)):
        return bytes(record)
    if isinstance(record, dict):
        for key in ("bytes", "image", "data", "content"):
            value = record.get(key)
            if isinstance(value, (bytes, bytearray)):
                return bytes(value)
    return None


def _attach_images(
    classified,
    kept_images,
    native,
    index,
    stem,
    image_format,
    image_path,
    embed_images,
    write_images,
):
    """Write or embed raster picture bytes; vector pictures stay bare.

    Image bytes come from the image lane's per-page extraction, correlated
    by order with the raster picture boxes. Unknown formats write nothing.
    """
    from . import _images as image_lane

    del kept_images
    raster = [
        box
        for box in classified
        if box.boxclass == "picture" and getattr(box, "_raster", False)
    ]
    if not raster:
        return
    records = image_lane.extract_images_on_page_simple(native[index], None, 0)
    known = (image_format or "png").lower()
    ext = {"png": "png", "jpg": "jpg", "jpeg": "jpeg"}.get(known)
    for position, box in enumerate(raster):
        payload = _record_bytes(records[position]) if position < len(records) else None
        if payload is None:
            continue
        if write_images and ext is not None:
            name = "%s-%04d-%02d.%s" % (stem, index + 1, position + 1, ext)
            if image_path:
                os.makedirs(image_path, exist_ok=True)
                path = image_lane.md_path(image_path, name)
            else:
                path = name
            with open(path, "wb") as handle:
                handle.write(payload)
            box.image = path
        if embed_images:
            box.image = base64.b64encode(payload).decode("ascii")


def _list_levels(page):
    """Nesting level per list-item box from indent runs in steps of 30pt.

    Levels count from 1 (top-level items), matching the renderers'
    indent of three spaces per level below the top. Unspecified
    upstream; our behaviour for levels beyond the one recorded
    nesting step.
    """
    levels = {}
    run = []

    def flush():
        if not run:
            return
        base = min(box.x0 for box in run)
        for box in run:
            levels[id(box)] = max(0, int(round((box.x0 - base) / 30))) + 1

    for box in page.boxes or []:
        if box.boxclass == "list-item":
            run.append(box)
        else:
            flush()
            run = []
    flush()
    return levels


def _image_ref(box, page, write, embed, limit, image_format):
    """Markdown reference for a picture box, or None for a bare gap."""
    if not box.image or not (write or embed):
        return None
    width, height = page.width, page.height
    if width and height:
        small = min((box.x1 - box.x0) / width, (box.y1 - box.y0) / height)
        if small < limit:
            return None
    if embed:
        mime = {"png": "image/png", "jpg": "image/jpeg", "jpeg": "image/jpeg"}.get(
            (image_format or "png").lower(), "image/png"
        )
        return "data:%s;base64,%s" % (mime, box.image)
    return box.image


def _table_text(table, table_format, max_width, min_col_width):
    """Render a table record as a plain-text grid.

    Only the recorded formats pass through to the grid renderer; anything
    else, including "markdown", renders as grid. Unspecified upstream; our
    behaviour for formats beyond the recorded set.
    """
    if table is None or not table.extract:
        return ""
    from . import _tables as table_lane

    import tabulate

    if table_format not in ("grid", "plain", "simple", "psql"):
        table_format = "grid"
    wrapped = table_lane.wrap_table_for_tabulate(
        table.extract, max_width, min_col_width
    )
    return tabulate.tabulate(wrapped, tablefmt=table_format) + "\n\n"


def _render_box(
    box,
    page,
    kind,
    ignore_code,
    write,
    embed,
    limit,
    level,
    table_format,
    table_max_width,
    table_min_col_width,
    image_format,
    siblings,
):
    """Render one box to its markdown or text piece (possibly empty)."""
    if box.boxclass == "table":
        if kind == "md":
            return box.table.markdown or "" if box.table else ""
        return _table_text(
            box.table, table_format, table_max_width, table_min_col_width
        )
    if box.boxclass == "picture":
        lines = box.textlines or []
        ref = _image_ref(box, page, write, embed, limit, image_format)
        if kind == "text":
            if lines:
                # No leading blank: the recorded text blocks abut the
                # previous piece (G124).
                return siblings.picture_text_to_text(lines, ignore_code)
            return ""
        if ref:
            return "\n![](%s)\n\n" % ref
        if lines:
            return "\n\n" + siblings.picture_text_to_md(lines, ignore_code)
        return "\n\n"
    lines = box.textlines or []
    if not lines:
        return ""
    if box.boxclass == "footnote":
        if kind == "md":
            return siblings.footnote_to_md(lines)
        return siblings.footnote_to_text(lines)
    if box.boxclass == "list-item":
        if kind == "md":
            return siblings.list_item_to_md(lines, level)
        return siblings.list_item_to_text(lines, level)
    if box.boxclass == "section-header" and kind == "md":
        return siblings.section_hdr_to_md(box.header_level, lines)
    if siblings.is_monospaced(lines) and not ignore_code:
        if kind == "md":
            return siblings.code_block_to_md(lines)
        return siblings.code_block_to_text(lines)
    if kind == "md":
        return siblings.text_to_md(lines, ignore_code)
    return siblings.text_to_text(lines, ignore_code)


def _render_document(
    doc,
    kind,
    header,
    footer,
    write_images,
    embed_images,
    ignore_code,
    page_separators,
    page_chunks,
    options,
):
    """Shared markdown/text renderer, returning a string or chunk list.

    Page separators append an end-of-page marker line plus a blank line to
    each page's text, last page included; chunk texts stay unmarked.
    Unspecified upstream; our behaviour for separators combined with chunks
    and for the text flavour, neither of which was recorded. A size limit
    of 1 or more is a ValueError.
    """
    from . import _markdown as siblings

    write = bool(write_images or doc.write_images)
    embed = bool(embed_images or doc.embed_images)
    limit = options.get("image_size_limit", 0.05)
    if limit is None:
        limit = 0.05
    if limit >= 1 or limit < 0:
        raise ValueError("image_size_limit must be below 1")
    table_format = options.get("table_format", "grid")
    table_max_width = options.get("table_max_width", 100)
    table_min_col_width = options.get("table_min_col_width", 10)
    texts = []
    chunks = []
    for page in doc.pages or []:
        levels = _list_levels(page)
        pieces = []
        for box in page.boxes or []:
            if box.boxclass == "page-header" and not header:
                pieces.append("")
                continue
            if box.boxclass == "page-footer" and not footer:
                pieces.append("")
                continue
            pieces.append(
                _render_box(
                    box,
                    page,
                    kind,
                    ignore_code,
                    write,
                    embed,
                    limit,
                    levels.get(id(box), 0),
                    table_format,
                    table_max_width,
                    table_min_col_width,
                    doc.image_format,
                    siblings,
                )
            )
        text = "".join(pieces)
        if page_separators:
            text += "--- end of page.page_number=%d ---\n\n" % page.page_number
        texts.append(text)
        if page_chunks:
            chunks.append(
                make_page_chunk(
                    doc, page, "".join(pieces), [len(piece) for piece in pieces]
                )
            )
    if page_chunks:
        return chunks
    return "".join(texts)


def to_json(*args, **kwargs):
    """Convert a document to its JSON string (layout only, SPEC G122)."""
    doc = args[0] if args else kwargs.pop("doc", None)
    early = check_call("to_json", doc, dict(kwargs), layout=is_layout_mode())
    if early is not CONTINUE:
        return early
    if not is_layout_mode():
        raise NotImplementedError("JSON is only available in layout mode")
    known = {
        "filename": "",
        "image_dpi": 150,
        "ocr_dpi": 150,
        "image_format": "png",
        "image_path": "",
        "pages": None,
        "show_progress": False,
        "embed_images": False,
        "write_images": False,
        "force_text": True,
        "use_ocr": True,
        "force_ocr": False,
        "ocr_language": "eng",
        "ocr_function": None,
        "render_html_tables": None,
        "edge_threshold": None,
    }
    for key in kwargs:
        if key in known:
            known[key] = kwargs[key]
    return parse_document(doc, **known).to_json()


def _form_fields(native, xrefs):
    """Field name to value/pages record from native form details."""
    out = {}
    for record in _native_module().door_form_details(native):
        entry = {"value": record["value"], "pages": [int(p) for p in record["pages"]]}
        if xrefs:
            entry["xref"] = int(record["xref"])
        out[record["name"]] = entry
    return out


def get_key_values(doc, xrefs=False, **kwargs):
    """Map form field names to their value and 0-based page list."""
    del kwargs
    if doc is None:
        raise TypeError("expected a file path or Document, got None")
    native, _display = _open_doc(doc)
    return _form_fields(native, bool(xrefs))


def extract_form_fields_with_pages(doc, xrefs=False):
    """Map form field names to value, pages and optionally xref.

    Lives here; installed onto the utils helper at import so the helper
    path and the top-level view share one implementation.
    """
    if doc is None:
        raise TypeError("expected a file path or Document, got None")
    native, _display = _open_doc(doc)
    return _form_fields(native, bool(xrefs))


def _words_for_page(native, index):
    """Legacy word rows for one page via the W1 word binding.

    Each row is [x0, y0, x1, y1, text, block, line, word] in the y-down
    frame; the binding reports y-up. Raises NotImplementedError while the
    binding is absent.
    """
    module = _native_module()
    if not hasattr(module.Document, "extract_words"):
        raise NotImplementedError("word extraction needs the W1 word binding")
    page = native[index]
    height = _exact_page_size(page)[1]
    out = []
    for word in page.words():
        x0, y0, x1, y1 = tuple(word.bbox)
        out.append(
            [
                _f32(x0),
                _f32(height - y1),
                _f32(x1),
                _f32(height - y0),
                word.text,
                int(word.block),
                int(word.line),
                int(word.word),
            ]
        )
    return out


def _legacy_tables(native, index, proposals, drawings):
    """Loose legacy table records: ruling-span bbox with grid size.

    Unspecified upstream; our behaviour: the bbox spans the rulings inside
    the proposal, falling back to the proposal region without rulings.
    """
    rulings = drawings["rulings"]
    fills = drawings["fills"]
    tables = []
    for proposal in proposals:
        region = proposal["region"]
        covered = max([_fill_coverage(fill, region) for fill in fills] or [0.0])
        if covered >= 0.5:
            continue
        grid = _ruling_grid(rulings, region)
        if grid is None:
            grid = _alignment_grid(native, index, region, [])
        if grid is None:
            continue
        xs, ys = grid
        inside = [line for line in rulings if _rects_touch(line, region)]
        if inside:
            bbox = _unions(inside)
        else:
            bbox = list(region)
        tables.append(
            {
                "bbox": _f32_list(bbox),
                "rows": len(ys) - 1,
                "columns": len(xs) - 1,
            }
        )
    return tables


_PARSE_KEYS = (
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


def _layout_chunks(doc, options):
    """Build layout page chunks through a fresh parse.

    A lone integer page renders just that page; an empty selection is an
    IndexError, matching the layout entry's own reports.
    """
    raw_pages = options.get("pages")
    if isinstance(raw_pages, int) and not isinstance(raw_pages, bool):
        raw_pages = [raw_pages]
    if isinstance(raw_pages, (list, tuple, range)) and len(raw_pages) == 0:
        raise IndexError("no pages selected")
    parse_options = {key: options[key] for key in _PARSE_KEYS if key in options}
    if "pages" in options:
        parse_options["pages"] = raw_pages
    parsed = parse_document(doc, **parse_options)
    extra = {
        key: options[key]
        for key in (
            "image_size_limit",
            "table_format",
            "table_max_width",
            "table_min_col_width",
        )
        if key in options
    }
    return parsed.to_markdown(
        header=options.get("header", True),
        footer=options.get("footer", True),
        write_images=options.get("write_images", False),
        embed_images=options.get("embed_images", False),
        ignore_code=options.get("ignore_code", False),
        page_separators=options.get("page_separators", False),
        page_chunks=True,
        **extra,
    )


def _legacy_chunks(doc, options):
    """Build legacy page chunks page by page through the legacy entry."""
    from . import _markdown as siblings

    native, display = _open_doc(doc)
    count = native.page_count
    indices = _normalize_legacy_pages(options.get("pages"), count)
    module = _native_module()
    file_meta = module.door_file_metadata(native)
    toc = [list(row) for row in module.door_toc(native)]
    if display is not None:
        shown = display
    else:
        shown = getattr(native, "name", None) or options.get("filename", "") or ""
    want_words = bool(options.get("extract_words"))
    headers = siblings.IdentifyHeaders(doc)
    chunks = []
    for index in indices:
        sub = {
            key: value
            for key, value in options.items()
            if key not in ("page_chunks", "extract_words", "doc", "pages")
        }
        sub["pages"] = [index]
        sub.setdefault("hdr_info", headers)
        text = siblings.legacy_to_markdown(native, **sub)
        drawings = module.door_page_drawings(native, index)
        proposals = module.door_page_tables(native, index)
        tables = _legacy_tables(native, index, proposals, drawings)
        pairs = _normalise_pairs(
            siblings.get_raw_lines(
                textpage=native[index], ignore_invisible=False, only_horizontal=False
            )
        )
        image_numbers = _number_images(_fulltext_blocks(pairs), drawings["images"])
        images = []
        for number, record in zip(image_numbers, drawings["images"]):
            images.append(
                {
                    "number": number,
                    "bbox": {"$type": "Rect"},
                    "transform": _f32_list(record["transform"]),
                    "width": record["width"],
                    "height": record["height"],
                    "colorspace": record["colorspace"],
                    "cs-name": record["cs-name"],
                    "xres": record["xres"],
                    "yres": record["yres"],
                    "bpc": record["bpc"],
                    "size": record["size"],
                    "has-mask": record["has-mask"],
                }
            )
        words = _words_for_page(native, index) if want_words else []
        metadata = dict(file_meta)
        metadata["file_path"] = shown
        metadata["page_count"] = count
        metadata["page"] = index + 1
        chunks.append(
            {
                "text": text,
                "tables": tables,
                "images": images,
                "graphics": [],
                "words": words,
                "metadata": metadata,
                "toc_items": [list(row) for row in toc if row[2] == index + 1],
            }
        )
    return chunks


def _routable(doc):
    """True when chunk assembly can take the input directly."""
    return isinstance(doc, (str, os.PathLike)) or hasattr(doc, "page_count")


def _install_wrappers():
    """Route chunk requests on the markdown entries to this module.

    The layout entry learns `page_chunks`, the legacy entry learns
    `page_chunks` and `extract_words`; anything else passes through
    untouched. The utils helper gains the shared form-field view. All
    three patches keep the recorded signatures intact.
    """
    from . import _markdown as entries

    original_top = entries.to_markdown
    original_legacy = entries.legacy_to_markdown

    def to_markdown(*args, **kwargs):
        doc = args[0] if args else kwargs.get("doc")
        if _routable(doc) and kwargs.get("page_chunks"):
            early = check_call(
                "to_markdown", doc, dict(kwargs), layout=is_layout_mode()
            )
            if early is not CONTINUE:
                return early
            if is_layout_mode():
                return _layout_chunks(doc, kwargs)
            return _legacy_chunks(doc, kwargs)
        if _routable(doc) and not is_layout_mode() and kwargs.get("extract_words"):
            early = check_call(
                "to_markdown", doc, dict(kwargs), layout=is_layout_mode()
            )
            if early is not CONTINUE:
                return early
            return _legacy_chunks(doc, kwargs)
        return original_top(*args, **kwargs)

    @functools.wraps(original_legacy)
    def legacy_to_markdown(*args, **kwargs):
        doc = args[0] if args else kwargs.get("doc")
        if _routable(doc) and (
            kwargs.get("page_chunks") or kwargs.get("extract_words")
        ):
            early = check_call("to_markdown", doc, dict(kwargs), layout=False)
            if early is not CONTINUE:
                return early
            return _legacy_chunks(doc, kwargs)
        return original_legacy(*args, **kwargs)

    entries.to_markdown = to_markdown
    entries.legacy_to_markdown = legacy_to_markdown
    from .helpers import legacy as legacy_helper
    from .helpers import utils as utils_helper

    legacy_helper.to_markdown = legacy_to_markdown
    utils_helper.extract_form_fields_with_pages = extract_form_fields_with_pages


_install_wrappers()
