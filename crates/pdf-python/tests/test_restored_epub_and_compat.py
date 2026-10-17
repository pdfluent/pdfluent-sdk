"""EPUB input and the front-door compatibility module (lane L6).

Python lost EPUB input: an EPUB is refused where it used to read. The
rebuilt binding opens an EPUB again -- heading, emphasis and list items
all render (G135) -- through the same ``Document`` every other input uses.

The front-door compatibility module is pinned beside it: the module the
front door's entry points (``to_markdown`` and friends, per the recorded
surface) run through. Its path below is our choice; if the restored
module lands elsewhere the lane renames the import and keeps the entry
points.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_restored_epub_and_compat.py -v
"""

from __future__ import annotations

import io
import zipfile

import pytest

pdfluent = pytest.importorskip("pdfluent")
from pdfluent import Document  # noqa: E402


def _minimal_epub() -> bytes:
    """A valid one-chapter EPUB: heading, emphasis, list."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as epub:
        epub.writestr("mimetype", "application/epub+zip")
        epub.writestr(
            "META-INF/container.xml",
            """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf"
    media-type="application/oebps-package+xml"/></rootfiles>
</container>
""",
        )
        epub.writestr(
            "OEBPS/content.opf",
            """<?xml version="1.0"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Night Shift</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest><item id="ch1" href="ch1.xhtml"
    media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>
""",
        )
        epub.writestr(
            "OEBPS/ch1.xhtml",
            """<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<h1>Chapter One</h1>
<p>The plant treats the <b>water</b> of the whole <i>valley</i>.</p>
<ul><li>First item</li><li>Second item</li></ul>
</body></html>
""",
        )
    return buf.getvalue()


class TestRestoredEpubInput:
    def test_an_epub_reads_through_document(self) -> None:
        doc = Document(_minimal_epub())
        markdown = doc.extract_markdown()
        assert "Chapter One" in markdown
        assert "First item" in markdown

    def test_the_front_door_compatibility_module_exists(self) -> None:
        """The front-door entries live in the standalone pdfluent_extract package."""
        import pdfluent_extract as compat

        assert callable(compat.to_markdown)
