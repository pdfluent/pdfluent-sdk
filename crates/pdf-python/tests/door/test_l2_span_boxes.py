# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L2: span boxes for embedded fonts through the public raw-lines path."""

from contextlib import contextmanager

import pytest
from conftest import FIX, SKIP_REASON, SPEC_MISSING

from pdfluent_extract.helpers import legacy

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]


@contextmanager
def _page(fixture, number=0):
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for page input"
    )
    with pdfluent.Document(str(FIX(fixture))) as doc:
        yield doc[number]


def _spans(pairs):
    return [span for _bbox, spans in pairs for span in spans]


def test_g_span_boxes_wired_for_embedded_fonts():
    with _page("code-snippet.pdf") as page:
        spans = _spans(legacy.get_raw_lines(textpage=page))
    mono = [s for s in spans if s["font"] == "LiberationMono"]
    assert mono, "expected embedded-font spans on the page"
    for span in mono:
        x0, top, x1, bottom = span["bbox"]
        assert x0 == span["origin"][0]
        assert x1 > x0, span["text"]
        assert top <= bottom, span["text"]
        assert span["ascender"] == 0.800000011920929
        assert span["descender"] == -0.20000000298023224
        assert span["flags"] & 8
    by_text = {}
    for span in mono:
        by_text.setdefault(span["text"], []).append(span["bbox"])
    assert [72.0, 124.75390625, 78.0, 132.0] in by_text["f"]
    assert [102.0, 124.75390625, 108.0, 132.0] in by_text["i"]
    assert [84.0, 132.0, 90.0, 132.0] in by_text[" "]


def test_g_span_boxes_unwired_for_base14_fonts():
    with _page("code-snippet.pdf") as page:
        spans = _spans(legacy.get_raw_lines(textpage=page))
    helv = [s for s in spans if s["font"] == "Helvetica"]
    assert helv, "expected base-14 spans on the page"
    for span in helv:
        x, y = span["origin"]
        assert span["bbox"] == [x, y, x, y]
        assert span["ascender"] == 0.0
        assert span["descender"] == 0.0
