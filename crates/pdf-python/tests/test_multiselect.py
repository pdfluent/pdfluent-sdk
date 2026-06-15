"""Multi-select list box binding test (AcroForm closure).

Self-contained: the fixture is embedded as base64 so the test needs no
external file and runs in CI. Skips when the native module is not built.
"""
import base64

import pytest

pdfluent_native = pytest.importorskip("pdfluent._native")
Document = pdfluent_native.Document

# Minimal AcroForm with one multi-select list box "languages",
# /Opt = ["EN","NL","DE","FR"] (from pdfluent-forms gen_acroform_corpus).
MULTISELECT_PDF_B64 = "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvQ2gvVChsYW5ndWFnZXMpL0ZmIDIwOTcxNTIvUmVjdFsxMDAgNDAwIDMyMCA1MjBdL09wdFsoRU4pKE5MKShERSkoRlIpXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAFVAAABAAABigAAAQAAAcYAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQ1NAolJUVPRg=="


def _fixture(tmp_path):
    p = tmp_path / "multiselect.pdf"
    p.write_bytes(base64.b64decode(MULTISELECT_PDF_B64))
    return str(p)


def test_set_multi_select_round_trips(tmp_path):
    doc = Document(_fixture(tmp_path))
    assert doc.set_multi_select("languages", ["FR", "EN"]) is True
    out = tmp_path / "filled.pdf"
    doc.save(str(out))

    reloaded = Document(str(out))
    values = {f.name: f.value for f in reloaded.get_form_fields()}
    assert "languages" in values
    # /V is an array → the flat read joins it; both options present.
    assert "FR" in values["languages"]
    assert "EN" in values["languages"]


def test_set_multi_select_rejects_unknown_option(tmp_path):
    doc = Document(_fixture(tmp_path))
    with pytest.raises(Exception):
        doc.set_multi_select("languages", ["KL"])


def test_set_multi_select_empty_clears(tmp_path):
    doc = Document(_fixture(tmp_path))
    assert doc.set_multi_select("languages", []) is True


def test_set_multi_select_unknown_field_returns_false(tmp_path):
    doc = Document(_fixture(tmp_path))
    assert doc.set_multi_select("does_not_exist", ["EN"]) is False


def test_deprecated_alias_set_form_field_multi_still_works(tmp_path):
    """The pre-beta.9 name forwards to set_multi_select (removed in 1.0.0)."""
    doc = Document(_fixture(tmp_path))
    assert hasattr(doc, "set_form_field_multi")
    assert doc.set_form_field_multi("languages", ["FR", "EN"]) is True
    out = tmp_path / "filled_alias.pdf"
    doc.save(str(out))
    reloaded = Document(str(out))
    values = {f.name: f.value for f in reloaded.get_form_fields()}
    assert "FR" in values["languages"]
    assert "EN" in values["languages"]
