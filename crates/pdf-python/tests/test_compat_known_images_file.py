# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Compat known-images as encoded files, decoded independently (#636).

``Document.extract_image`` over fixtures/known-images.pdf returns the image
FILE (header facts plus encoded bytes), checked against
fixtures/known-images.expected.json: ``ext`` matches the payload's magic
bytes, and the payload decoded with pypng (third party, MIT, not PDFluent)
yields the expected dimensions, components, and decoded-pixel sha256. A
lossy (JPEG) payload cannot match a pixel sha by construction, so it pins
only magic bytes, dimensions, and components; its size comes from the JPEG
Start Of Frame segment, parsed with the standard library.
"""

from __future__ import annotations

import hashlib
import json
import os

import pytest

pdfluent = pytest.importorskip("pdfluent")
png = pytest.importorskip("png")

HERE = os.path.dirname(__file__)
FIXTURES = os.path.normpath(os.path.join(HERE, "..", "..", "..", "fixtures"))
PDF = os.path.join(FIXTURES, "known-images.pdf")
EXPECTED = os.path.join(FIXTURES, "known-images.expected.json")

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
JPEG_MAGIC = b"\xff\xd8\xff"


def _expected() -> list[dict]:
    with open(EXPECTED, encoding="utf-8") as handle:
        images = json.load(handle)["images"]
    assert len(images) == 2
    return sorted(images, key=lambda w: (w["width"], w["height"]))


def _norm_space(name: object) -> str:
    text = str(name).lower().replace("device", "").replace("grey", "gray")
    return "".join(ch for ch in text if ch.isalnum())


def _open_compat(path: str):
    opener = getattr(pdfluent, "open", None)
    assert callable(opener), "compat open() missing"
    return opener(path)


def _png_samples(data: bytes) -> tuple[int, int, int, bytes]:
    """Decode PNG bytes to row-major interleaved 8-bit samples, no alpha.

    Returns ``(width, height, components, samples)``. A palette maps through
    its entries to RGB (a palette alpha entry is dropped); a direct alpha
    channel is stripped; 16-bit samples keep the high byte; sub-8-bit grey
    scales to full range. All match the expected no-alpha 8-bit layout.
    """
    width, height, rows, info = png.Reader(bytes=data).read()
    bitdepth = info["bitdepth"]
    palette = info.get("palette")

    def scale(sample: int) -> int:
        if bitdepth >= 8:
            return sample >> (bitdepth - 8)
        return sample * 255 // ((1 << bitdepth) - 1)

    if palette is not None:
        rgb = [tuple(entry[:3]) for entry in palette]
        out = bytearray()
        for row in rows:
            for index in row:
                out.extend(rgb[index])
        return width, height, 3, bytes(out)
    if info["greyscale"]:
        step = 2 if info["alpha"] else 1
        out = bytearray()
        for row in rows:
            out.extend(scale(v) for v in row[::step])
        return width, height, 1, bytes(out)
    step = 4 if info["alpha"] else 3
    out = bytearray()
    for row in rows:
        for pos in range(0, len(row), step):
            out.extend(scale(v) for v in row[pos : pos + 3])
    return width, height, 3, bytes(out)


def _jpeg_frame(data: bytes) -> tuple[int, int, int]:
    """Scan JPEG markers for the first Start Of Frame.

    Returns ``(width, height, components)`` from the SOF segment, so the
    JPEG branch needs no image library.
    """
    assert data[:2] == b"\xff\xd8", "SOI marker"
    pos = 2
    while pos + 4 <= len(data):
        assert data[pos] == 0xFF, "marker prefix at offset %d" % pos
        marker = data[pos + 1]
        pos += 2
        if marker == 0xD8 or marker == 0x01 or 0xD0 <= marker <= 0xD7:
            continue  # SOI, TEM, RSTn: standalone, no payload
        assert marker not in (0xD9, 0xDA), "reached SOS/EOI without SOF"
        length = int.from_bytes(data[pos : pos + 2], "big")
        assert length >= 2, "segment length"
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            assert pos + 8 <= len(data), "SOF segment truncated"
            height = int.from_bytes(data[pos + 3 : pos + 5], "big")
            width = int.from_bytes(data[pos + 5 : pos + 7], "big")
            return width, height, data[pos + 7]
        pos += length
    raise AssertionError("no SOF segment found")


def test_extract_image_returns_the_expected_encoded_file() -> None:
    want = _expected()
    by_size = {(w["width"], w["height"]): w for w in want}
    doc = _open_compat(PDF)
    try:
        refs = doc.get_page_images(0)
        assert len(refs) == 2
        for ref in refs:
            xref, _mask, width, height = ref[0], ref[1], ref[2], ref[3]
            w = by_size[(width, height)]
            payload = doc.extract_image(xref)
            ext = payload["ext"]
            assert ext in ("png", "jpeg"), "ext is a file kind, got %r" % (ext,)
            assert payload["width"] == w["width"]
            assert payload["height"] == w["height"]
            assert payload["colorspace"] == w["components"]
            assert _norm_space(payload["cs-name"]) == _norm_space(w["colorspace"])
            assert payload["bpc"] == w["bpc"]
            data = bytes(payload["image"])
            if ext == "png":
                assert data[:8] == PNG_MAGIC, "png ext matches PNG magic"
                dec_width, dec_height, dec_comps, samples = _png_samples(data)
                assert (dec_width, dec_height) == (w["width"], w["height"])
                assert dec_comps == w["components"]
                assert len(samples) == w["width"] * w["height"] * w["components"]
                assert hashlib.sha256(samples).hexdigest() == w["sha256"]
            else:
                assert data[:3] == JPEG_MAGIC, "jpeg ext matches JPEG magic"
                frame_width, frame_height, frame_comps = _jpeg_frame(data)
                assert (frame_width, frame_height) == (w["width"], w["height"])
                assert frame_comps == w["components"]
    finally:
        close = getattr(doc, "close", None)
        if callable(close):
            close()
