# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Plain known-images on the shared construction fixture (#643).

``Document.page_images`` plus ``Document.decode_image_pixels`` over
fixtures/known-images.pdf, checked against
fixtures/known-images.expected.json: count, dimensions, colour space,
components, bpc, and the decoded-pixel sha256. ``extract_image_data``
rides along to pin the encoded-vs-pixels split (PNG bytes, not samples).
"""

from __future__ import annotations

import hashlib
import json
import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

Document = pdfluent.Document

HERE = os.path.dirname(__file__)
FIXTURES = os.path.normpath(os.path.join(HERE, "..", "..", "..", "fixtures"))
PDF = os.path.join(FIXTURES, "known-images.pdf")
EXPECTED = os.path.join(FIXTURES, "known-images.expected.json")


def _expected() -> list[dict]:
    with open(EXPECTED, encoding="utf-8") as handle:
        images = json.load(handle)["images"]
    assert len(images) == 2
    return sorted(images, key=lambda w: (w["width"], w["height"]))


def _norm_space(name: object) -> str:
    text = str(name).lower().replace("device", "").replace("grey", "gray")
    return "".join(ch for ch in text if ch.isalnum())


def test_page_images_lists_the_shared_fixture() -> None:
    want = _expected()
    with Document(PDF) as doc:
        images = doc.page_images(0)
    assert len(images) == len(want) == 2
    got = sorted(images, key=lambda i: (i["width"], i["height"]))
    for info, w in zip(got, want):
        assert info["width"] == w["width"]
        assert info["height"] == w["height"]
        assert _norm_space(info["color_space"]) == _norm_space(w["colorspace"])
        assert info["components"] == w["components"]
        assert info["bits_per_component"] == w["bpc"]


def test_decode_image_pixels_matches_the_expected_sha() -> None:
    want = _expected()
    by_size = {(w["width"], w["height"]): w for w in want}
    with Document(PDF) as doc:
        images = doc.page_images(0)
        assert len(images) == 2
        for info in images:
            decoded = doc.decode_image_pixels(info["id"])
            w = by_size[(info["width"], info["height"])]
            assert decoded["width"] == w["width"]
            assert decoded["height"] == w["height"]
            assert decoded["components"] == w["components"]
            assert _norm_space(decoded["color_space"]) == _norm_space(w["colorspace"])
            pixels = bytes(decoded["pixels"])
            assert len(pixels) == w["width"] * w["height"] * w["components"]
            assert hashlib.sha256(pixels).hexdigest() == w["sha256"]


def test_extract_image_data_is_png_not_pixels() -> None:
    with Document(PDF) as doc:
        images = doc.page_images(0)
        assert len(images) == 2
        for info in images:
            extracted = doc.extract_image_data(info["id"])
            assert extracted["encoding"] == "png"
            assert bytes(extracted["data"])[:4] == b"\x89PNG"
            decoded = doc.decode_image_pixels(info["id"])
            assert len(bytes(extracted["data"])) != len(bytes(decoded["pixels"]))
