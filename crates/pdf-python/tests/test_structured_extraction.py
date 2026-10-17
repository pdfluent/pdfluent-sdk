"""The structured-extraction API through the Python binding (#573).

Two methods, one document. ``fixtures/structured.pdf`` is built and pinned by
``crates/pdf-engine/tests/the_api_over_the_block_model.rs``: a heading and a
bulleted list on page 0, a ruled table on page 1. Every binding suite reads
that same file and asserts the same three things, so a fault in one crossing
stands next to five that still work.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/test_structured_extraction.py
"""

import os

import pytest

from pdfluent import Document, PdfluentValidationError


FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")


@pytest.fixture()
def doc():
    with Document(STRUCTURED_PDF) as opened:
        yield opened


def test_extract_markdown_has_a_heading_a_list_item_and_a_table_cell(doc):
    markdown = doc.extract_markdown()

    assert "# Safety rules" in markdown
    assert "- Wear the issued boots on the floor." in markdown
    assert "| Band | Units | Rate |" in markdown


def test_extract_blocks_has_a_heading_a_list_item_and_a_table_cell(doc):
    blocks = doc.extract_blocks()

    headings = [b for b in blocks if b.kind == "heading"]
    items = [b for b in blocks if b.kind == "list_item"]
    cells = [b for b in blocks if b.kind == "table_cell"]

    assert any(b.text == "Safety rules" and b.level == 1 for b in headings)
    assert any(b.text == "Wear the issued boots on the floor." for b in items)
    assert any(
        b.text == "Band" and b.row == 0 and b.column == 0 and b.header for b in cells
    )


def test_a_block_carries_its_page_and_box(doc):
    first = doc.extract_blocks()[0]

    assert first.page == 0
    assert len(first.bbox) == 4
    assert first.bbox[2] > first.bbox[0]
    assert first.index == 0
    assert first.continues_previous is False


def test_the_kind_specific_attributes_are_present_only_on_their_own_kind(doc):
    blocks = doc.extract_blocks()
    item = next(b for b in blocks if b.kind == "list_item")
    cell = next(b for b in blocks if b.kind == "table_cell")

    # A bullet has no number of its own, and a cell is not a list item.
    assert item.ordinal is None
    assert item.row is None
    assert cell.row_span == 1
    assert cell.column_span == 1
    assert cell.level is None

    assert "Block(" in repr(cell)
    assert str(cell) == cell.text


def test_a_window_holds_the_pages_it_asks_for(doc):
    second = doc.extract_blocks(first_page=1, page_count=1)

    assert second, "the second page has blocks"
    assert all(b.page == 1 for b in second)
    assert not any(b.text == "Safety rules" for b in second)
    assert "| Band | Units | Rate |" in doc.extract_markdown(first_page=1, page_count=1)


def test_a_window_past_the_end_is_empty_rather_than_an_error(doc):
    assert doc.extract_blocks(first_page=9, page_count=4) == []
    assert doc.extract_markdown(first_page=9, page_count=4) == ""


def test_a_page_list_holds_the_pages_it_names_in_reading_order(doc):
    # The list selects and the reading order stands: [1, 0] reads as
    # "pages 0 and 1", the same answer a window over them would give.
    blocks = doc.extract_blocks(pages=[1, 0])

    assert blocks, "both pages of the fixture have blocks"
    assert all(b.page in (0, 1) for b in blocks)
    assert [b.page for b in blocks] == sorted(b.page for b in blocks)
    assert any(b.text == "Safety rules" for b in blocks)
    assert "| Band | Units | Rate |" in doc.extract_markdown(pages=[1, 0])


def test_a_page_list_beside_a_window_is_a_validation_error(doc):
    # pages beside first_page/page_count is a mistake in the call: it reads
    # as PdfluentValidationError, not PdfluentIoError (#574).
    with pytest.raises(PdfluentValidationError):
        doc.extract_blocks(first_page=0, page_count=1, pages=[0])


SCANNED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "scanned", "scan_01_sample.pdf"
)


def test_vision_advice_names_the_scanned_page_and_not_the_typeset_ones(doc):
    # One entry per page, and neither page of the typeset fixture asks for a
    # vision model: both have a text layer (#624, #625).
    typeset = doc.vision_advice()
    assert [a.page for a in typeset] == [0, 1]
    assert not any(a.needs_vision for a in typeset)

    # A scan draws an image and yields no text, and says why it needs vision.
    (scan,) = Document(SCANNED_PDF).vision_advice()
    assert scan.needs_vision is True
    assert scan.no_text_layer is True
    # Not measured yet, and never guessed at.
    assert scan.low_confidence_table is False
    assert scan.figure is False
