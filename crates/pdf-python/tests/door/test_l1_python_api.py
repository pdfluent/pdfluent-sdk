# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L1: the Python SDK surface the door builds on.

``render_markdown`` renders blocks to markdown, and ``TextSpan.flags``
carries each span's style as bits: 16 bold, 2 italic, 8 monospace,
32 hidden, 64 non-horizontal.

Run (needs the native wheel):
    cd crates/pdf-python
    maturin develop
    pytest tests/door/test_l1_python_api.py -v
"""

from __future__ import annotations

import pytest

pdfluent = pytest.importorskip("pdfluent")
from pdfluent import Document  # noqa: E402


def _styled_pdf() -> bytes:
    """One page: plain, bold, italic and monospaced lines, then hidden."""
    content = (
        b"BT /F1 12 Tf 72 700 Td (The plain line.) Tj ET\n"
        b"BT /F2 12 Tf 72 686 Td (The bold line.) Tj ET\n"
        b"BT /F3 12 Tf 72 672 Td (The slanted line.) Tj ET\n"
        b"BT /F4 12 Tf 72 658 Td (The fixed line.) Tj ET\n"
        b"3 Tr\n"
        b"BT /F1 12 Tf 72 644 Td (The hidden line.) Tj ET\n"
        b"0 Tr\n"
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R "
        b"/F3 7 0 R /F4 8 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"endstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
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


def _spans_by_line():
    """Group native spans (character granularity) into (text, flags) lines."""
    doc = Document(_styled_pdf())
    page = doc[0]
    rows = {}
    for block in page.extract_text_blocks():
        for span in block.spans:
            rows.setdefault(round(span.y, 1), []).append(span)
    out = []
    for y in sorted(rows, reverse=True):
        chars = sorted(rows[y], key=lambda span: span.x)
        out.append(("".join(span.text for span in chars), [span.flags for span in chars]))
    return out


def test_render_markdown_renders_blocks():
    doc = Document(_styled_pdf())
    out = pdfluent.render_markdown(doc.extract_blocks())
    assert "The plain line." in out
    assert "The bold line." in out
    assert out == doc.extract_markdown()


def test_span_flags_carry_style_bits():
    seen = {}
    for text, flags in _spans_by_line():
        assert flags and all(isinstance(bits, int) for bits in flags)
        assert len(set(flags)) == 1
        seen[text] = flags[0]
    assert seen["The plain line."] == 0
    assert seen["The bold line."] & 16
    assert seen["The slanted line."] & 2
    assert seen["The fixed line."] & 8


def test_hidden_span_carries_hidden_bit():
    seen = {text: bits[0] for text, bits in _spans_by_line()}
    assert seen["The hidden line."] & 32
    assert not seen["The plain line."] & 32
