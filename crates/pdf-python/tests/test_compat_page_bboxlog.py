# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Page.get_bboxlog through the public path (#646).

One (kind, rect) entry per painted item in content-stream order, with an
optional layer element. Boxes are line-metric, which the documented
difference calls out against the recording's glyph-ink boxes.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_page_bboxlog.py -v
"""

from __future__ import annotations

import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

HERE = os.path.dirname(__file__)
SAMPLE = os.path.normpath(
    os.path.join(HERE, "..", "..", "..", "fixtures", "sample.pdf")
)


def test_bboxlog_lists_text_spans() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        entries = doc[0].get_bboxlog()
    finally:
        doc.close()
    assert len(entries) == 1
    kind, rect = entries[0]
    assert kind == "fill-text"
    assert rect[2] > rect[0] and rect[3] > rect[1]


def test_bboxlog_empty_layers_match_default() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        assert doc[0].get_bboxlog([]) == doc[0].get_bboxlog()
    finally:
        doc.close()


def test_bboxlog_true_layers_add_the_layer_element() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        entries = doc[0].get_bboxlog(True)
    finally:
        doc.close()
    assert len(entries) == 1
    kind, rect, layer = entries[0]
    assert kind == "fill-text"
    assert layer == ""


def test_bboxlog_two_args_is_type_error() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        with pytest.raises(TypeError):
            doc[0].get_bboxlog(1, 2)
    finally:
        doc.close()
