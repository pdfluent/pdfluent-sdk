"""Words through the Python binding.

``fixtures/structured.pdf`` is the document every binding suite reads: a
heading and a bulleted list on page 0, a ruled table on page 1. The engine
reads 41 words out of it
(``crates/pdf-engine/tests/words_come_from_the_blocks.rs`` pins the boxing);
this suite pins the crossing, so a fault here stands next to a Node suite
that still works.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/test_extract_words.py
"""

import os

import pytest

from pdfluent import Document

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")

#: What the engine reads out of the fixture: 41 words in 16 blocks.
ENGINE_WORD_COUNT = 41


@pytest.fixture()
def doc():
    with Document(STRUCTURED_PDF) as opened:
        yield opened


def test_extract_words_matches_the_engine_count(doc):
    words = doc.extract_words()

    assert len(words) == ENGINE_WORD_COUNT
    assert [w.text for w in words[:2]] == ["Safety", "rules"]
    assert words[-1].text == "11.07"


def test_a_word_carries_its_place_and_box(doc):
    words = doc.extract_words()
    blocks = doc.extract_blocks()

    first = words[0]
    assert first.page == 0
    assert first.block == 0
    assert first.line == 0
    assert len(first.bbox) == 4
    assert first.bbox[2] > first.bbox[0]
    assert first.bbox[3] > first.bbox[1]
    assert first.text in blocks[first.block].text
    assert first.font_size > 0
    assert first.bold is True
    assert first.italic is False


def test_a_page_list_returns_only_those_pages_words(doc):
    words = doc.extract_words(pages=[1])

    assert len(words) > 0
    assert all(w.page == 1 for w in words)
    assert "Safety" not in [w.text for w in words]
