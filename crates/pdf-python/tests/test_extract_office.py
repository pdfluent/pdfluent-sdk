# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`pdfluent.extract_office`: Office files to Markdown or Docling JSON.

Sub-task A of the every-platform work: the binding exposes the extract crates
(`docx-extract`, `pptx-extract`, `xlsx-extract`), which only the reverse
direction (`to_docx`/`to_xlsx` export) reached before. One fixture per format,
built in `office_fixtures.py`; routing by content type, not extension; and the
refusals (a PDF, garbage, a bad format name) each fail their own way.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/test_extract_office.py
"""

import json
import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

from office_fixtures import (  # noqa: E402
    DOCX_SENTENCE,
    PPTX_SENTENCE,
    XLSX_SENTENCE,
    docx_bytes,
    docx_with_chart_bytes,
    pptx_bytes,
    xlsx_bytes,
)

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
STRUCTURED_PDF = os.path.join(FIXTURES, "structured.pdf")


def test_docx_extracts_to_markdown_with_its_sentence():
    markdown = pdfluent.extract_office(docx_bytes())
    assert "# Quarterly report" in markdown
    assert DOCX_SENTENCE in markdown


def test_pptx_extracts_to_markdown_with_its_sentence():
    markdown = pdfluent.extract_office(pptx_bytes())
    assert "# Quarterly review." in markdown
    assert "Revenue is up year over year." in markdown


def test_xlsx_extracts_to_markdown_with_its_sentence():
    markdown = pdfluent.extract_office(xlsx_bytes())
    assert "# Q1" in markdown
    assert XLSX_SENTENCE in markdown
    assert "North | 42" in markdown


def test_docx_with_a_chart_routes_by_its_main_part():
    # The container names spreadsheetml (embedded worksheet) and drawingml
    # (chart): routing must still follow /word/document.xml, not a substring.
    markdown = pdfluent.extract_office(docx_with_chart_bytes())
    assert "# Quarterly report" in markdown
    assert DOCX_SENTENCE in markdown


def test_routing_ignores_the_extension(tmp_path):
    # A workbook saved as .docx extracts as a workbook: content types decide.
    misnamed = tmp_path / "book.docx"
    misnamed.write_bytes(xlsx_bytes())
    markdown = pdfluent.extract_office(str(misnamed))
    assert XLSX_SENTENCE in markdown


def test_bytes_and_paths_agree(tmp_path):
    office = tmp_path / "report.docx"
    office.write_bytes(docx_bytes())
    assert pdfluent.extract_office(docx_bytes()) == pdfluent.extract_office(
        str(office)
    )


def test_docling_format_is_a_docling_document(tmp_path):
    office = tmp_path / "report.docx"
    office.write_bytes(docx_bytes())
    doc = json.loads(
        pdfluent.extract_office(str(office), format="docling", name="report")
    )
    assert doc["name"] == "report"
    texts = " ".join(t.get("text", "") for t in doc["texts"])
    assert DOCX_SENTENCE in texts


def test_unknown_format_is_refused_by_name():
    with pytest.raises(pdfluent.PdfluentValidationError, match="markdown.*docling"):
        pdfluent.extract_office(docx_bytes(), format="html")


def test_a_pdf_is_not_an_office_file():
    with open(STRUCTURED_PDF, "rb") as fh:
        pdf = fh.read()
    with pytest.raises(pdfluent.PdfluentParseError, match="not an Office file"):
        pdfluent.extract_office(pdf)


def test_garbage_is_not_an_office_file():
    with pytest.raises(pdfluent.PdfluentParseError, match="not an Office file"):
        pdfluent.extract_office(b"not a zip at all")


def test_missing_file_is_an_io_error():
    with pytest.raises(pdfluent.PdfluentIoError):
        pdfluent.extract_office("/nonexistent/report.docx")
