# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# Lane L4: SPEC sections 7, 8, 9.
"""Image filtering, orphan attachment and markdown path helpers.

Call protocol (our contract; the observed signatures carry no types):

- ``page`` is anything sized in points with ``width`` and ``height``
  attributes; native pages qualify. Where image boxes are needed the page
  may also offer ``image_boxes``, called with no arguments.
- ``parms`` is the page's image records: a list or tuple of dicts holding
  at least ``bbox`` (``[x0, y0, x1, y1]`` in points), or an object with an
  ``images`` attribute holding such a list. Anything else is a ``TypeError``
  naming this contract.
- ``blocks`` are layout dicts with ``bbox`` and ``boxclass`` keys; objects
  with the same attribute names read alike.

Size rule (G82): an image is kept when its smaller relative side reaches
the limit, i.e. ``min(w/W, h/H) >= limit`` for image size ``(w, h)`` on a
page of ``(W, H)``. Limit 0 keeps everything (G79); a limit of 1 or more
is refused (G81).
"""
from __future__ import annotations

from ._errors import CONTINUE, register_option_rule

#: The legacy converter's default size limit (G80).
DEFAULT_IMAGE_SIZE_LIMIT = 0.05


def _check_size_limit(limit):
    """Refuse a size limit no fraction can reach."""
    if isinstance(limit, (int, float)) and limit >= 1:
        raise ValueError("image_size_limit must be below 1, got %r" % (limit,))
    return limit


def _page_size(page):
    """Return ``(width, height)`` of a page in points."""
    width = getattr(page, "width", None)
    height = getattr(page, "height", None)
    if width is None or height is None:
        rect = getattr(page, "rect", None)
        if rect is not None:
            width, height = rect[2] - rect[0], rect[3] - rect[1]
    if width is None or height is None:
        raise TypeError(
            "page must carry width/height in points, got %r" % (page,)
        )
    return float(width), float(height)


def _record_box(record):
    """Return the ``(x0, y0, x1, y1)`` floats of one image record."""
    if isinstance(record, dict):
        box = record.get("bbox")
    else:
        box = getattr(record, "bbox", None)
    if box is None:
        raise TypeError("image record must carry bbox, got %r" % (record,))
    try:
        x0, y0, x1, y1 = (float(v) for v in box)
    except (TypeError, ValueError):
        raise TypeError(
            "image bbox must hold four numbers, got %r" % (box,)
        ) from None
    return x0, y0, x1, y1


def _page_boxes(page):
    """Return the image records the page itself offers, if any."""
    boxes = getattr(page, "image_boxes", None)
    if boxes is None:
        held = getattr(page, "images", None)
        if isinstance(held, (list, tuple)):
            return list(held)
        return []
    listed = boxes() if callable(boxes) else boxes
    return [{"bbox": list(box)} for box in listed]


def _page_images(page, parms):
    """Return the image records ``parms`` holds for ``page``."""
    if isinstance(parms, (list, tuple)):
        return list(parms)
    held = getattr(parms, "images", None)
    if isinstance(held, (list, tuple)):
        return list(held)
    offered = _page_boxes(page)
    if offered:
        return offered
    raise TypeError(
        "parms must be image records or hold .images, got %r" % (parms,)
    )


def _relative_min(box, size):
    """The smaller relative side of an image box on its page."""
    x0, y0, x1, y1 = box
    width, height = size
    if width <= 0 or height <= 0:
        return 0.0
    w = max(0.0, x1 - x0)
    h = max(0.0, y1 - y0)
    return min(w / width, h / height)


def _kept(box, size, limit):
    """Whether the image box clears the size limit on its page."""
    if limit is not None and limit <= 0:
        return True
    return _relative_min(box, size) >= limit


def extract_images_on_page_simple(page, parms, image_size_limit):
    """Collect one page's images above the size limit.

    Every record comes back as a dict carrying its fields plus ``keep``,
    true where the record clears ``image_size_limit`` under the module
    size rule. A limit of 1 or more raises ``ValueError``.
    """
    _check_size_limit(image_size_limit)
    size = _page_size(page)
    out = []
    for record in _page_images(page, parms):
        box = _record_box(record)
        keep = _kept(box, size, image_size_limit)
        if isinstance(record, dict):
            marked = dict(record)
            marked["keep"] = keep
        else:
            marked = {"bbox": list(box), "keep": keep}
        out.append(marked)
    return out


def extract_images_on_page_simple_drop(page, parms, image_size_limit):
    """Collect one page's images, dropping the ones below the limit.

    Unspecified upstream; our behaviour: the kept records of
    :func:`extract_images_on_page_simple`, in order.
    """
    return [
        record
        for record in extract_images_on_page_simple(
            page, parms, image_size_limit
        )
        if record["keep"]
    ]


def filter_small_images(page, parms, image_size_limit):
    """Keep only the images that clear the size limit.

    Returns the kept records as given, in order; the input is never
    mutated. A limit of 1 or more raises ``ValueError``.
    """
    _check_size_limit(image_size_limit)
    size = _page_size(page)
    return [
        record
        for record in _page_images(page, parms)
        if _kept(_record_box(record), size, image_size_limit)
    ]


def _block_box(block):
    """Return the bbox floats of a layout block, or None when malformed."""
    if isinstance(block, dict):
        box = block.get("bbox")
    else:
        box = getattr(block, "bbox", None)
    if box is None:
        return None
    try:
        x0, y0, x1, y1 = (float(v) for v in box)
    except (TypeError, ValueError):
        return None
    return (x0, y0, x1, y1)


def _block_field(block, key, default=None):
    """Read one field off a dict block or a block-like object."""
    if isinstance(block, dict):
        return block.get(key, default)
    return getattr(block, key, default)


def _overlap_area(a, b):
    """The intersection area of two bbox tuples."""
    w = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    h = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    return w * h


def add_image_orphans(page, blocks):
    """Attach images that no text block claimed.

    Unspecified upstream; our behaviour: an image box less than half
    covered by non-picture blocks is appended as a ``picture`` block with
    no image yet. Blocks already listed pass through untouched, and the
    input list is never mutated.
    """
    kept = list(blocks)
    images = _page_boxes(page)
    covered = [
        box
        for block in kept
        if _block_field(block, "boxclass") != "picture"
        for box in [_block_box(block)]
        if box is not None
    ]
    for record in images:
        try:
            box = _record_box(record)
        except TypeError:
            continue
        area = max(0.0, box[2] - box[0]) * max(0.0, box[3] - box[1])
        if area <= 0:
            continue
        claimed = sum(_overlap_area(box, other) for other in covered)
        if claimed < 0.5 * area:
            kept.append(
                {"bbox": list(box), "boxclass": "picture", "image": None}
            )
    return kept


def clean_pictures(page, blocks):
    """Prune picture blocks that carry no usable image.

    Unspecified upstream; our behaviour: a ``picture`` block whose image
    is missing or empty goes, everything else stays. The page argument is
    the pipeline's convenience and takes no part in the rule.
    """
    del page
    return [
        block
        for block in blocks
        if _block_field(block, "boxclass") != "picture"
        or _block_field(block, "image")
    ]


def image_reference(folder, filename):
    """Join a folder and file name for a markdown image reference."""
    if not folder:
        return filename
    if folder.endswith("/"):
        return folder + filename
    return folder + "/" + filename


def md_path(folder: str, filename: str):
    """Join a folder and file name for a markdown image reference.

    The joined path comes back twice as a 2-list (G209). Unspecified
    upstream; our behaviour: the join always uses a forward slash, so the
    recorded references match on every platform.
    """
    joined = image_reference(folder, filename)
    return [joined, joined]


def layout_image_name(base, page, index, ext="png"):
    """Name one written file in layout style (G68).

    ``page`` and ``index`` count from zero; both leave counted from one,
    the page on four digits and the index on two.
    """
    return "%s-%04d-%02d.%s" % (base, page + 1, index + 1, ext)


def legacy_image_name(base, page, index, ext="png"):
    """Name one written file in legacy style (G69).

    ``page`` and ``index`` count from zero and stay that way; the index
    counts kept images, restarting at each page (G83).
    """
    return "%s-%d-%d.%s" % (base, page, index, ext)


def image_extension(image_format):
    """Map an image format option onto a file extension.

    ``jpg`` and ``jpeg`` write ``.jpg`` (G71); anything else -- including
    a bogus format, which without writing is silently ignored (G72) --
    writes ``.png``. Unspecified upstream; our behaviour: the bogus format
    falls back to ``png`` where files are written.
    """
    if image_format in ("jpg", "jpeg"):
        return "jpg"
    return "png"


def image_base_name(path, filename=None):
    """Return the base name image files are built on.

    An explicit filename wins as given (G70); otherwise the input path's
    own base name, extension included.
    """
    if filename is not None:
        return filename
    text = str(path).replace("\\", "/")
    return text.rsplit("/", 1)[-1]


def _legacy_image_option_rule(options, layout):
    """Refuse legacy image options the release refuses (G81, G91)."""
    if layout:
        return CONTINUE
    limit = options.get("image_size_limit", DEFAULT_IMAGE_SIZE_LIMIT)
    if isinstance(limit, (int, float)) and limit >= 1:
        raise ValueError("image_size_limit must be below 1, got %r" % (limit,))
    if not options.get("force_text", True):
        raise ValueError(
            "force_text=False is not supported by the legacy converter"
        )
    return CONTINUE


register_option_rule("to_markdown", _legacy_image_option_rule)


__all__ = [
    "DEFAULT_IMAGE_SIZE_LIMIT",
    "add_image_orphans",
    "clean_pictures",
    "extract_images_on_page_simple",
    "extract_images_on_page_simple_drop",
    "filter_small_images",
    "image_base_name",
    "image_extension",
    "image_reference",
    "layout_image_name",
    "legacy_image_name",
    "md_path",
]
