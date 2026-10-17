# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""SPEC section 1 (public surface) and section 20 extras live here; L0 owns both."""

import inspect

import pytest
from conftest import SKIP_REASON, SPEC_MISSING

import pdfluent_extract as pe
from pdfluent_extract._mode import is_layout_mode

pytestmark = pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON)


def test_g1_default_engine_is_layout():
    assert is_layout_mode() is True


def test_g2_use_layout_switch():
    for yes in (True, 1, "yes"):
        assert pe.use_layout(yes) is None
        assert is_layout_mode() is True
    for no in (False, 0, None):
        assert pe.use_layout(no) is None
        assert is_layout_mode() is False


def test_g3_llama_reader_stub():
    with pytest.raises(NotImplementedError):
        pe.LlamaMarkdownReader("whatever.pdf")
    with pytest.raises(NotImplementedError):
        pe.LlamaMarkdownReader()
    with pytest.raises(NotImplementedError):
        pe.LlamaMarkdownReader("whatever.pdf", no_such_kwarg=1)


def test_surface_top_level_names():
    for name in (
        "to_markdown",
        "to_text",
        "to_json",
        "use_layout",
        "convert_batch",
        "get_key_values",
        "LlamaMarkdownReader",
        "VERSION",
        "version",
        "VERSION_TUPLE",
        "version_tuple",
        "batch_converter",
        "helpers",
        "ocr",
        "worker_sizing",
        "versions_file",
    ):
        assert hasattr(pe, name), name


def test_surface_version_attributes():
    assert pe.VERSION == "1.28.2"
    assert pe.version == "1.28.2"
    assert pe.VERSION_TUPLE == (1, 28, 2)
    assert pe.version_tuple == (1, 28, 2)
    assert pe.versions_file.VERSION == pe.VERSION
    assert pe.versions_file.VERSION_TUPLE == pe.VERSION_TUPLE


def test_surface_all_covers_public_names():
    assert set(pe.__all__) >= {
        "to_markdown",
        "to_text",
        "to_json",
        "use_layout",
        "convert_batch",
        "get_key_values",
        "LlamaMarkdownReader",
        "VERSION",
        "version",
        "VERSION_TUPLE",
        "version_tuple",
    }
    for name in pe.__all__:
        assert hasattr(pe, name), name


def test_surface_submodule_layout():
    from pdfluent_extract import batch_converter, helpers, ocr, worker_sizing

    assert hasattr(ocr, "analyze_page") and hasattr(ocr, "compute_ocr_features")
    for sub in (
        "legacy",
        "document_layout",
        "get_text_lines",
        "multi_column",
        "progress",
        "utils",
    ):
        assert hasattr(helpers, sub), sub
    assert hasattr(batch_converter, "convert_batch")
    assert hasattr(worker_sizing, "auto_workers")


def test_surface_key_signatures():
    from pdfluent_extract.helpers import document_layout, legacy

    assert str(inspect.signature(pe.to_markdown)) == "(*args, **kwargs)"
    assert str(inspect.signature(pe.to_text)) == "(*args, **kwargs)"
    assert str(inspect.signature(pe.to_json)) == "(*args, **kwargs)"
    assert str(inspect.signature(pe.use_layout)) == "(yes)"
    assert str(inspect.signature(pe.get_key_values)) == "(doc, xrefs=False, **kwargs)"
    params = inspect.signature(pe.convert_batch).parameters
    assert list(params)[:1] == ["inputs"]
    assert params["backend"].default == "md"
    assert params["workers"].default is None
    assert params["streaming"].default is False
    legacy_params = inspect.signature(legacy.to_markdown).parameters
    assert legacy_params["table_strategy"].default == "lines_strict"
    assert legacy_params["image_size_limit"].default == 0.05
    assert legacy_params["force_text"].default is True
    parse_params = inspect.signature(document_layout.parse_document).parameters
    assert parse_params["ocr_language"].default == "eng"
    assert parse_params["force_text"].default is False
    assert int(parse_params["use_ocr"].default) == 2


def test_surface_classes_and_modes():
    from pdfluent_extract.helpers import document_layout
    from pdfluent_extract.ocr import OCRMode

    assert [m.value for m in OCRMode] == [0, 1, 2, 3, 4]
    assert OCRMode(2) is OCRMode.SELECT_KEEP_OLD
    assert issubclass(OCRMode, int)
    from pdfluent_extract import batch_converter

    assert str(inspect.signature(batch_converter.BatchResult)) == (
        "(input, success, error, output)"
    )
    assert "get_header_id" in dir(document_layout.IdentifyHeaders)
    assert "get_header_id" in dir(document_layout.TocHeaders)


def test_surface_error_classes():
    assert issubclass(pe.EmptyFileError, pe.FileDataError)
    assert issubclass(pe.FileDataError, Exception)
