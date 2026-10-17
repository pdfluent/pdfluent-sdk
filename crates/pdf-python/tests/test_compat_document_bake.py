# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Document.bake through the public path (#648).

Mirrors the l4-document-bake case through lib.open: annotations and widgets
flatten into content, both flags off changes nothing, and a non-boolean
flag raises ValueError.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_document_bake.py -v
"""

from __future__ import annotations

import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

HERE = os.path.dirname(__file__)
ANNOTATIONS = os.path.normpath(
    os.path.join(HERE, "..", "..", "visual-regression", "fixtures", "annotations.pdf")
)
ACROFORM = os.path.normpath(
    os.path.join(HERE, "..", "..", "..", "fixtures", "acroform.pdf")
)


def _counts(doc) -> tuple[int, int]:
    page = doc[0]
    return len(list(page.annots())), len(list(page.widgets()))


def test_bake_flattens_annots() -> None:
    doc = pdfluent.open(ANNOTATIONS)
    try:
        assert _counts(doc) == (3, 0)
        assert doc.bake() is None
        assert _counts(doc) == (0, 0)
    finally:
        doc.close()


def test_bake_flattens_widgets() -> None:
    doc = pdfluent.open(ACROFORM)
    try:
        assert _counts(doc) == (0, 2)
        doc.bake()
        assert _counts(doc) == (0, 0)
    finally:
        doc.close()


def test_bake_with_both_flags_off_changes_nothing() -> None:
    doc = pdfluent.open(ANNOTATIONS)
    try:
        assert doc.bake(annots=False, widgets=False) is None
        assert _counts(doc) == (3, 0)
    finally:
        doc.close()
    doc = pdfluent.open(ACROFORM)
    try:
        doc.bake(annots=False, widgets=False)
        assert _counts(doc) == (0, 2)
    finally:
        doc.close()


def test_bake_bad_flag_raises_value_error() -> None:
    doc = pdfluent.open(ANNOTATIONS)
    try:
        with pytest.raises(ValueError):
            doc.bake(annots="x")
    finally:
        doc.close()
