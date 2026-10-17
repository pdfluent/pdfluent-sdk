# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""SPEC section 20 (bad-input errors); L0 owns it, so nothing here xfails."""

import pytest
from conftest import FIX, SKIP_REASON, SPEC_MISSING, outcome, read_bytes

import pdfluent_extract as pe

pytestmark = pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON)

ENTRIES = ("to_markdown", "to_text", "to_json")


def _run(entry, *args, **kwargs):
    return getattr(pe, entry)(*args, **kwargs)


def test_g172_missing_path():
    for entry in ENTRIES:
        with pytest.raises(FileNotFoundError):
            _run(entry, FIX("does-not-exist.pdf"))
        pe.use_layout(False)
        try:
            with pytest.raises(FileNotFoundError):
                _run(entry, FIX("does-not-exist.pdf"))
        finally:
            pe.use_layout(True)


def test_g173_garbage_and_empty():
    for entry in ENTRIES:
        with pytest.raises(pe.FileDataError):
            _run(entry, FIX("not-a-pdf.bin"))
        with pytest.raises(pe.EmptyFileError):
            _run(entry, FIX("empty.bin"))
        pe.use_layout(False)
        try:
            with pytest.raises(pe.FileDataError):
                _run(entry, FIX("not-a-pdf.bin"))
            with pytest.raises(pe.EmptyFileError):
                _run(entry, FIX("empty.bin"))
        finally:
            pe.use_layout(True)


def test_g174_locked_needs_unlock():
    with pytest.raises(TypeError):
        pe.to_markdown(FIX("encrypted.pdf"))
    pe.use_layout(False)
    try:
        with pytest.raises(ValueError):
            pe.to_markdown(FIX("encrypted.pdf"))
    finally:
        pe.use_layout(True)


def test_g175_unlocked_doc_matches_plain():
    assert read_bytes("errors/encrypted-auth.md") == read_bytes("smoke/path-str.md")
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for unlock path"
    )
    with pdfluent.Document(str(FIX("encrypted.pdf")), password="pw") as unlocked:
        unlocked_outcome = outcome(lambda: pe.to_markdown(unlocked))
    plain_outcome = outcome(lambda: pe.to_markdown(str(FIX("multi-page.pdf"))))
    assert unlocked_outcome == plain_outcome


def _assert_no_input_error(call):
    """Unknown options must never raise a bad-input error (engine may be absent)."""
    try:
        return call()
    except NotImplementedError:
        return None


def test_g176_unknown_kwargs_tolerated():
    _assert_no_input_error(
        lambda: pe.to_markdown(str(FIX("multi-page.pdf")), no_such_xyz=1)
    )
    pe.use_layout(False)
    try:
        _assert_no_input_error(
            lambda: pe.to_markdown(str(FIX("multi-page.pdf")), no_such_xyz=1)
        )
    finally:
        pe.use_layout(True)


def test_g177_use_ocr_values_tolerated():
    for value in ("always", 2):
        _assert_no_input_error(
            lambda v=value: pe.to_markdown(str(FIX("multi-page.pdf")), use_ocr=v)
        )


def test_g178_kv_extra_kw_tolerated():
    _assert_no_input_error(
        lambda: pe.get_key_values(str(FIX("form-fields.pdf")), bogus_kwarg=1)
    )


def test_errors_hierarchy():
    assert issubclass(pe.EmptyFileError, pe.FileDataError)
    with pytest.raises(pe.FileDataError):
        raise pe.EmptyFileError("empty")


def test_errors_check_call_contract():
    from pdfluent_extract._errors import CONTINUE, check_call

    assert check_call("to_markdown", object(), {}, layout=True) is CONTINUE
    assert check_call("to_markdown", None, {}, layout=True) is CONTINUE
    with pytest.raises(FileNotFoundError):
        check_call("to_markdown", FIX("does-not-exist.pdf"), {}, layout=True)
