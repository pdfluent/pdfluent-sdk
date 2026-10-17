"""The LangChain loader over fixtures/structured.pdf (#630).

The fixture every binding suite reads: a heading and a bulleted list of three
items on page 0, a ruled table of four rows on page 1. Built and pinned by
crates/pdf-engine/tests/the_api_over_the_block_model.rs.
"""

import os

import pytest
from langchain_core.documents import Document

from langchain_pdfluent import PDFluentLoader

STRUCTURED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf"
)


def test_blocks_mode_gives_one_document_per_block_and_one_per_table():
    docs = PDFluentLoader(STRUCTURED_PDF).load()

    assert all(isinstance(d, Document) for d in docs)
    assert [d.metadata["kind"] for d in docs] == [
        "heading",
        "list_item",
        "list_item",
        "list_item",
        "table",
    ]
    heading, first_item, *_, table = docs
    assert heading.page_content == "Safety rules"
    assert heading.metadata["level"] == 1
    assert first_item.page_content == "Wear the issued boots on the floor."
    assert first_item.metadata["section"] == "Safety rules"
    assert table.page_content.splitlines()[0] == "Band | Units | Rate"
    assert len(table.page_content.splitlines()) == 4


def test_every_chunk_carries_its_source_page_and_box():
    for doc in PDFluentLoader(STRUCTURED_PDF).lazy_load():
        meta = doc.metadata
        assert meta["source"] == STRUCTURED_PDF
        assert meta["page"] in (0, 1)
        x0, y0, x1, y1 = meta["bbox"]
        assert x1 > x0 and y1 > y0
        # Present only where a learned model decided the kind, and then a
        # probability.
        assert "confidence" not in meta or 0.0 <= meta["confidence"] <= 1.0
    table = PDFluentLoader(STRUCTURED_PDF).load()[-1]
    assert table.metadata["page"] == 1


def test_markdown_mode_gives_one_document_per_page():
    docs = PDFluentLoader(STRUCTURED_PDF, mode="markdown").load()

    assert [d.metadata["page"] for d in docs] == [0, 1]
    assert docs[0].page_content.startswith("# Safety rules")
    assert "| Band | Units | Rate |" in docs[1].page_content


def test_a_window_loads_only_its_pages():
    docs = PDFluentLoader(STRUCTURED_PDF, first_page=1, page_count=1).load()

    assert [d.metadata["kind"] for d in docs] == ["table"]


def test_an_unknown_mode_is_refused_before_anything_is_read():
    with pytest.raises(ValueError):
        PDFluentLoader(STRUCTURED_PDF, mode="pages")
