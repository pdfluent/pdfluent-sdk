# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`pdfluent_extract` over `fixtures/structured.pdf` and built Office files.

The PDF fixture every binding suite reads: a heading and a bulleted list of
three items on page 0, a ruled table of four rows on page 1. Built and pinned
by `crates/pdf-engine/tests/the_api_over_the_block_model.rs`. The Office
fixture is a one-paragraph Word file built below, in the same style as
`crates/pdf-python/tests/office_fixtures.py`.
"""

import io
import json
import os
import zipfile

import pytest

pdfluent = pytest.importorskip("pdfluent")

from pdfluent_extract import Chunk, to_chunks, to_json, to_markdown  # noqa: E402

STRUCTURED_PDF = os.path.join(
    os.path.dirname(__file__), "..", "..", "..", "fixtures", "structured.pdf"
)

DOCX_SENTENCE = "The quarterly figures are final."


def docx_bytes() -> bytes:
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">\n'
        "<w:body>"
        "<w:p><w:r><w:t>" + DOCX_SENTENCE + "</w:t></w:r></w:p>"
        "</w:body></w:document>"
    )
    types = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument'
        '.wordprocessingml.document.main+xml"/>\n'
        "</Types>"
    )
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("[Content_Types].xml", types)
        zf.writestr("word/document.xml", document)
    return buf.getvalue()


def test_to_markdown_carries_the_fixture_sentence():
    markdown = to_markdown(STRUCTURED_PDF)
    assert "# Safety rules" in markdown
    assert "- Wear the issued boots on the floor." in markdown
    assert "| Band | Units | Rate |" in markdown


def test_to_markdown_reads_bytes_and_file_like_objects():
    with open(STRUCTURED_PDF, "rb") as fh:
        data = fh.read()
    assert to_markdown(data) == to_markdown(STRUCTURED_PDF)
    assert to_markdown(io.BytesIO(data)) == to_markdown(STRUCTURED_PDF)


def test_to_markdown_selects_pages():
    first = to_markdown(STRUCTURED_PDF, pages=[0])
    assert "# Safety rules" in first
    assert "| Band | Units | Rate |" not in first
    second = to_markdown(STRUCTURED_PDF, pages=[1])
    assert "| Band | Units | Rate |" in second
    assert "# Safety rules" not in second


def test_to_json_is_a_docling_document_named_for_the_file():
    doc = to_json(STRUCTURED_PDF)
    assert doc["name"] == "structured"
    texts = " ".join(t.get("text", "") for t in doc["texts"])
    assert "Safety rules" in texts


def test_to_json_names_bytes_sources_document():
    with open(STRUCTURED_PDF, "rb") as fh:
        assert to_json(fh.read())["name"] == "document"


def test_to_chunks_covers_the_fixture_with_provenance():
    chunks = to_chunks(STRUCTURED_PDF)
    assert all(isinstance(c, Chunk) for c in chunks)
    assert [c.metadata["kind"] for c in chunks] == [
        "heading",
        "list_item",
        "list_item",
        "list_item",
        "table",
    ]
    heading, first_item, *_, table = chunks
    assert heading.text == "Safety rules"
    assert first_item.metadata["section_path"] == ["Safety rules"]
    assert first_item.metadata["page"] == 0
    assert table.metadata["page"] == 1
    x0, y0, x1, y1 = table.metadata["bbox"]
    assert x1 > x0 and y1 > y0
    assert heading.metadata["source"] == STRUCTURED_PDF


def test_to_chunks_selects_pages_in_listed_order():
    chunks = to_chunks(STRUCTURED_PDF, pages=[1, 0])
    assert [c.metadata["page"] for c in chunks] == [1, 0, 0, 0, 0]


def test_to_chunks_budgets_by_tokens():
    roomy = to_chunks(STRUCTURED_PDF, max_tokens=512)
    tight = to_chunks(STRUCTURED_PDF, max_tokens=8)
    # The table (four rows) no longer fits one 32-char piece: it splits by
    # rows with the header repeated, so there are more chunks, not fewer.
    assert len(tight) > len(roomy)
    assert any("table_part" in c.metadata for c in tight)
    with pytest.raises(ValueError, match="max_tokens"):
        to_chunks(STRUCTURED_PDF, max_tokens=0)


def test_quality_other_than_fast_is_refused_by_name():
    for fn in (to_markdown, to_json, to_chunks):
        with pytest.raises(ValueError, match="fast"):
            fn(STRUCTURED_PDF, quality="accurate")


def test_office_routes_by_magic_bytes_not_the_name(tmp_path):
    # A Word file with a PDF's name still extracts as Word.
    misnamed = tmp_path / "report.pdf"
    misnamed.write_bytes(docx_bytes())
    assert DOCX_SENTENCE in to_markdown(str(misnamed))
    assert DOCX_SENTENCE in to_markdown(misnamed.read_bytes())
    doc = to_json(misnamed.read_bytes())
    assert doc["name"] == "document"
    assert DOCX_SENTENCE in json.dumps(doc)


def test_office_chunks_are_not_in_v1():
    with pytest.raises(NotImplementedError, match="v1"):
        to_chunks(docx_bytes())


def test_office_pages_are_refused_explicitly():
    with pytest.raises(ValueError, match="pages"):
        to_markdown(docx_bytes(), pages=[0])


def test_office_without_sub_task_a_is_a_clear_not_implemented_error(monkeypatch):
    # Before sub-task A landed, `pdfluent` had no `extract_office`: Office
    # raised instead of misrouting. The dependency pins pdfluent>=1.1.0, so
    # this only fires on a hand-built older pdfluent — simulated here.
    monkeypatch.delattr(pdfluent, "extract_office", raising=False)
    with pytest.raises(NotImplementedError, match="pdfluent>=1.1.0"):
        to_markdown(docx_bytes())


def test_garbage_is_not_a_pdf_or_office_file():
    with pytest.raises(pdfluent.PdfluentParseError, match="not a PDF or Office"):
        to_markdown(b"not a document at all")


def test_a_bad_source_type_is_a_type_error():
    with pytest.raises(TypeError, match="path, bytes or a file-like"):
        to_markdown(42)
