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
pdfluent_native = pytest.importorskip("pdfluent._native")
Document = pdfluent_native.Document
open_pdf = pdfluent_native.open_pdf
merge_pdfs = pdfluent_native.merge_pdfs
validate_pdfa = pdfluent_native.validate_pdfa
decrypt_pdf = pdfluent_native.decrypt_pdf
FormField = pdfluent_native.FormField
Annotation = pdfluent_native.Annotation
RedactReport = pdfluent_native.RedactReport


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

def test_form_fields_read():
    doc = Document(ACROFORM_PDF)
    fields = doc.get_form_fields()
    assert isinstance(fields, list)
    assert len(fields) > 0
    for f in fields:
        assert isinstance(f.name, str)
        assert isinstance(f.field_type, str)
        assert f.field_type in ("text", "button", "choice", "signature", "unknown")
        # value may be None for empty fields
        assert f.value is None or isinstance(f.value, str)
        # page may be None if widget has no page association
        assert f.page is None or isinstance(f.page, int)


# ---------- Scenario 6: Fill text field, save ----------
#
# set_form_field routes through the single SDK writeback chain
# (pdf_forms::apply_field_value): correct /V encoding (ASCII literal else
# UTF-16BE+BOM), per-widget /AS sync, and /AP regeneration — replacing the
# old raw-bytes /V write (mojibake on non-ASCII) + bogus "NeedsAppearances"
# key (extra `s`).

def test_form_field_write(tmp_path):
    doc = Document(ACROFORM_PDF)
    fields = doc.get_form_fields()
    # Find the first text field
    text_fields = [f for f in fields if f.field_type == "text"]
    if not text_fields:
        pytest.skip("no text fields in acroform.pdf")
    field_name = text_fields[0].name
    # Non-ASCII value: the writeback chain must encode /V as UTF-16BE+BOM.
    assert doc.set_form_field(field_name, "Café Test") is True
    # Save and reload: the value must round-trip without mojibake (the old
    # raw-bytes write produced UTF-8 bytes misread as PDFDocEncoding).
    out = str(tmp_path / "filled.pdf")
    doc.save(out)
    reloaded = Document(out)
    values = {f.name: f.value for f in reloaded.get_form_fields()}
    assert values[field_name] == "Café Test"
    assert reloaded.page_count == doc.page_count


def test_form_field_write_unknown_field_returns_false():
    doc = Document(SAMPLE_PDF)
    # sample.pdf has no form: contract is False (not found), not an error.
    assert doc.set_form_field("does.not.exist", "x") is False


# ---------- Scenario 7: Read annotations ----------

def test_annotations_read():
    doc = Document(SAMPLE_PDF)
    annots = doc.get_annotations(0)
    assert isinstance(annots, list)
    for a in annots:
        assert isinstance(a.annot_type, str)
        assert isinstance(a.rect, tuple)
        assert len(a.rect) == 4


# ---------- Scenario 8: Add highlight, save ----------

def test_annotation_highlight(tmp_path):
    doc = Document(SAMPLE_PDF)
    doc.add_annotation(0, "highlight", (72.0, 700.0, 300.0, 720.0), "highlighted text")
    out = str(tmp_path / "annotated.pdf")
    doc.save(out)
    reloaded = Document(out)
    assert reloaded.page_count == doc.page_count
    annots = reloaded.get_annotations(0)
    assert len(annots) >= 1


def test_annotation_freetext(tmp_path):
    doc = Document(SAMPLE_PDF)
    doc.add_annotation(0, "freetext", (72.0, 600.0, 300.0, 650.0), "Free text note")
    out = str(tmp_path / "freetext.pdf")
    doc.save(out)
    reloaded = Document(out)
    assert reloaded.page_count == doc.page_count


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


# ---------- Scenario 9b: Redact text ----------

def test_redact_text(tmp_path):
    doc = Document(SAMPLE_PDF)
    report = doc.redact_text("the")
    assert isinstance(report, RedactReport)
    assert isinstance(report.matches_found, int)
    assert isinstance(report.areas_redacted, int)
    assert isinstance(report.pages_affected, int)
    out = str(tmp_path / "redacted.pdf")
    doc.save(out)
    assert os.path.exists(out)


def test_redact_text_no_match():
    doc = Document(SAMPLE_PDF)
    report = doc.redact_text("ZZZZZZNOTFOUNDZZZZ")
    assert report.matches_found == 0
    assert report.areas_redacted == 0


def test_redact_text_specific_page():
    doc = Document(MULTI_PDF)
    report = doc.redact_text("the", page=0)
    assert isinstance(report.matches_found, int)


# ---------- Scenario 10b: Encrypt / decrypt ----------

def test_encrypt_decrypt(tmp_path):
    doc = Document(SAMPLE_PDF)
    enc_path = str(tmp_path / "encrypted.pdf")
    doc.encrypt(enc_path, "secret123")
    assert os.path.exists(enc_path)
    # Decrypt using standalone decrypt_pdf (pdf-syntax doesn't support AES-256 reading)
    dec_path = str(tmp_path / "decrypted.pdf")
    decrypt_pdf(enc_path, dec_path, "secret123")
    dec_doc = Document(dec_path)
    assert dec_doc.page_count == doc.page_count


def test_encrypt_wrong_password(tmp_path):
    doc = Document(SAMPLE_PDF)
    enc_path = str(tmp_path / "encrypted.pdf")
    doc.encrypt(enc_path, "correct")
    with pytest.raises(Exception):
        decrypt_pdf(enc_path, str(tmp_path / "out.pdf"), "wrong")


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


# ---------- Capability-matrix parity smoke (GAP-003) ----------


def test_extract_text_blocks_runtime():
    """Runtime smoke for ``Page.extract_text_blocks()``.

    The typing test in ``test_pdfluent_typing.py`` only asserts the
    return type. This test pins the documented public shape at runtime so
    any refactor that drops a field on ``TextBlock`` or ``TextSpan`` fails
    a binding-level CI gate, not only the typing one.
    """
    doc = Document(SAMPLE_PDF)
    blocks = doc[0].extract_text_blocks()
    assert isinstance(blocks, list)
    for block in blocks:
        # TextBlock public surface
        assert isinstance(block.text, str)
        assert isinstance(block.spans, list)
        for span in block.spans:
            # TextSpan public surface (font_size present; G1 metadata
            # fields may be None until G1 lands — they are optional by
            # design and that is documented on PyTextSpan).
            assert isinstance(span.text, str)
            assert isinstance(span.x, float)
            assert isinstance(span.y, float)
            assert isinstance(span.font_size, float)
