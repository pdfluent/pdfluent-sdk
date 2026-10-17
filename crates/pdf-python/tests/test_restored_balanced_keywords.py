"""Balanced-tier keywords on the Python extraction path (lane L7b).

``extract_markdown``/``extract_blocks`` lost the balanced-tier keywords while
the CLI kept them (``--quality balanced --model-dir ...``): today
``quality``/``model_dir``/``keep_furniture`` are rejected as unexpected
keywords. The rebuilt binding accepts them again.

The keyword names mirror the engine's own ``ExtractOptions`` fields; the
``"balanced"`` value mirrors the CLI. Anything model-shaped is
unspecified; these run where no models are installed, so the balanced tier
refuses -- the refusal itself is the proof the keywords arrived.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_restored_balanced_keywords.py -v
"""

from __future__ import annotations

import pytest

pdfluent = pytest.importorskip("pdfluent")
from pdfluent import Document, PdfluentError  # noqa: E402


def _one_page_pdf() -> bytes:
    """One born-digital page: two lines of body text."""
    content = (
        b"BT /F1 12 Tf 72 700 Td (The plant treats the water of the valley.) Tj ET\n"
        b"BT /F1 12 Tf 72 686 Td (It runs through the night.) Tj ET\n"
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"endstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
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


class TestRestoredBalancedKeywords:
    def test_blocks_refuse_the_balanced_tier(self) -> None:
        # Needs no models: blocks have no balanced answer, so this refusal
        # fires before any weights are looked for.
        doc = Document(_one_page_pdf())
        with pytest.raises(PdfluentError):
            doc.extract_blocks(quality="balanced")

    def test_markdown_honours_quality_model_dir_and_keep_furniture(self) -> None:
        # No models installed: the tier refuses naming the install. The
        # refusal -- not a TypeError for unknown keywords -- is the proof
        # all three keywords arrived.
        doc = Document(_one_page_pdf())
        with pytest.raises(PdfluentError):
            doc.extract_markdown(
                quality="balanced",
                model_dir="/nonexistent-model-dir",
                keep_furniture=True,
            )
