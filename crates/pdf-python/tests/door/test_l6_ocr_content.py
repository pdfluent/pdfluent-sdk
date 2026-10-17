# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L6: door OCR content recall on the scanned fixture (owner D8).

Run: cd crates/pdf-python; maturin develop; pytest tests/door/test_l6_ocr_content.py

The recorded-parity tests (g133/g183) pin the recogniser's exact bytes, so
they move with every Tesseract version. This test pins the content instead:
on the scanned fixture the default conversion must produce text whose words
cover the reference at the calibrated OCR recall from tolerance.toml [ocr].
"""

import os
import re

import pytest
from conftest import FIX, SKIP_REASON, SPEC_MISSING
from conftest import read_text as _read_text

import pdfluent_extract as pe
from pdfluent_extract._chunks import _tesseract_version

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]

#: Reason shared with the conftest live-OCR skip: without a recogniser
#: binary the test cannot run, so it skips (never passes).
_NO_BINARY_REASON = "SKIPPED (not a pass): no tesseract binary installed"


def _ocr_recall_floor():
    """The calibrated OCR recall from tolerance.toml [ocr]."""
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.dirname(
        os.path.dirname(os.path.dirname(os.path.dirname(here)))
    )
    path = os.path.join(root, "tests", "compat", "tolerance.toml")
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    try:
        import tomllib

        with open(path, "rb") as handle:
            return float(tomllib.load(handle)["ocr"]["recall"])
    except ImportError:
        match = re.search(
            r"(?ms)^\[ocr\].*?^recall\s*=\s*([0-9.]+)", text
        )
        return float(match.group(1))


def _reference_words():
    """Unique lowercase words of the recorded OCR reference text."""
    text = _read_text("ocrwords/words-default-md.md")
    return sorted(set(re.findall(r"[a-z0-9]+", text.lower())))


def test_door_ocr_content_recall():
    version = _tesseract_version()
    if version is None:
        pytest.skip(_NO_BINARY_REASON)
    print("tesseract-version: %s" % version)
    out = pe.to_markdown(str(FIX("scanned-words.pdf")))
    assert out.strip() != "", "scanned pages must produce text, got %r" % out
    words = _reference_words()
    assert words, "the reference carries no words to cover"
    lowered = out.lower()
    hits = sum(1 for word in words if word in lowered)
    recall = hits / len(words)
    floor = _ocr_recall_floor()
    assert recall >= floor, (
        "OCR recall %.3f below the calibrated floor %.2f "
        "(%d of %d reference words, tesseract %s)"
        % (recall, floor, hits, len(words), version)
    )
