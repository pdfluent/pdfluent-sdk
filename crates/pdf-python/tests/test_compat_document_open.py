# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Non-PDF input on the compatible open path (#646).

EPUB, images and HTML open as PDF through lib.open, so convert_to_pdf
works on all three. Named formats without a converter fail as
unsupported. The real-Chrome tests only run with PDFLUENT_REAL_CHROME=1;
without it they report SKIPPED, and a stand-in Chrome covers the
plumbing on every run.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_document_open.py -v
"""

from __future__ import annotations

import io
import os
import stat
import zipfile

import pytest

pdfluent = pytest.importorskip("pdfluent")

HERE = os.path.dirname(__file__)
FIXTURES = os.path.normpath(
    os.path.join(HERE, "..", "..", "pdfluent", "tests", "fixtures")
)
PNG_10X6 = os.path.join(FIXTURES, "tiny_10x6.png")
PNG_144DPI = os.path.join(FIXTURES, "tiny_144dpi.png")
JPG_150DPI = os.path.join(FIXTURES, "tiny_150dpi.jpg")

REAL_CHROME = os.environ.get("PDFLUENT_REAL_CHROME") == "1"

_FAKE_PDF = (
    b"%PDF-1.7\n"
    b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n"
    b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n"
    b"3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 100 100]/Contents 4 0 R/Resources<<>>>>endobj\n"
    b"4 0 obj<</Length 0>>stream\n\nendstream\nendobj\n"
    b"trailer<</Root 1 0 R>>\n"
)


def _minimal_epub() -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as epub:
        epub.writestr("mimetype", "application/epub+zip")
        epub.writestr(
            "META-INF/container.xml",
            '<?xml version="1.0"?><container version="1.0"'
            ' xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>'
            '<rootfile full-path="OEBPS/content.opf"'
            ' media-type="application/oebps-package+xml"/></rootfiles></container>',
        )
        epub.writestr(
            "OEBPS/content.opf",
            '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf"'
            ' version="3.0" unique-identifier="id">'
            '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
            "<dc:title>t</dc:title></metadata><manifest>"
            '<item id="c0" href="ch1.xhtml" media-type="application/xhtml+xml"/>'
            '</manifest><spine><itemref idref="c0"/></spine></package>',
        )
        epub.writestr(
            "OEBPS/ch1.xhtml",
            '<?xml version="1.0" encoding="utf-8"?>'
            '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head>'
            "<body><h1>Chapter One</h1><p>Some body text here.</p></body></html>",
        )
    return buf.getvalue()


def test_epub_opens_and_converts() -> None:
    doc = pdfluent.open(_minimal_epub())
    try:
        assert doc.page_count >= 1
        assert "Chapter One" in doc[0].get_text()
        blob = doc.convert_to_pdf()
    finally:
        doc.close()
    back = pdfluent.open(stream=blob, filetype="pdf")
    try:
        assert back.page_count >= 1
    finally:
        back.close()


def test_png_opens_sized_to_the_image() -> None:
    doc = pdfluent.open(PNG_10X6)
    try:
        assert doc.page_count == 1
        rect = doc[0].rect
        assert (rect.x0, rect.y0, rect.x1, rect.y1) == (0, 0, 10, 6)
        blob = doc.convert_to_pdf()
    finally:
        doc.close()
    back = pdfluent.open(stream=blob, filetype="pdf")
    try:
        assert back.page_count == 1
    finally:
        back.close()


def test_image_dpi_sizes_the_page() -> None:
    doc = pdfluent.open(PNG_144DPI)
    try:
        rect = doc[0].rect
        assert abs(rect.x1 - 6.0) < 0.01
        assert abs(rect.y1 - 6.0) < 0.01
    finally:
        doc.close()
    doc = pdfluent.open(JPG_150DPI)
    try:
        rect = doc[0].rect
        assert abs(rect.x1 - 14.4) < 0.01
        assert abs(rect.y1 - 9.6) < 0.01
    finally:
        doc.close()


def test_unsupported_format_fails_by_name() -> None:
    with pytest.raises(
        pdfluent.PdfluentError, match="not supported in this version: cbz"
    ):
        pdfluent.open(stream=b"junk bytes here", filetype="cbz")
    with pytest.raises(
        pdfluent.PdfluentError, match="not supported in this version: mobi"
    ):
        pdfluent.open(stream=b"junk bytes here", filetype="mobi")


def _write_fake_chrome(tmp_path) -> tuple[str, str]:
    script = str(tmp_path / "fake-chrome.sh")
    args_log = str(tmp_path / "args.txt")
    with open(script, "w") as handle:
        handle.write(
            "#!/bin/sh\n"
            f'echo "$@" > {args_log}\n'
            'out=""\n'
            'for a in "$@"; do case "$a" in'
            ' --print-to-pdf=*) out="${a#--print-to-pdf=}";; esac; done\n'
            f"printf '%s' '{_FAKE_PDF.decode()}' > \"$out\"\n"
        )
    os.chmod(script, os.stat(script).st_mode | stat.S_IXUSR)
    return script, args_log


@pytest.mark.skipif(os.name != "posix", reason="the stand-in Chrome is a shell script")
def test_html_plumbing_uses_env_chrome(tmp_path, monkeypatch) -> None:
    script, args_log = _write_fake_chrome(tmp_path)
    monkeypatch.setenv("PDFLUENT_CHROME", script)
    doc = pdfluent.open(stream=b"<html><body><p>x</p></body></html>", filetype="html")
    try:
        assert doc.page_count == 1
    finally:
        doc.close()
    with open(args_log) as handle:
        args = handle.read()
    assert "--headless" in args
    assert "--print-to-pdf=" in args


@pytest.mark.skipif(
    not REAL_CHROME, reason="set PDFLUENT_REAL_CHROME=1 for real Chrome"
)
def test_html_content_through_real_chrome(monkeypatch) -> None:
    monkeypatch.delenv("PDFLUENT_CHROME", raising=False)
    doc = pdfluent.open(
        stream=b"<html><body><h1>Hello from HTML</h1></body></html>", filetype="html"
    )
    try:
        assert doc.page_count >= 1
        assert "Hello from HTML" in doc[0].get_text()
    finally:
        doc.close()


@pytest.mark.skipif(
    not REAL_CHROME, reason="set PDFLUENT_REAL_CHROME=1 for real Chrome"
)
def test_html_missing_chrome_message(monkeypatch) -> None:
    monkeypatch.delenv("PDFLUENT_CHROME", raising=False)
    try:
        doc = pdfluent.open(stream=b"<html></html>", filetype="html")
    except pdfluent.PdfluentError as exc:
        assert str(exc) == (
            "HTML conversion requires Chrome or Chromium; see"
            " https://pdfluent.com/how-to/files/#generate-pdf-from-html-rust"
        )
    else:
        doc.close()
        pytest.skip("a Chrome is installed; the missing branch needs one without")
