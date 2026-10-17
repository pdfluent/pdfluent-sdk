"""``render_markdown`` and ``TextSpan.flags`` (lane L1).

Python lost the module-level ``render_markdown`` and the style bits on
``TextSpan``: spans carry text and geometry but no longer say how they
were set. The rebuilt binding renders Markdown from blocks again and
reports each span's style as flag bits.

Only the shape is pinned: ``render_markdown`` is a callable on the
package, and ``flags`` is an int that differs between a bold span and a
plain one. The front door's own span records carry the same ``flags``
(G131); the bit values are ours to choose.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_restored_render_markdown_and_span_flags.py -v
"""

from __future__ import annotations

import pytest

pdfluent = pytest.importorskip("pdfluent")
from pdfluent import Document  # noqa: E402


def _plain_and_bold_pdf() -> bytes:
    """One page: a plain line over a bold one."""
    content = (
        b"BT /F1 12 Tf 72 700 Td (The plain line.) Tj ET\n"
        b"BT /F2 12 Tf 72 686 Td (The bold line.) Tj ET\n"
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"endstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
    ]
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for offset in offsets:
        out += b"%010d 00000 n \n" % offset
    out += (
        b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
        % (len(objects) + 1, xref)
    )
    return bytes(out)


def _all_spans() -> list:
    doc = Document(_plain_and_bold_pdf())
    page = doc[0]
    spans = [span for block in page.extract_text_blocks() for span in block.spans]
    assert len(spans) >= 2
    return spans


class TestRestoredRenderMarkdownAndSpanFlags:
    def test_render_markdown_is_a_package_callable(self) -> None:
        assert hasattr(pdfluent, "render_markdown")
        assert callable(pdfluent.render_markdown)

    def test_span_flags_are_ints(self) -> None:
        for span in _all_spans():
            assert isinstance(span.flags, int)

    def test_bold_and_plain_spans_differ_in_flags(self) -> None:
        flags = {span.flags for span in _all_spans()}
        assert len(flags) >= 2
