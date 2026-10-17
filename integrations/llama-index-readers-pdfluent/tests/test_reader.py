"""The LlamaIndex reader over fixtures/structured.pdf (#630).

The fixture every binding suite reads: a heading and a bulleted list of three
items on page 0, a ruled table of four rows on page 1. Built and pinned by
crates/pdf-engine/tests/the_api_over_the_block_model.rs.
"""

import os

import pytest
from llama_index.core import SimpleDirectoryReader
from llama_index.core.schema import Document, MetadataMode

from llama_index.readers.pdfluent import PDFluentReader

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")


def test_blocks_mode_gives_one_document_per_block_and_one_per_table():
    docs = PDFluentReader().load_data(STRUCTURED_PDF)

    assert all(isinstance(d, Document) for d in docs)
    assert [d.metadata["kind"] for d in docs] == [
        "heading",
        "list_item",
        "list_item",
        "list_item",
        "table",
    ]
    assert docs[0].text == "Safety rules"
    assert docs[1].metadata["section"] == "Safety rules"
    assert docs[-1].text.splitlines()[0] == "Band | Units | Rate"
    assert len(docs[-1].text.splitlines()) == 4


def test_every_document_carries_its_source_page_and_box():
    for doc in PDFluentReader().load_data(STRUCTURED_PDF, extra_info={"batch": "a"}):
        meta = doc.metadata
        assert meta["source"] == STRUCTURED_PDF
        assert meta["batch"] == "a"
        assert meta["page"] in (0, 1)
        x0, y0, x1, y1 = meta["bbox"]
        assert x1 > x0 and y1 > y0
        assert "confidence" not in meta or 0.0 <= meta["confidence"] <= 1.0


def test_the_geometry_is_kept_from_the_models_and_the_page_is_not():
    doc = PDFluentReader().load_data(STRUCTURED_PDF)[1]

    for mode in (MetadataMode.EMBED, MetadataMode.LLM):
        seen = doc.get_metadata_str(mode)
        assert "bbox" not in seen
        assert "page" in seen and "section" in seen


def test_markdown_mode_gives_one_document_per_page():
    docs = PDFluentReader(mode="markdown").load_data(STRUCTURED_PDF)

    assert [d.metadata["page"] for d in docs] == [0, 1]
    assert docs[0].text.startswith("# Safety rules")
    assert "| Band | Units | Rate |" in docs[1].text


def test_it_works_as_a_file_extractor_of_simple_directory_reader():
    docs = SimpleDirectoryReader(
        input_files=[STRUCTURED_PDF],
        file_extractor={".pdf": PDFluentReader()},
    ).load_data()

    assert [d.metadata["kind"] for d in docs][-1] == "table"
    assert all(d.metadata["file_name"] == "structured.pdf" for d in docs)


def test_an_unknown_mode_is_refused():
    with pytest.raises(ValueError):
        PDFluentReader(mode="pages")
