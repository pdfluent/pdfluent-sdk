"""``pdfluent.chunks``, the chunker under both RAG loaders (#630).

The loaders' own suites (integrations/*/tests) check what LangChain and
LlamaIndex receive; this checks the inputs they both pass through.
"""

import os
import pathlib

import pytest

from pdfluent import Document
from pdfluent.chunks import iter_chunks

STRUCTURED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf"
)


def kinds(chunks):
    return [c.metadata["kind"] for c in chunks]


def test_a_path_bytes_and_an_open_document_give_the_same_chunks():
    with open(STRUCTURED_PDF, "rb") as handle:
        data = handle.read()
    from_path = list(iter_chunks(pathlib.Path(STRUCTURED_PDF)))
    from_bytes = list(iter_chunks(data))
    with Document(STRUCTURED_PDF) as doc:
        from_document = list(iter_chunks(doc))

    assert [c.text for c in from_path] == [c.text for c in from_bytes]
    assert [c.text for c in from_bytes] == [c.text for c in from_document]
    assert kinds(from_path) == ["heading", "list_item", "list_item", "list_item", "table"]
    # Only a path names a source.
    assert from_path[0].metadata["source"] == STRUCTURED_PDF
    assert "source" not in from_bytes[0].metadata


def test_a_table_is_one_chunk_with_the_union_of_its_cells_boxes():
    table = list(iter_chunks(STRUCTURED_PDF))[-1]
    with Document(STRUCTURED_PDF) as doc:
        cells = [b for b in doc.extract_blocks() if b.kind == "table_cell"]

    assert table.metadata["bbox"] == [
        min(c.bbox[0] for c in cells),
        min(c.bbox[1] for c in cells),
        max(c.bbox[2] for c in cells),
        max(c.bbox[3] for c in cells),
    ]
    assert len(table.text.splitlines()) == len({c.row for c in cells})


def test_an_unknown_mode_is_refused():
    with pytest.raises(ValueError):
        list(iter_chunks(STRUCTURED_PDF, mode="pages"))
