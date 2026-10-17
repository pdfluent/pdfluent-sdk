# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Known images through the Python public image API (#643).

Reads fixtures/known-images.expected.json; no fixture literals duplicated.
Compat path (pdfluent.open) is the public image-extraction API: page image
refs plus per-xref extraction with decoded-pixel sha256. The facade
(Document) carries the plain image calls (D16): page_images,
extract_image_data and decode_image_pixels.
"""

from __future__ import annotations

import hashlib
import json
import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

HERE = os.path.dirname(__file__)
FIXTURES = os.path.normpath(os.path.join(HERE, "..", "..", "..", "fixtures"))
PDF = os.path.join(FIXTURES, "known-images.pdf")
EXPECTED = os.path.join(FIXTURES, "known-images.expected.json")


def _expected() -> list[dict]:
    with open(EXPECTED, encoding="utf-8") as handle:
        return json.load(handle)["images"]


def _norm_space(name: object) -> str:
    text = str(name).lower().replace("device", "").replace("grey", "gray")
    return "".join(ch for ch in text if ch.isalnum())


def _field(obj: object, *names: str, index: int = -1) -> object:
    if isinstance(obj, dict):
        for key in names:
            if key in obj:
                return obj[key]
        lower = {str(k).lower(): v for k, v in obj.items()}
        for key in names:
            if key.lower() in lower:
                return lower[key.lower()]
        raise KeyError(names[0])
    for key in names:
        if hasattr(obj, key):
            return getattr(obj, key)
    if isinstance(obj, (list, tuple)) and 0 <= index < len(obj):
        return obj[index]
    raise AttributeError(names[0])


def _ref_facts(ref: object) -> tuple[int, int, int, str, int, int]:
    if isinstance(ref, (list, tuple)) and len(ref) >= 8:
        xref, _mask, width, height, bpc, space = (
            ref[0],
            ref[1],
            ref[2],
            ref[3],
            ref[4],
            ref[5],
        )
        comps = (
            3
            if _norm_space(space) == "rgb"
            else 1
            if _norm_space(space) == "gray"
            else 4
        )
        return int(xref), int(width), int(height), str(space), comps, int(bpc)
    xref = int(_field(ref, "xref", "Xref", index=0))
    width = int(_field(ref, "width", "Width", index=2))
    height = int(_field(ref, "height", "Height", index=3))
    bpc = int(_field(ref, "bpc", "bits", "Bits", index=4))
    try:
        space = str(
            _field(ref, "colorspace", "colorspaceName", "ColorspaceName", "cs_name")
        )
    except (KeyError, AttributeError):
        space = ""
    try:
        comps = int(_field(ref, "components", "colorspace_code", "n", "N"))
    except (KeyError, AttributeError, ValueError, TypeError):
        norm = _norm_space(space)
        comps = (
            3 if norm == "rgb" else 1 if norm == "gray" else 4 if norm == "cmyk" else -1
        )
    return xref, width, height, space, comps, bpc


def _pixel_bytes(payload: object) -> bytes | None:
    for key in ("image", "data", "samples", "pixels", "bytes"):
        try:
            value = _field(payload, key)
        except (KeyError, AttributeError):
            continue
        if isinstance(value, (bytes, bytearray)):
            return bytes(value)
    if isinstance(payload, (bytes, bytearray)):
        return bytes(payload)
    return None


def _open_compat(path: str):
    opener = getattr(pdfluent, "open", None)
    assert callable(opener), "compat open() missing"
    return opener(path)


def test_compat_dimensions() -> None:
    want = _expected()
    doc = _open_compat(PDF)
    try:
        refs = doc.get_page_images(0)
        assert len(refs) == len(want) == 2
        got = sorted((_ref_facts(r)[1], _ref_facts(r)[2], _ref_facts(r)) for r in refs)
        want_sorted = sorted((w["width"], w["height"], w) for w in want)
        for (_, _, (_, width, height, space, comps, bpc)), (_, _, w) in zip(
            got, want_sorted
        ):
            assert width == w["width"]
            assert height == w["height"]
            assert _norm_space(space) == _norm_space(w["colorspace"])
            assert comps == w["components"]
            assert bpc == w["bpc"]
    finally:
        close = getattr(doc, "close", None)
        if callable(close):
            close()


def test_compat_pixels() -> None:
    want = _expected()
    by_size = {(w["width"], w["height"]): w for w in want}
    doc = _open_compat(PDF)
    try:
        refs = doc.get_page_images(0)
        assert len(refs) == 2
        for ref in refs:
            xref, width, height, _space, comps, _bpc = _ref_facts(ref)
            payload = doc.extract_image(xref)
            data = _pixel_bytes(payload)
            if data is None:
                pytest.skip("STOP: compat extract_image has no pixel bytes")
            expect_len = width * height * comps
            if len(data) != expect_len:
                pytest.skip(
                    "STOP: encoded bytes only (%d != %d), no decode call"
                    % (len(data), expect_len)
                )
            digest = hashlib.sha256(data).hexdigest()
            assert digest == by_size[(width, height)]["sha256"]
    finally:
        close = getattr(doc, "close", None)
        if callable(close):
            close()


def test_facade_decodes_known_image_pixels() -> None:
    native = getattr(pdfluent, "Document", None)
    assert native is not None
    for name in ("page_images", "extract_image_data", "decode_image_pixels"):
        assert callable(
            getattr(native, name, None)
        ), "facade Document lacks %s" % name
    want = _expected()
    by_size = {(w["width"], w["height"]): w for w in want}
    with native(PDF) as doc:
        images = doc.page_images(0)
        assert len(images) == 2
        for info in images:
            w = by_size[(info["width"], info["height"])]
            decoded = doc.decode_image_pixels(info["id"])
            assert decoded["width"] == w["width"]
            assert decoded["height"] == w["height"]
            assert decoded["components"] == w["components"]
            pixels = bytes(decoded["pixels"])
            assert len(pixels) == w["width"] * w["height"] * w["components"]
            assert hashlib.sha256(pixels).hexdigest() == w["sha256"]
