# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L6: SPEC sections 15, 16, 17, 21.
"""Batch conversion, progress display and OCR selection for the front door.

This lane owns the door's non-PDF inputs (EPUB, XPS and PNG render through
rules registered here, Office archives are refused), batch conversion with
its run logs, the text progress bar, and the helpers that pick the OCR
path: the recogniser choice, the per-page decision, and the page analysis
behind it.
"""
from __future__ import annotations

import contextlib
import datetime
import io
import os
import re
import sys
import zipfile
from enum import IntEnum
from typing import Any, List
from xml.etree import ElementTree

from ._errors import (
    CONTINUE,
    FileDataError,
    check_call,
    register_document_rule,
    register_option_rule,
)
from ._markdown import REPLACEMENT_CHARACTER, to_markdown
from ._mode import is_batch_poisoned, is_layout_mode, note_batch_run

__all__ = [
    "OCRMode",
    "ANALYZE_FLAGS",
    "BAD_CHAR_THRESHOLD",
    "BLOCK_IMAGE",
    "BLOCK_TEXT",
    "BLOCK_VECTOR",
    "FEATURE_NAMES",
    "GRAY",
    "TEXT_FILLED",
    "TEXT_STROKED",
    "OCR_MODEL_THRESHOLD",
    "SOBEL_X",
    "SOBEL_Y",
    "TESSERACT_FONT_NAME",
    "OFFICE_FORMATS",
    "OFFICE_TO_PDF",
    "INFO_MESSAGES",
    "RUNNING_WITH_MARINA",
    "BatchResult",
    "convert_batch",
    "convert_document",
    "auto_workers",
    "ProgressBar",
    "select_ocr_function",
    "make_ocr_decision",
    "analyze_page",
    "get_layout_locked",
    "is_ocr_text",
    "is_ocr_span",
    "compute_features",
    "conv2d_fast",
    "sobel_features",
    "sobel_features_page",
    "predict_ocr_probability",
    "check_images",
    "as_strided",
]


class OCRMode(IntEnum):
    """How hard a conversion looks for text it cannot see."""

    NEVER = 0
    SELECT_DROP_OLD = 1
    SELECT_KEEP_OLD = 2
    FORCE_DROP_OLD = 3
    FORCE_KEEP_OLD = 4


#: Page-reading flags the analysis pass uses.
ANALYZE_FLAGS = 1540
#: Bad-character fraction above which a page reads as garbled.
BAD_CHAR_THRESHOLD = 0.1
#: Block-type tag for image blocks.
BLOCK_IMAGE = 1
#: Block-type tag for text blocks.
BLOCK_TEXT = 0
#: Block-type tag for vector blocks.
BLOCK_VECTOR = 3
#: Feature names :func:`compute_features` returns, in order.
FEATURE_NAMES = [
    "text_chars",
    "text_blocks",
    "edge_density",
    "mean_magnitude",
    "dark_fraction",
    "dark_ink",
    "dark_spread",
]
#: No colour-space objects exist on this side of the binding; grey is a
#: conversion the analysis does itself.
GRAY = None
#: Span flag for filled text.
TEXT_FILLED = 16
#: Span flag for stroked text.
TEXT_STROKED = 32
#: Recognition score at or above which a page wants OCR.
OCR_MODEL_THRESHOLD = 0.93
#: Horizontal Sobel kernel.
SOBEL_X = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]]
#: Vertical Sobel kernel.
SOBEL_Y = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]]
#: Font name the OCR layer writes its spans in.
TESSERACT_FONT_NAME = "GlyphLessFont"
#: Office files never convert through the door, so no format converts.
OFFICE_FORMATS = set()
#: Office files never convert through the door, so nothing converts them.
OFFICE_TO_PDF = None
#: The running conversion's parser messages collect here.
INFO_MESSAGES = io.StringIO()
#: Recognition tuning is off in this build.
RUNNING_WITH_MARINA = False


class BatchResult:
    """One file's outcome from :func:`convert_batch`."""

    def __init__(self, input, success, error, output):
        self.input = input
        self.success = success
        self.error = error
        self.output = output

    def __repr__(self):
        return "BatchResult(input=%r, success=%r, error=%r, output=%r)" % (
            self.input,
            self.success,
            self.error,
            self.output,
        )


#: The non-PDF input the current call stashed for its option rule: one of
#: ("epub", chapters), ("xps", None), ("png", None), or None for PDFs.
_STASH = None

#: While set, the document rules below stand down: batch's own argument
#: check passes no document, and must not trip over that.
_SUSPEND_DOC_RULES = False


@contextlib.contextmanager
def _suspend_doc_rules():
    """Run one check with this lane's document rules stood down."""
    global _SUSPEND_DOC_RULES
    was = _SUSPEND_DOC_RULES
    _SUSPEND_DOC_RULES = True
    try:
        yield
    finally:
        _SUSPEND_DOC_RULES = was


@register_document_rule
def _rule_scalar_docs(doc, layout):
    """Refuse scalar documents: None, integers and raw bytes."""
    if _SUSPEND_DOC_RULES:
        return CONTINUE
    if doc is None:
        if layout:
            # The entries refuse None themselves ("no document was given");
            # the rule stands down so its message never pre-empts theirs.
            return CONTINUE
        return ""
    if isinstance(doc, int):
        raise TypeError(
            "document must be a path or an open document, not %r" % (doc,)
        )
    if isinstance(doc, (bytes, bytearray, memoryview)):
        raise TypeError("document bytes are not accepted; pass a file path")
    return CONTINUE


@register_document_rule
def _rule_archive_docs(doc, layout):
    """Sort ZIP and image inputs: EPUB and XPS pass through, Office and
    unknown archives are refused, PNG passes through."""
    global _STASH
    _STASH = None
    if _SUSPEND_DOC_RULES:
        return CONTINUE
    path = None
    if isinstance(doc, str):
        path = doc
    elif isinstance(doc, os.PathLike):
        try:
            path = os.fspath(doc)
        except TypeError:
            return CONTINUE
        if isinstance(path, bytes):
            return CONTINUE
    if path is None:
        return CONTINUE
    try:
        with open(path, "rb") as handle:
            head = handle.read(4)
    except OSError:
        return CONTINUE
    if head.startswith(b"%PDF"):
        return CONTINUE
    if head.startswith(b"\x89PNG"):
        _STASH = ("png", None)
        return CONTINUE
    if head[:2] != b"PK":
        return CONTINUE
    try:
        names = _zip_names(path)
    except zipfile.BadZipFile:
        raise FileDataError("cannot convert %r: not a readable archive" % (path,))
    if _is_epub_container(path, names):
        try:
            _STASH = ("epub", _read_epub_chapters(path))
        except Exception as exc:
            raise FileDataError(
                "cannot convert %r: not a readable EPUB (%s)" % (path, exc)
            )
        return CONTINUE
    if any(name.lower().endswith(".fdseq") for name in names):
        _STASH = ("xps", None)
        return CONTINUE
    raise FileDataError(
        "cannot convert %r: ZIP archives open only as EPUB or XPS" % (path,)
    )


def _zip_names(path):
    """The entry names of the ZIP at `path`."""
    with zipfile.ZipFile(path) as archive:
        return archive.namelist()


def _is_epub_container(path, names):
    """Whether the ZIP at `path` carries an EPUB: the EPUB mimetype, or
    the container document where the mimetype is missing."""
    with zipfile.ZipFile(path) as archive:
        if "mimetype" in names:
            try:
                kind = archive.read("mimetype").decode("ascii", "replace").strip()
            except KeyError:
                kind = ""
            if kind == "application/epub+zip":
                return True
        return "META-INF/container.xml" in names


def _local(tag):
    """An XML tag without its namespace."""
    return tag.rsplit("}", 1)[-1]


def _read_epub_chapters(path):
    """The book at `path` as chapters of blocks, in spine order.

    Each block is ("heading", level, segments), ("para", segments),
    ("item", depth, ordinal, segments), ("link", text, href) or
    ("image", alt, src); a segment is (text, bold, italic, href).
    Tables read as one paragraph per cell."""
    with zipfile.ZipFile(path) as archive:
        container = ElementTree.fromstring(
            archive.read("META-INF/container.xml")
        )
        package_path = None
        for element in container.iter():
            if _local(element.tag) == "rootfile":
                package_path = element.get("full-path")
                if package_path:
                    break
        if not package_path:
            raise ValueError("container names no package document")
        package = ElementTree.fromstring(archive.read(package_path))
        base = package_path.rsplit("/", 1)[0] + "/" if "/" in package_path else ""
        manifest = {}
        for element in package.iter():
            if _local(element.tag) == "item":
                href = element.get("href")
                if element.get("id") and href:
                    manifest[element.get("id")] = _join_href(base, href)
        order = []
        for element in package.iter():
            if _local(element.tag) == "itemref":
                ref = element.get("idref")
                if ref in manifest:
                    order.append(manifest[ref])
        chapters = []
        for href in order:
            try:
                text = archive.read(href).decode("utf-8", "replace")
            except KeyError:
                continue
            chapters.append(_read_epub_chapter(text))
        return chapters


def _join_href(base, href):
    """A manifest href as an archive path, percent escapes decoded."""
    href = href.split("#", 1)[0]
    out = []
    for part in (base + href).split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            if out:
                out.pop()
            continue
        out.append(part)
    try:
        from urllib.parse import unquote

        return unquote("/".join(out))
    except ImportError:  # pragma: no cover - urllib ships with Python
        return "/".join(out)


def _read_epub_chapter(text):
    """One chapter document as blocks."""
    try:
        root = ElementTree.fromstring(text)
    except ElementTree.ParseError as exc:
        raise ValueError("chapter is not XML (%s)" % (exc,))
    body = None
    for element in root.iter():
        if _local(element.tag) == "body":
            body = element
            break
    if body is None:
        body = root
    blocks = []
    _read_epub_flow(body, blocks, 0, [None])
    return blocks


def _read_epub_flow(element, blocks, depth, ordered):
    """A flow element's children as blocks appended to `blocks`."""
    for child in element:
        name = _local(child.tag)
        if name in ("h1", "h2", "h3", "h4", "h5", "h6"):
            segments = _read_epub_inline(child)
            if segments:
                blocks.append(("heading", int(name[1]), segments))
        elif name == "p":
            segments = _read_epub_inline(child)
            if segments:
                blocks.append(("para", segments))
        elif name in ("ul", "ol"):
            ordered.append(0 if name == "ol" else None)
            _read_epub_flow(child, blocks, depth, ordered)
            ordered.pop()
        elif name == "li":
            ordinal = None
            if ordered[-1] is not None:
                ordered[-1] += 1
                ordinal = ordered[-1]
            segments = _read_epub_inline(child)
            if segments:
                blocks.append(("item", depth, ordinal, segments))
            _read_epub_flow(child, blocks, depth + 1, ordered)
        elif name == "a" and child.get("href"):
            text = _segments_text(_read_epub_inline(child))
            blocks.append(("link", text, child.get("href")))
        elif name == "img":
            blocks.append(
                ("image", child.get("alt", "") or "", child.get("src", "") or "")
            )
        elif name == "table":
            for cell in child.iter():
                if _local(cell.tag) in ("td", "th"):
                    segments = _read_epub_inline(cell)
                    if segments:
                        blocks.append(("para", segments))
        elif name in ("br", "hr", "link", "meta"):
            continue
        else:
            _read_epub_flow(child, blocks, depth, ordered)


def _read_epub_inline(element):
    """An element's inline content as segments: (text, bold, italic,
    href). Marks accumulate through nesting; tails read unmarked."""
    segments = []

    def walk(node, bold, italic, href):
        if node.text:
            found = _clean_inline(node.text)
            if found:
                segments.append((found, bold, italic, href))
        for child in node:
            name = _local(child.tag)
            if name in ("b", "strong"):
                walk(child, True, italic, href)
            elif name in ("i", "em"):
                walk(child, bold, True, href)
            elif name == "a":
                walk(child, bold, italic, child.get("href") or href)
            elif name == "img":
                alt = _clean_inline(child.get("alt", "") or "")
                if alt:
                    src = child.get("src", "") or ""
                    segments.append(("![%s](%s)" % (alt, src), False, False, None))
            elif name == "br":
                pass
            elif name in ("ul", "ol"):
                # Nested lists become their own items; only the tail below
                # stays with this segment run.
                pass
            else:
                walk(child, bold, italic, href)
            if child.tail:
                found = _clean_inline(child.tail)
                if found:
                    segments.append((found, False, False, None))

    walk(element, False, False, None)
    return segments


def _clean_inline(text):
    """Inline text with its whitespace runs collapsed to one space."""
    return " ".join(text.split())


def _segments_text(segments):
    """Segments joined with single spaces, unmarked."""
    return " ".join(text for text, _, _, _ in segments)


def _segments_markdown(segments):
    """Segments joined with single spaces, emphasis marked."""
    parts = []
    for text, bold, italic, href in segments:
        if text.startswith("![") and text.endswith(")"):
            parts.append(text)
            continue
        if italic:
            text = "_%s_" % (text,)
        if bold:
            text = "**%s**" % (text,)
        if href:
            text = "[%s](%s)" % (text, href)
        parts.append(text)
    return " ".join(parts)


def _render_epub_markdown(chapters, legacy):
    """Chapters as markdown: `# **`-wrapped headings, `- ` items, every
    block spaced out; legacy without trailing spaces, indented items, and
    a closing blank line."""
    out = []
    for blocks in chapters:
        for block in blocks:
            kind = block[0]
            if kind == "heading":
                _, level, segments = block
                text = "#" * level + " **%s**" % (_segments_markdown(segments),)
            elif kind == "para":
                text = _segments_markdown(block[1])
            elif kind == "item":
                _, depth, ordinal, segments = block
                if legacy:
                    marker = "  %d.  " % (ordinal,) if ordinal else "  - "
                elif ordinal:
                    marker = "  " * depth + "%d.  " % (ordinal,)
                else:
                    marker = "  " * depth + "- "
                text = marker + _segments_markdown(segments)
            elif kind == "link":
                text = "[%s](%s)" % (block[1], block[2])
            elif kind == "image":
                text = "![%s](%s)" % (block[1], block[2])
            else:  # pragma: no cover - the reader makes no other blocks
                continue
            out.append(text + ("\n\n" if legacy else " \n\n"))
    if legacy:
        return "".join(out) + "\n"
    return "".join(out)


def _render_epub_text(chapters):
    """Chapters as plain text: bare headings, `•  ` items, every block
    spaced out."""
    out = []
    for blocks in chapters:
        for block in blocks:
            kind = block[0]
            if kind == "heading":
                out.append(_segments_text(block[2]) + " \n\n")
            elif kind == "para":
                out.append(_segments_text(block[1]) + " \n\n")
            elif kind == "item":
                _, depth, ordinal, segments = block
                if ordinal:
                    marker = "   " * depth + "%d.  " % (ordinal,)
                else:
                    marker = "   " * depth + "\u2022  "
                out.append(marker + _segments_text(segments) + "\n\n")
            elif kind == "link":
                text, href = block[1], block[2]
                out.append("%s (%s) \n\n" % (text, href) if href else text + " \n\n")
            elif kind == "image":
                if block[1]:
                    out.append(block[1] + " \n\n")
    return "".join(out)


#: The batch run log's OCR line: batch runs without OCR options, so the
#: line reads the same for every file.
_BATCH_OCR_LINE = "OCR when needed dropping old OCR text"

#: Bold page markers the batch markdown drops (G155). Unspecified upstream
#: beyond the five observed lines; our behaviour: a line holding only a
#: bold `PAGE i OF n` marker goes, with the blank line that follows it.
_PAGE_MARKER = re.compile(r"(?m)^\*\*PAGE \d+ OF \d+\*\* ?\n\n")


def auto_workers(
    file_count: int, user_dpi: int, ocr_mode: OCRMode, plugin=None
) -> int:
    """How many workers a batch of `file_count` files wants: one per file
    up to eight. The DPI, mode and plugin are accepted and ignored."""
    return max(0, min(file_count, 8))


def convert_batch(
    inputs,
    *,
    workers=None,
    streaming=False,
    out_dir=None,
    backend="md",
    options=None,
    consumer=None,
    consumer_kwargs=None,
    progress_callback=None,
    persistent=False,
):
    """Convert every file in `inputs`, writing one directory per file.

    With `out_dir`, each input lands in `<out_dir>/<stem>/` as
    `<stem>.md` (backend `"md"`) or `<stem>.json` (backend `"json"`)
    next to its `run-log.txt`. Without `out_dir`, nothing converts and a
    stub item per input comes back. `workers`, `streaming` and
    `persistent` are accepted: files convert one after another in input
    order either way. Any call that reaches processing poisons later
    layout conversions in the same process."""
    if isinstance(inputs, (str, bytes, bytearray)):
        raise TypeError("convert_batch inputs must be a list of paths")
    try:
        items = list(inputs)
    except TypeError:
        raise TypeError("convert_batch inputs must be a list of paths") from None
    with _suspend_doc_rules():
        early = check_call(
            "convert_batch",
            None,
            {
                "workers": workers,
                "streaming": streaming,
                "out_dir": out_dir,
                "backend": backend,
                "options": options,
                "consumer": consumer,
                "consumer_kwargs": consumer_kwargs,
                "progress_callback": progress_callback,
                "persistent": persistent,
            },
            layout=is_layout_mode(),
        )
    if early is not CONTINUE:
        return early
    if not items:
        return []
    if not out_dir:
        note_batch_run()
        return [
            BatchResult(_abspath_or_self(source), False, None, None)
            for source in items
        ]
    out_root = os.fspath(out_dir)
    os.makedirs(out_root, exist_ok=True)
    total = len(items)
    results = []
    for number, source in enumerate(items, 1):
        results.append(
            _convert_one(
                source,
                out_root=out_root,
                backend=backend,
                consumer=consumer,
                consumer_kwargs=consumer_kwargs,
            )
        )
        if progress_callback is not None:
            progress_callback(number, total)
    note_batch_run()
    return results


def _abspath_or_self(source):
    """`source` as an absolute path, or itself where it is no path."""
    if isinstance(source, (str, bytes, os.PathLike)):
        try:
            return os.path.abspath(source)
        except (TypeError, ValueError):
            return source
    return source


def _convert_one(source, *, out_root, backend, consumer, consumer_kwargs):
    """Convert one batch input: capture its stdout into the run log,
    write the output file, and report the item."""
    start = datetime.datetime.now().isoformat()
    abspath = _abspath_or_self(source)
    try:
        stem = os.path.splitext(os.path.basename(os.fspath(source)))[0]
    except TypeError:
        stem = str(source)
    if not stem:
        stem = "document"
    target_dir = os.path.join(out_root, stem)
    os.makedirs(target_dir, exist_ok=True)
    captured = io.StringIO()
    try:
        with contextlib.redirect_stdout(captured):
            if consumer is None:
                if backend == "json":
                    from ._chunks import to_json

                    output = to_json(abspath)
                else:
                    output = to_markdown(abspath)
                    output = _PAGE_MARKER.sub("", output)
            else:
                output = convert_document(
                    abspath,
                    backend=backend,
                    options={},
                    consumer=consumer,
                    consumer_kwargs=consumer_kwargs or {},
                )
    except Exception as exc:
        end = datetime.datetime.now().isoformat()
        log = (
            "Start: %s\nOCR mode: %s\nLayout mode: %s\n"
            "Processing document: %s\n%s\nEnd: %s\nStatus: error\n"
            "Exception:\n%s\n" % (start, _BATCH_OCR_LINE,
                                  is_layout_mode(), abspath,
                                  captured.getvalue(), end, exc)
        )
        with open(os.path.join(target_dir, "run-log.txt"), "w",
                  encoding="utf-8", newline="") as handle:
            handle.write(log)
        return BatchResult(abspath, False, str(exc), None)
    end = datetime.datetime.now().isoformat()
    suffix = ".json" if backend == "json" else ".md"
    with open(os.path.join(target_dir, stem + suffix), "w",
              encoding="utf-8", newline="") as handle:
        handle.write(output)
    log = (
        "Start: %s\nOCR mode: %s\nLayout mode: %s\n"
        "Processing document: %s\n%s\nEnd: %s\nStatus: success\n"
        % (start, _BATCH_OCR_LINE, is_layout_mode(), abspath,
           captured.getvalue(), end)
    )
    with open(os.path.join(target_dir, "run-log.txt"), "w",
              encoding="utf-8", newline="") as handle:
        handle.write(log)
    return BatchResult(abspath, True, None, output)


def convert_document(input, *, backend, options, consumer, consumer_kwargs):
    """Convert one file through a batch consumer.

    The consumer runs as `consumer(input, **consumer_kwargs)` and hands
    back the path it wrote; that file's text is the result. Anything else
    is a missing file. Without a consumer or options there is nothing to
    run through, which reads as AttributeError."""
    with _suspend_doc_rules():
        early = check_call(
            "convert_document",
            None,
            {
                "backend": backend,
                "options": options,
                "consumer": consumer,
                "consumer_kwargs": consumer_kwargs,
            },
            layout=is_layout_mode(),
        )
    if early is not CONTINUE:
        return early
    if consumer is None or options is None:
        raise AttributeError(
            "convert_document needs a consumer and options; "
            "it cannot convert standalone"
        )
    result = consumer(input, **dict(consumer_kwargs or {}))
    if (
        isinstance(result, (str, bytes, os.PathLike))
        and os.path.exists(result)
    ):
        with open(result, encoding="utf-8", newline="") as handle:
            return handle.read()
    raise FileNotFoundError("no such file: '%s'" % (result,))


def ProgressBar(list: List[Any], progress_width: int = 40):
    """Wrap `list` in a 40-cell text progress bar, yielding its items.

    Prints `[ forty cells ] (done/total)`, rewriting the bar in place
    with backspaces as items are pulled, and closes the line when the
    last item goes out."""
    total = len(list)
    bar = "[%s] (0/%d)" % (" " * progress_width, total)
    sys.stdout.write(bar)
    sys.stdout.flush()
    filled = 0
    for index, item in enumerate(list, 1):
        new = (progress_width * index) // total
        sys.stdout.write("\x08" * (len(bar) - 1 - filled))
        sys.stdout.write("=" * (new - filled))
        sys.stdout.write("\x08" * (len(bar) - 1))
        bar = "[%s%s] (%d/%d)" % (
            "=" * new,
            " " * (progress_width - new),
            index,
            total,
        )
        sys.stdout.write(bar)
        sys.stdout.flush()
        filled = new
        yield item
    sys.stdout.write("\x08" * len("] (%d/%d)" % (total, total)) + "]\n")
    sys.stdout.flush()


def _default_ocr_function(page, *args, **kwargs):
    """The default page recogniser: recognition runs in the engine, so a
    page-level Python function has nothing to call into."""
    raise NotImplementedError(
        "page-level OCR runs in the engine; pass ocr_function=None to use it"
    )


def select_ocr_function():
    """The default page recogniser. Unspecified upstream beyond being a
    function; our behaviour: recognition runs in the engine, so calling
    the function raises NotImplementedError and pipelines pass None."""
    return _default_ocr_function


def make_ocr_decision(page, use_ocr):
    """Whether `page` wants OCR under `use_ocr`: [attempt, 0, has_text].

    `attempt` reads True unless the mode is NEVER or the page already
    carries usable text; the middle slot reads 0 in every recording.
    Unspecified upstream beyond three recorded triples; our behaviour:
    SELECT modes attempt textless pages, NEVER attempts nothing."""
    has_text = _page_has_usable_text(page)
    attempt = use_ocr != OCRMode.NEVER and not has_text
    return [attempt, 0, has_text]


def _page_has_usable_text(page):
    """Whether the page's own text layer holds anything but whitespace."""
    return bool((page.extract_text() or "").strip())


def analyze_page(page, blocks=None, replace_ocr=False, ocr_dpi=200, stats=None):
    """Score one page for OCR: joins, areas, character counts and the
    recognition score. `replace_ocr`, `ocr_dpi` and `stats` are accepted
    and ignored; analysis renders at its own fixed resolution.

    Pages with usable text skip scoring (`probability` None, `needs_ocr`
    False). Textless pages score their dark ink and its texture: the
    recognition probability runs from 0.90 for inkless pages up, and
    `needs_ocr` reads whether it reaches :data:`OCR_MODEL_THRESHOLD`.
    Unspecified upstream beyond two recorded pages; our behaviour
    throughout."""
    if blocks is None:
        blocks = page.extract_text_blocks()
    blocks = list(blocks)
    chars_total, chars_bad, ocr_spans, has_text, area = _text_stats(blocks)
    page_area = _page_area(page)
    if page_area > 0:
        txt_area = min(1.0, area / page_area)
    else:
        txt_area = 0.0
    if has_text:
        probability = None
        needs_ocr = False
        reason = None
    else:
        gray, _, _ = _page_gray(page)
        probability = predict_ocr_probability(
            {
                "dark_ink": _dark_fraction(gray, 64),
                "dark_spread": _dark_spread(gray),
            }
        )
        needs_ocr = probability >= OCR_MODEL_THRESHOLD
        if needs_ocr:
            reason = (
                "image-only page with dark ink scores above "
                "the recognition threshold"
            )
        else:
            reason = None
    width, height = _page_size(page)
    return {
        "covered": (0.0, 0.0, width, height),
        "img_joins": 0.0,
        "img_area": 0.0,
        "txt_joins": 1.0 if len(blocks) > 1 else 0.0,
        "txt_area": txt_area,
        "vec_joins": 0.0,
        "vec_area": 0.0,
        "chars_total": chars_total,
        "chars_bad": chars_bad,
        "bad_areas": (chars_bad / chars_total) if chars_total else 0.0,
        "ocr_spans": ocr_spans,
        "pixmap": None,
        "vec_norects": 0,
        "needs_ocr": needs_ocr,
        "reason": reason,
        "probability": probability,
    }


def _text_stats(blocks):
    """Character counts, OCR-span count, text presence and estimated text
    area over text blocks (native blocks or dicts)."""
    chars_total = 0
    chars_bad = 0
    ocr_spans = 0
    has_text = False
    area = 0.0
    for block in blocks:
        spans = _block_spans(block)
        if spans:
            texts = [_span_text(span) for span in spans]
            ocr_spans += sum(1 for span in spans if is_ocr_span(span))
            for span in spans:
                box = _span_box(span)
                if box is not None:
                    text, size = box
                    area += len(text) * 0.5 * size * size
        else:
            texts = [_block_text(block)]
        for text in texts:
            chars_total += len(text)
            chars_bad += sum(1 for char in text if _is_bad_char(char))
            if text.strip():
                has_text = True
    return chars_total, chars_bad, ocr_spans, has_text, area


def _block_text(block):
    """A text block's text, native or dict."""
    if isinstance(block, dict):
        return str(block.get("text", "") or "")
    return str(getattr(block, "text", "") or "")


def _block_spans(block):
    """A text block's spans, native or dict; the block itself where it is
    span-shaped but holds no spans."""
    if isinstance(block, dict):
        spans = block.get("spans") or []
    else:
        spans = getattr(block, "spans", None) or []
    spans = list(spans)
    if not spans and _block_text(block):
        return [block]
    return spans


def _span_text(span):
    """A span's text, native or dict."""
    if isinstance(span, dict):
        return str(span.get("text", "") or "")
    return str(getattr(span, "text", "") or "")


def _span_box(span):
    """A span as (text, size) for the area estimate, or None where the
    span states no size."""
    if isinstance(span, dict):
        text = str(span.get("text", "") or "")
        size = span.get("font_size", span.get("size"))
    else:
        text = str(getattr(span, "text", "") or "")
        size = getattr(span, "font_size", None)
    try:
        size = float(size)
    except (TypeError, ValueError):
        return None
    if size <= 0:
        return None
    return text, size


def _is_bad_char(char):
    """Whether a character reads as garbled output: the replacement
    character, a private-use character, or a bare control."""
    point = ord(char)
    if char == REPLACEMENT_CHARACTER:
        return True
    if 0xE000 <= point <= 0xF8FF:
        return True
    return point < 32 and char not in "\t\n\r"


def _page_size(page):
    """The page size as floats, or zeros where the page states none."""
    try:
        return float(page.width), float(page.height)
    except (AttributeError, TypeError, ValueError):
        return 0.0, 0.0


def _page_area(page):
    """The page area, or 0 where the page states no size."""
    width, height = _page_size(page)
    return width * height


def get_layout_locked(page, **kwargs):
    """The layout lock state for `page`: always None. Unspecified
    upstream beyond the None recording; our behaviour: no lock."""
    return None


def is_ocr_text(span):
    """Whether the span was written by the OCR layer, like
    :func:`is_ocr_span`: its font is the glyphless one."""
    return is_ocr_span(span)


def is_ocr_span(span):
    """Whether the span was written by the OCR layer: its font is the
    glyphless one."""
    if isinstance(span, dict):
        font = span.get("font_name", span.get("font"))
    else:
        font = getattr(span, "font_name", getattr(span, "font", None))
    return font == TESSERACT_FONT_NAME


def compute_features(blocks, page_rect, page):
    """Scoreable features for one page: text counts plus edge and ink
    densities from a grey render. `page_rect` is accepted and ignored;
    densities normalize themselves. Keys follow :data:`FEATURE_NAMES`."""
    blocks = list(blocks)
    chars_total, _, _, _, _ = _text_stats(blocks)
    gray, _, _ = _page_gray(page)
    edges = sobel_features(gray)
    return {
        "text_chars": chars_total,
        "text_blocks": len(blocks),
        "edge_density": edges["edge_density"],
        "mean_magnitude": edges["mean_magnitude"],
        "dark_fraction": _dark_fraction(gray, 128),
        "dark_ink": _dark_fraction(gray, 64),
        "dark_spread": _dark_spread(gray),
    }


def _page_gray(page, dpi=100):
    """The page rendered at `dpi` as nested grey rows (0-255) with its
    pixel size."""
    image = page.render(dpi=dpi)
    width, height = image.width, image.height
    pixels = bytes(image.pixels)
    gray = []
    for row in range(height):
        line = []
        base = row * width * 4
        for column in range(width):
            at = base + column * 4
            red, green, blue = pixels[at], pixels[at + 1], pixels[at + 2]
            line.append((77 * red + 150 * green + 29 * blue) >> 8)
        gray.append(line)
    return gray, width, height


def _dark_fraction(gray, below):
    """The fraction of grey pixels darker than `below`."""
    total = 0
    dark = 0
    for line in gray:
        for value in line:
            total += 1
            if value < below:
                dark += 1
    if not total:
        return 0.0
    return dark / total


def _dark_spread(gray):
    """How thinly the page's dark pixels scatter across grey levels: 1
    where dark pixels spread out (text antialiasing), 0 where they bunch
    into one flat level or hardly exist."""
    total = 0
    levels = {}
    for line in gray:
        for value in line:
            total += 1
            if value < 200:
                levels[value] = levels.get(value, 0) + 1
    if not total:
        return 0.0
    if sum(levels.values()) / total < 0.0001:
        return 0.0
    peak = max(levels.values()) / total
    return 1.0 - min(1.0, peak / 0.05)


def conv2d_fast(img, kernel):
    """Convolve grey rows `img` with `kernel`, valid mode: the plain
    double loop over nested sequences, no strided tricks."""
    rows = [list(line) for line in img]
    mask = [list(line) for line in kernel]
    if not rows or not rows[0] or not mask or not mask[0]:
        return []
    height, width = len(rows), len(rows[0])
    kh, kw = len(mask), len(mask[0])
    if kh > height or kw > width:
        return []
    out = []
    for row in range(height - kh + 1):
        line = []
        for column in range(width - kw + 1):
            total = 0.0
            for ky in range(kh):
                source = rows[row + ky]
                weights = mask[ky]
                for kx in range(kw):
                    total += source[column + kx] * weights[kx]
            line.append(total)
        out.append(line)
    return out


def sobel_features(gray):
    """Edge summary of grey rows: the fraction of pixels whose Sobel
    magnitude tops 100, and the mean magnitude."""
    rows = [list(line) for line in gray]
    if not rows or not rows[0]:
        return {"edge_density": 0.0, "mean_magnitude": 0.0}
    gx = conv2d_fast(rows, SOBEL_X)
    gy = conv2d_fast(rows, SOBEL_Y)
    total = 0
    edges = 0
    magnitude = 0.0
    for ax, ay in zip(gx, gy):
        for x, y in zip(ax, ay):
            total += 1
            cell = abs(x) + abs(y)
            magnitude += cell
            if cell > 100.0:
                edges += 1
    if not total:
        return {"edge_density": 0.0, "mean_magnitude": 0.0}
    return {"edge_density": edges / total, "mean_magnitude": magnitude / total}


def sobel_features_page(page):
    """Edge summary of a page's grey render, like :func:`sobel_features`."""
    gray, _, _ = _page_gray(page)
    return sobel_features(gray)


def predict_ocr_probability(features: dict) -> float:
    """Recognition probability from a feature dict: 0.90 for pages
    without dark ink, rising with the ink fraction and its texture to
    0.99. Unspecified upstream beyond one recorded score; our behaviour:
    pages whose ink reads at or above :data:`OCR_MODEL_THRESHOLD` want
    OCR."""
    try:
        dark = max(0.0, float(features.get("dark_ink", 0.0)))
    except (TypeError, ValueError):
        dark = 0.0
    try:
        spread = min(1.0, max(0.0, float(features.get("dark_spread", 0.0))))
    except (TypeError, ValueError):
        spread = 0.0
    return 0.90 + min(0.09, max(dark * 300.0, spread * 0.09))


def check_images(image_blocks, prob, threshold=0.93):
    """Confirm an OCR decision against image blocks: True where blocks
    exist and `prob` clears `threshold`. Unspecified upstream beyond the
    signature; our behaviour: both must hold."""
    return bool(image_blocks) and float(prob) >= float(threshold)


def as_strided(x, shape=None, strides=None, subok=False, writeable=True):
    """Flat sequence `x` reshaped to nested lists along `shape` and
    element `strides` (row-major where omitted), bounds-checked. A copy,
    not a view; `subok` and `writeable` are accepted and ignored."""
    flat = list(x)
    if shape is None:
        return flat
    shape = [int(dim) for dim in shape]
    if strides is None:
        strides = []
        stride = 1
        for dim in reversed(shape):
            strides.insert(0, stride)
            stride *= dim
    else:
        strides = [int(item) for item in strides]
    if len(shape) != len(strides):
        raise ValueError("shape and strides must pair up")
    if any(dim < 0 for dim in shape) or any(item < 0 for item in strides):
        raise ValueError("shape and strides must not go negative")
    needed = 1 + sum((dim - 1) * item for dim, item in zip(shape, strides))
    if needed > len(flat):
        raise ValueError("shape and strides run past the input")

    def build(depth, base):
        if depth == len(shape):
            return flat[base]
        return [
            build(depth + 1, base + index * strides[depth])
            for index in range(shape[depth])
        ]

    return build(0, 0)


def _rule_batch_args(options, layout):
    """Refuse unknown batch backends and any batch options: only "md"
    and "json" convert, and only without options."""
    if options.get("backend") not in ("md", "json"):
        raise ValueError(
            "convert_batch backend must be 'md' or 'json', not %r"
            % (options.get("backend"),)
        )
    if options.get("options"):
        raise ValueError("convert_batch takes no options")
    return CONTINUE


def _rule_batch_poison(options, layout):
    """Refuse layout conversions after a batch run poisoned them."""
    if layout and is_batch_poisoned():
        raise ValueError(
            "layout conversion is unavailable after convert_batch in this "
            "process; switch to legacy mode or start a fresh process"
        )
    return CONTINUE


def _rule_ocr_function(options, layout):
    """Refuse an unrelated recogniser where forced OCR would call it:
    only None (the default recogniser) converts."""
    if not layout:
        return CONTINUE
    function = options.get("ocr_function")
    if function is None or function is _default_ocr_function:
        return CONTINUE
    forced = bool(options.get("force_ocr")) or options.get("use_ocr") in (
        OCRMode.FORCE_DROP_OLD,
        OCRMode.FORCE_KEEP_OLD,
    )
    if forced:
        raise TypeError(
            "unrelated ocr_function %r: pass None for the default recogniser"
            % (function,)
        )
    return CONTINUE


def _rule_nonpdf_markdown(options, layout):
    """Render the stashed non-PDF input as markdown: EPUB chapters,
    empty images, and the XPS blank."""
    if _STASH is None:
        return CONTINUE
    kind, payload = _STASH
    if kind == "png":
        return ""
    if kind == "xps":
        return "\n\n" if layout else ""
    return _render_epub_markdown(payload, legacy=not layout)


def _rule_nonpdf_text(options, layout):
    """Render the stashed non-PDF input as text, layout only: EPUB
    chapters, empty images, and the XPS blank."""
    if _STASH is None or not layout:
        return CONTINUE
    kind, payload = _STASH
    if kind == "png":
        return ""
    if kind == "xps":
        return ""
    return _render_epub_text(payload)


for _entry in ("to_markdown", "to_text", "to_json", "parse_document"):
    register_option_rule(_entry, _rule_batch_poison)
    register_option_rule(_entry, _rule_ocr_function)
register_option_rule("to_markdown", _rule_nonpdf_markdown)
register_option_rule("to_text", _rule_nonpdf_text)
register_option_rule("convert_batch", _rule_batch_args)
