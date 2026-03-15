"""
Smoke tests for the pdf-python binding — 12 core scenarios.

Run:
    cd crates/pdf-python
    maturin develop
    pytest tests/
"""

import os
import pytest

# Path to fixtures (relative to project root)
FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
SAMPLE_PDF = os.path.join(FIXTURES, "sample.pdf")
ACROFORM_PDF = os.path.join(FIXTURES, "acroform.pdf")
SIGNED_PDF = os.path.join(FIXTURES, "signed.pdf")
MULTI_PDF = os.path.join(FIXTURES, "multi-page.pdf")

# Import the native module — skip all if not built
pdfengine = pytest.importorskip("pdfengine._native")
Document = pdfengine.Document
open_pdf = pdfengine.open_pdf
merge_pdfs = pdfengine.merge_pdfs
validate_pdfa = pdfengine.validate_pdfa


# ---------- Scenario 1: Open PDF, count pages ----------

def test_open_and_page_count():
    doc = Document(SAMPLE_PDF)
    assert doc.page_count >= 1


def test_multi_page():
    doc = Document(MULTI_PDF)
    assert doc.page_count > 1


# ---------- Scenario 2: Render page 1 ----------

def test_render_page():
    doc = Document(SAMPLE_PDF)
    page = doc[0]
    img = page.render(dpi=72.0)
    assert img.width > 0
    assert img.height > 0
    assert len(img.pixels) == img.width * img.height * 4


# ---------- Scenario 3: Extract text ----------

def test_extract_text():
    doc = Document(SAMPLE_PDF)
    page = doc[0]
    text = page.extract_text()
    # simple.pdf may have minimal text; verify API works
    assert isinstance(text, str)


# ---------- Scenario 4: Read metadata ----------

def test_metadata():
    doc = Document(SAMPLE_PDF)
    meta = doc.metadata
    # All standard keys should be accessible (may be None)
    assert hasattr(meta, "title")
    assert hasattr(meta, "author")
    assert hasattr(meta, "subject")
    assert hasattr(meta, "creator")
    assert hasattr(meta, "producer")


# ---------- Scenario 5: Read AcroForm fields ----------

@pytest.mark.skip(reason="TODO: Document.form_fields() not yet exposed in Python binding")
def test_form_fields_read():
    doc = Document(ACROFORM_PDF)
    # TODO: fields = doc.form_fields()
    # assert len(fields) > 0


# ---------- Scenario 6: Fill text field, save ----------

@pytest.mark.skip(reason="TODO: Form write API not yet exposed in Python binding")
def test_form_field_write():
    pass


# ---------- Scenario 7: Read annotations ----------

@pytest.mark.skip(reason="TODO: Annotation reading not yet exposed in Python binding")
def test_annotations_read():
    pass


# ---------- Scenario 8: Add highlight, save ----------

@pytest.mark.skip(reason="TODO: Annotation creation not yet exposed in Python binding")
def test_annotation_highlight():
    pass


# ---------- Scenario 9: Validate PDF/A ----------

def test_pdfa_validation():
    report = validate_pdfa(SAMPLE_PDF)
    assert hasattr(report, "is_compliant")
    assert hasattr(report, "error_count")
    assert hasattr(report, "warning_count")
    assert hasattr(report, "issues")
    assert isinstance(report.issues, list)
    assert isinstance(report.is_compliant, bool)


# ---------- Scenario 10: Merge 2 PDFs ----------

def test_merge_pdfs(tmp_path):
    output = str(tmp_path / "merged.pdf")
    merge_pdfs([SAMPLE_PDF, MULTI_PDF], output)
    merged = Document(output)
    doc_a = Document(SAMPLE_PDF)
    doc_b = Document(MULTI_PDF)
    assert merged.page_count == doc_a.page_count + doc_b.page_count


# ---------- Scenario 11: Verify signature ----------

@pytest.mark.skip(reason="TODO: Signature verification not yet exposed in Python binding")
def test_verify_signature():
    pass


# ---------- Scenario 12: Extract images ----------

@pytest.mark.skip(reason="TODO: Image extraction not yet exposed in Python binding")
def test_extract_images():
    pass


# ---------- Extra: open_pdf convenience function ----------

def test_open_pdf_function():
    doc = open_pdf(SAMPLE_PDF)
    assert doc.page_count >= 1


# ---------- Extra: document.extract_text(page_num) ----------

def test_document_extract_text():
    doc = Document(SAMPLE_PDF)
    text = doc.extract_text(0)
    assert isinstance(text, str)


# ---------- Extra: document.save ----------

def test_document_save(tmp_path):
    doc = Document(SAMPLE_PDF)
    out = str(tmp_path / "copy.pdf")
    doc.save(out)
    copy = Document(out)
    assert copy.page_count == doc.page_count


# ---------- Extra: context manager ----------

def test_context_manager():
    with Document(SAMPLE_PDF) as doc:
        assert doc.page_count >= 1


# ---------- Extra: page geometry ----------

def test_page_geometry():
    doc = Document(SAMPLE_PDF)
    page = doc[0]
    geo = page.geometry
    assert geo.width > 0
    assert geo.height > 0


# ---------- Extra: thumbnail ----------

def test_thumbnail():
    doc = Document(SAMPLE_PDF)
    page = doc[0]
    thumb = page.thumbnail(max_dimension=200)
    assert thumb.width > 0
    assert thumb.height > 0
    assert max(thumb.width, thumb.height) <= 200


# ---------- Extra: iteration ----------

def test_iteration():
    doc = Document(MULTI_PDF)
    pages = list(doc)
    assert len(pages) == doc.page_count


# ---------- Extra: error handling ----------

def test_invalid_pdf():
    with pytest.raises(Exception):
        Document(b"not a pdf")


def test_file_not_found():
    with pytest.raises(Exception):
        Document("/nonexistent.pdf")
