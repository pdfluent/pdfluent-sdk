"""Runs the plugin through MarkItDown's own plugin loading, on a real PDF."""

import io
from pathlib import Path

import pytest
from markitdown import MarkItDown, StreamInfo

import markitdown_pdfluent
from markitdown_pdfluent import PDFluentPdfConverter

PDF = Path(__file__).parent / "fixtures" / "sample.pdf"


def test_entry_point_is_discovered():
    from importlib.metadata import entry_points

    names = {e.name for e in entry_points(group="markitdown.plugin")}
    assert "pdfluent" in names
    assert markitdown_pdfluent.__plugin_interface_version__ == 1


def test_plugin_converts_a_real_pdf():
    result = MarkItDown(enable_plugins=True).convert(str(PDF))
    assert "Individual Taxpayer Identification Number" in result.markdown
    assert "W-7" in result.markdown


def test_plugin_output_is_the_pdfluent_engine_output():
    from pdfluent import Document

    with Document(PDF.read_bytes()) as doc:
        expected = doc.extract_markdown().strip() + "\n"
    assert MarkItDown(enable_plugins=True).convert(str(PDF)).markdown == expected
    assert PDFluentPdfConverter().convert(io.BytesIO(PDF.read_bytes()), StreamInfo(extension=".pdf")).markdown == expected


def test_stream_without_hints_is_detected_by_magic():
    c = PDFluentPdfConverter()
    stream = io.BytesIO(PDF.read_bytes())
    assert c.accepts(stream, StreamInfo())
    assert stream.tell() == 0
    assert not c.accepts(io.BytesIO(b"hello"), StreamInfo())


def test_other_formats_are_not_accepted():
    c = PDFluentPdfConverter()
    assert not c.accepts(io.BytesIO(b"x"), StreamInfo(extension=".docx"))
    assert c.accepts(io.BytesIO(b"x"), StreamInfo(mimetype="application/pdf"))


def test_corrupt_pdf_raises():
    with pytest.raises(Exception):
        PDFluentPdfConverter().convert(io.BytesIO(b"%PDF-1.4 not really"), StreamInfo(extension=".pdf"))
