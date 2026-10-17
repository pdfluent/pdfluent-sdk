# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L6: SPEC sections 15, 16, 17, 21. Flip markers as sections land.

Ordering matters: any batch call poisons later layout conversions in the
same process (G158), so the section 16 batch tests run last in this file
and this file runs last in the folder.
"""

import json
import os
import re
import subprocess
import sys
from contextlib import contextmanager

import pytest
from conftest import (
    FIX,
    SKIP_REASON,
    SPEC_DIR,
    SPEC_MISSING,
    assert_data_equal,
    read_json,
)
from conftest import read_text as _read_text

import pdfluent_extract as pe
from pdfluent_extract.helpers import document_layout as dl
from pdfluent_extract.helpers import get_text_lines as gtl
from pdfluent_extract.helpers import utils as du
from pdfluent_extract.ocr import OCRMode

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]


@contextmanager
def _page(fixture, number=0):
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for page input"
    )
    with pdfluent.Document(str(FIX(fixture))) as doc:
        yield doc[number]


def _legacy():
    pe.use_layout(False)


def test_g135_epub_layout():
    assert pe.to_markdown(str(FIX("sample.epub"))) == _read_text("nonpdf/epub-md.md")
    assert pe.to_text(str(FIX("sample.epub"))) == _read_text("nonpdf/epub-text.txt")


def test_g136_epub_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("sample.epub"))) == _read_text(
        "nonpdf/leg-epub-md.md"
    )


@pytest.mark.xfail(
    strict=True,
    reason="unpassable: record has no top-level 'format' to assert after exact match",
)
def test_g137_epub_json():
    got = json.loads(pe.to_json(str(FIX("sample.epub"))))
    assert_data_equal(got, read_json("nonpdf/epub-json.json"))
    assert got["format"] == "EPUB" and got["toc"] == []


@pytest.mark.xfail(
    strict=True,
    reason="unpassable: record has no top-level 'format' to assert after exact match",
)
def test_g138_png_empty():
    assert pe.to_markdown(str(FIX("sample.png"))) == ""
    assert pe.to_markdown(str(FIX("sample.png"))) == _read_text("nonpdf/png-md.md")
    assert pe.to_text(str(FIX("sample.png"))) == _read_text("nonpdf/png-text.txt")
    _legacy()
    assert pe.to_markdown(str(FIX("sample.png"))) == _read_text("nonpdf/leg-png-md.md")
    pe.use_layout(True)
    got = json.loads(pe.to_json(str(FIX("sample.png"))))
    assert_data_equal(got, read_json("nonpdf/png-json.json"))
    assert got["format"] == "PDF 1.7" and len(got["pages"]) == 1
    assert got["pages"][0]["boxes"] == []


def test_g139_xps_marks():
    assert pe.to_markdown(str(FIX("sample.xps"))) == "\n\n"
    assert pe.to_markdown(str(FIX("sample.xps"))) == _read_text("nonpdf/xps-md.md")
    assert pe.to_text(str(FIX("sample.xps"))) == _read_text("nonpdf/xps-text.txt")
    _legacy()
    assert pe.to_markdown(str(FIX("sample.xps"))) == _read_text("nonpdf/leg-xps-md.md")


def test_g140_docx_refused():
    with pytest.raises(pe.FileDataError):
        pe.to_markdown(str(FIX("sample.docx")))
    _legacy()
    with pytest.raises(pe.FileDataError):
        pe.to_markdown(str(FIX("sample.docx")))


def test_g141_bytes_refused():
    with open(FIX("multi-page.pdf"), "rb") as handle:
        data = handle.read()
    with pytest.raises(TypeError):
        pe.to_markdown(data)
    with pytest.raises(TypeError):
        pe.to_markdown(data, filename="multi-page.pdf")


def test_g142_scalar_docs():
    with pytest.raises(TypeError):
        pe.to_markdown(42)
    with pytest.raises(IndexError):
        pe.to_markdown(None)
    _legacy()
    assert pe.to_markdown(None) == ""
    assert pe.to_markdown(None) == _read_text("errors/leg-none.md")


def test_g159_auto_workers():
    cases = [
        ((1, 72, OCRMode.NEVER), "misc/auto-1.json", 1),
        ((4, 150, OCRMode.NEVER), "smoke/auto-workers.json", 4),
        ((10, 150, OCRMode.SELECT_DROP_OLD), "misc/auto-3.json", 8),
        ((50, 300, OCRMode.FORCE_KEEP_OLD), "misc/auto-2.json", 8),
    ]
    from pdfluent_extract import batch_converter as _bc

    for (count, dpi, mode), ref, want in cases:
        assert _bc.auto_workers(count, dpi, mode) == read_json(ref) == want


def test_g160_progress_layout(capsys, monkeypatch):
    monkeypatch.chdir(SPEC_DIR)
    out = pe.to_markdown("fixtures/multi-page.pdf", show_progress=True)
    assert out == _read_text("progress/sp-md.md")
    captured = capsys.readouterr()
    assert captured.out.startswith("Parsing 5 pages of 'fixtures/multi-page.pdf'...")
    assert "\x08" in captured.out and "(5/5)" in captured.out
    assert captured.err == ""
    out = pe.to_text("fixtures/multi-page.pdf", show_progress=True)
    assert out == _read_text("progress/sp-text.txt")
    captured = capsys.readouterr()
    assert captured.out.startswith("Parsing 5 pages of 'fixtures/multi-page.pdf'...")
    assert captured.err == ""
    out = pe.to_json("fixtures/multi-page.pdf", show_progress=True)
    assert json.loads(out) == read_json("progress/sp-json.json")
    captured = capsys.readouterr()
    assert captured.out.startswith("Parsing 5 pages of 'fixtures/multi-page.pdf'...")
    assert captured.err == ""


def test_g161_progress_legacy(capsys, monkeypatch):
    _legacy()
    monkeypatch.chdir(SPEC_DIR)
    out = pe.to_markdown("fixtures/multi-page.pdf", show_progress=True)
    assert out == _read_text("progress/sp-leg.md")
    captured = capsys.readouterr()
    assert captured.out.startswith("Processing fixtures/multi-page.pdf...")
    assert "\x08" in captured.out
    assert captured.err == ""


def test_g162_progress_before_messages(capsys, monkeypatch):
    monkeypatch.chdir(SPEC_DIR)
    pe.to_markdown("fixtures/multi-page.pdf", show_progress=True)
    captured = capsys.readouterr()
    assert captured.out.rindex("\x08") < captured.out.index(
        "=== Document parser messages ==="
    )


def test_g179_never_silent(capsys):
    out = pe.to_markdown(str(FIX("scanned-page.pdf")), use_ocr=OCRMode.NEVER)
    assert out == _read_text("ocr/cap-never-scan.md")
    captured = capsys.readouterr()
    assert "===" not in captured.out and "OCR on page" not in captured.out
    out = pe.to_markdown(str(FIX("scanned-words.pdf")), use_ocr=OCRMode.NEVER)
    assert out == _read_text("ocrwords/words-never-md.md")


def test_g180_parser_block_announced(capsys):
    for mode in (
        OCRMode.SELECT_DROP_OLD,
        OCRMode.SELECT_KEEP_OLD,
        OCRMode.FORCE_DROP_OLD,
        OCRMode.FORCE_KEEP_OLD,
    ):
        pe.to_markdown(str(FIX("scanned-words.pdf")), use_ocr=mode)
        captured = capsys.readouterr()
        assert "=== Document parser messages ===" in captured.out
        assert "Tesseract" in captured.out


def test_g181_forced_announces_page(capsys):
    for mode in (OCRMode.FORCE_KEEP_OLD, OCRMode.FORCE_DROP_OLD):
        pe.to_markdown(str(FIX("scanned-page.pdf")), use_ocr=mode)
        captured = capsys.readouterr()
        assert "OCR on page" in captured.out


def test_g182_forced_quiet_on_text(capsys):
    pe.to_markdown(str(FIX("multi-page.pdf")), use_ocr=OCRMode.FORCE_KEEP_OLD)
    captured = capsys.readouterr()
    assert "OCR on page" not in captured.out


def test_g183_default_finds_words():
    assert pe.to_markdown(str(FIX("scanned-words.pdf"))) == _read_text(
        "ocrwords/words-default-md.md"
    )


def test_g184_default_quiet_on_blank(capsys):
    out = pe.to_markdown(str(FIX("scanned-page.pdf")))
    assert out == _read_text("ocr/cap-def-scan.md")
    captured = capsys.readouterr()
    assert "OCR on page" not in captured.out
    out = pe.to_markdown(str(FIX("scanned-page.pdf")), use_ocr=OCRMode.SELECT_KEEP_OLD)
    assert out == _read_text("ocr/cap-sel-scan.md")


def test_g185_ocr_options_ignored():
    scan = str(FIX("scanned-page.pdf"))
    lang = pe.to_markdown(scan, ocr_language="xx-not-a-language")
    assert lang == _read_text("ocr/scan-lang-xx.md")
    assert pe.to_markdown(scan, ocr_dpi=72) == lang
    assert pe.to_markdown(scan, ocr_dpi=300) == lang
    assert pe.to_markdown(scan, force_ocr=False) == lang


def test_g186_force_flag_forces(capsys):
    out = pe.to_markdown(str(FIX("scanned-page.pdf")), force_ocr=True)
    assert out == _read_text("ocr/cap-focr-scan.md")
    captured = capsys.readouterr()
    assert "OCR on page" in captured.out


def test_g187_none_ocr_function():
    assert pe.to_markdown(
        str(FIX("scanned-page.pdf")), ocr_function=None, force_ocr=True
    ) == _read_text("misc2/ocr-fn-none.md")


def test_g188_custom_ocr_function_rejected():
    with pytest.raises(TypeError):
        pe.to_markdown(str(FIX("scanned-page.pdf")), ocr_function=len, force_ocr=True)


def test_g189_ocr_decisions():
    with _page("scanned-page.pdf") as scan:
        assert_data_equal(
            dl.make_ocr_decision(scan, OCRMode.NEVER),
            read_json("unit/ocr-dec-never.json"),
        )
    with _page("scanned-page.pdf") as scan:
        assert_data_equal(
            dl.make_ocr_decision(scan, OCRMode.FORCE_KEEP_OLD),
            read_json("unit/ocr-dec-force.json"),
        )
    with _page("multi-page.pdf") as text:
        assert_data_equal(
            dl.make_ocr_decision(text, OCRMode.FORCE_KEEP_OLD),
            read_json("unit/ocr-dec-text.json"),
        )


@pytest.mark.xfail(
    strict=True,
    reason="unpassable: approx(0.92) contradicts recorded 0.920142412185669",
)
def test_g190_page_analysis():
    keys = {
        "covered",
        "img_joins",
        "img_area",
        "txt_joins",
        "txt_area",
        "vec_joins",
        "vec_area",
        "chars_total",
        "chars_bad",
        "bad_areas",
        "ocr_spans",
        "pixmap",
        "vec_norects",
        "needs_ocr",
        "reason",
        "probability",
    }
    with _page("multi-page.pdf") as text:
        got = du.analyze_page(text)
    assert set(got) == keys
    assert_data_equal(got, read_json("unit/analyze-multi.json"))
    with _page("scanned-page.pdf") as scan:
        got = du.analyze_page(scan)
    assert set(got) == keys
    assert_data_equal(got, read_json("unit/analyze-scan.json"))
    assert got["needs_ocr"] is False and got["probability"] == pytest.approx(0.92)


def test_g191_default_ocr_function():
    assert callable(dl.select_ocr_function())


def test_g192_layout_locked_none():
    with _page("multi-page.pdf") as page:
        assert dl.get_layout_locked(page) is None
        assert read_json("unit/layout-locked.json") is None


def test_g193_lines_ocr_inert():
    with _page("scanned-words.pdf") as page:
        assert (
            gtl.get_text_lines(page, ocr=True) == _read_text("unit/gtl-ocr.json") == ""
        )


def _batch_inputs():
    return [str(FIX("multi-page.pdf")), str(FIX("lists.pdf"))]


def test_g143_batch_markdown(tmp_path):
    items = pe.convert_batch(
        _batch_inputs(), workers=1, out_dir=str(tmp_path), backend="md"
    )
    assert len(items) == 2
    for stem in ("multi-page", "lists"):
        ours = os.path.join(str(tmp_path), stem, stem + ".md")
        with open(ours, encoding="utf-8", newline="") as handle:
            got = handle.read()
        assert got == _read_text("batch/out-md/%s/%s.md" % (stem, stem))
        assert os.path.isfile(os.path.join(str(tmp_path), stem, "run-log.txt"))
    assert all(item.success for item in items)


def test_g144_batch_json(tmp_path):
    items = pe.convert_batch(
        [str(FIX("lists.pdf"))], workers=1, out_dir=str(tmp_path), backend="json"
    )
    assert len(items) == 1
    ours = os.path.join(str(tmp_path), "lists", "lists.json")
    assert json.load(open(ours)) == read_json("batch/out-bjson/lists/lists.json")


def test_g145_batch_backend_refused(tmp_path):
    for backend in ("text", "markdown"):
        with pytest.raises(ValueError):
            pe.convert_batch(
                [str(FIX("lists.pdf"))],
                workers=1,
                out_dir=str(tmp_path),
                backend=backend,
            )


def test_g146_batch_streaming_list(tmp_path):
    items = pe.convert_batch(
        [str(FIX("lists.pdf"))], workers=1, streaming=True, backend="md"
    )
    assert isinstance(items, list) and len(items) == 1


def test_g147_batch_no_dir():
    (item,) = pe.convert_batch([str(FIX("lists.pdf"))], workers=1, backend="md")
    assert item.success is False and item.output is None and item.error is None


def test_g148_batch_item_fields(tmp_path):
    (item,) = pe.convert_batch(
        [str(FIX("lists.pdf"))], workers=1, out_dir=str(tmp_path), backend="md"
    )
    assert item.success is True and item.error is None
    assert item.input == os.path.abspath(str(FIX("lists.pdf")))
    assert isinstance(item.output, str) and len(item.output) > 0


def test_g149_batch_options(tmp_path):
    items = pe.convert_batch(
        [str(FIX("lists.pdf"))],
        workers=1,
        out_dir=str(tmp_path),
        backend="md",
        options={},
    )
    assert len(items) == 1
    with pytest.raises(ValueError):
        pe.convert_batch(
            [str(FIX("lists.pdf"))],
            workers=1,
            out_dir=str(tmp_path),
            backend="md",
            options={"pages": [0]},
        )
    with pytest.raises(ValueError):
        pe.convert_batch(
            [str(FIX("header-footer.pdf"))],
            workers=1,
            out_dir=str(tmp_path),
            backend="md",
            options={"header": False},
        )


def test_g150_batch_workers(tmp_path):
    for workers in (0, 1, 2, None):
        items = pe.convert_batch(
            [str(FIX("lists.pdf"))],
            workers=workers,
            out_dir=str(tmp_path),
            backend="md",
        )
        assert len(items) == 1


def test_g151_batch_inputs_shape(tmp_path):
    with pytest.raises(TypeError):
        pe.convert_batch(
            str(FIX("lists.pdf")), workers=1, out_dir=str(tmp_path), backend="md"
        )
    assert pe.convert_batch([], workers=1, out_dir=str(tmp_path), backend="md") == []


def test_g152_batch_persistent(tmp_path):
    items = pe.convert_batch(
        [str(FIX("lists.pdf"))],
        workers=1,
        out_dir=str(tmp_path),
        backend="md",
        persistent=True,
    )
    assert len(items) == 1


def test_g153_batch_progress(tmp_path):
    calls = []

    def _callback(*args, **kwargs):
        calls.append({"args": list(args), "kwargs": dict(kwargs)})

    pe.convert_batch(
        _batch_inputs(),
        workers=1,
        out_dir=str(tmp_path),
        backend="md",
        progress_callback=_callback,
    )
    assert_data_equal(calls, read_json("misc/progress.json"))
    assert [tuple(c["args"]) for c in calls] == [(1, 2), (2, 2)]


def test_g154_batch_run_log(tmp_path):
    pe.convert_batch(_batch_inputs(), workers=1, out_dir=str(tmp_path), backend="md")
    with open(
        os.path.join(str(tmp_path), "multi-page", "run-log.txt"), encoding="utf-8"
    ) as fh:
        log = fh.read()
    assert re.search(r"^Start: 20\d\d-", log, re.M)
    assert re.search(r"^End: 20\d\d-", log, re.M)
    assert re.search(r"^OCR mode: .+", log, re.M)
    assert re.search(r"^Layout mode: .+", log, re.M)
    assert os.path.abspath(str(FIX("multi-page.pdf"))) in log
    assert "=== Document parser messages ===" in log
    assert log.rstrip().endswith("Status: success")


def test_g155_batch_drops_markers(tmp_path):
    single = pe.to_markdown(str(FIX("multi-page.pdf")))
    assert single == _read_text("smoke/path-str.md")
    pe.convert_batch(_batch_inputs(), workers=1, out_dir=str(tmp_path), backend="md")
    with open(
        os.path.join(str(tmp_path), "multi-page", "multi-page.md"), encoding="utf-8"
    ) as fh:
        batched = fh.read()
    assert batched == _read_text("batch/out-md/multi-page/multi-page.md")
    missing = [line for line in single.splitlines() if line not in batched.splitlines()]
    assert len(missing) == 5 and all("**" in line for line in missing)
    with open(os.path.join(str(tmp_path), "lists", "lists.md"), encoding="utf-8") as fh:
        assert fh.read() == _read_text("batch/out-md/lists/lists.md")


def test_g156_batch_consumer_misused(tmp_path):
    pe.convert_batch(
        [str(FIX("lists.pdf"))],
        workers=1,
        out_dir=str(tmp_path),
        backend="md",
        consumer=pe.to_markdown,
        consumer_kwargs={},
    )
    with open(
        os.path.join(str(tmp_path), "lists", "run-log.txt"), encoding="utf-8"
    ) as fh:
        log = fh.read()
    assert "Status: error" in log and "no such file" in log


def test_g157_convert_document():
    from pdfluent_extract import batch_converter as _bc

    with pytest.raises(AttributeError):
        _bc.convert_document(
            str(FIX("lists.pdf")),
            backend="md",
            options=None,
            consumer=None,
            consumer_kwargs=None,
        )
    helper = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "_fresh_convert.py"
    )
    with open(helper, "w", encoding="utf-8") as handle:
        handle.write(
            "import sys, os\n"
            "sys.path.insert(0, %r)\n"
            "from pdfluent_extract import batch_converter as bc, to_markdown\n"
            "try:\n"
            "    bc.convert_document(%r, backend='md', options={}, "
            "consumer=to_markdown, consumer_kwargs={})\n"
            "    print('returned')\n"
            "except FileNotFoundError:\n"
            "    print('FileNotFoundError')\n"
            "except Exception as exc:\n"
            "    print(type(exc).__name__)\n"
            % (
                os.path.join(
                    os.path.dirname(os.path.abspath(__file__)), "..", "..", "python"
                ),
                str(FIX("lists.pdf")),
            )
        )
    try:
        proc = subprocess.run(
            [sys.executable, helper], capture_output=True, text=True, timeout=120
        )
    finally:
        os.unlink(helper)
    assert proc.stdout.strip().splitlines()[-1] == "FileNotFoundError"


def test_g158_batch_poisons_layout(tmp_path):
    pe.convert_batch(
        [str(FIX("lists.pdf"))], workers=1, out_dir=str(tmp_path), backend="md"
    )
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("lists.pdf")))
    _legacy()
    assert isinstance(pe.to_markdown(str(FIX("lists.pdf"))), str)
