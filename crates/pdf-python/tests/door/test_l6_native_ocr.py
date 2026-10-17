# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L6: native OCR availability and the Python recogniser bridge.

Run: cd crates/pdf-python; maturin develop; pytest tests/door/test_l6_native_ocr.py
"""

import pytest


def _pdfluent():
    return pytest.importorskip("pdfluent")


def _one_page_pdf():
    text = "(The plant treats the water of the valley.)"
    stream = "BT /F1 12 Tf 72 720 Td %s Tj ET" % text
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        "/Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        "<< /Length %d >>\nstream\n%s\nendstream" % (len(stream), stream),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for num, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += ("%d 0 obj\n%s\nendobj\n" % (num, body)).encode("latin-1")
    xref = len(out)
    out += ("xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)).encode("latin-1")
    for offset in offsets:
        out += ("%010d 00000 n \n" % offset).encode("latin-1")
    out += (
        "trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
        % (len(objs) + 1, xref)
    ).encode("latin-1")
    return bytes(out)


def test_native_ocr_engine_available_reports_a_bool():
    _pdfluent()
    from pdfluent import _native

    assert _native.ocr_engine_available() in (True, False)


def test_ocr_function_bridge_reads_pages():
    pdfluent = _pdfluent()
    seen = []

    def record(image, width, height, dpi):
        seen.append((bytes(image), width, height, dpi))
        return []

    with pdfluent.Document(_one_page_pdf()) as doc:
        default = doc.extract_markdown()
        forced = doc.extract_markdown(ocr="force", ocr_function=record)
    assert "valley" in default
    assert seen, "the recogniser was never called"
    assert seen[0][1] > 0 and seen[0][2] > 0 and seen[0][3] == 150
    assert "valley" not in forced


def test_unknown_ocr_option_rejected():
    pdfluent = _pdfluent()
    with pdfluent.Document(_one_page_pdf()) as doc:
        with pytest.raises(pdfluent.PdfluentValidationError):
            doc.extract_markdown(ocr="sometimes")


def test_non_callable_ocr_function_rejected():
    pdfluent = _pdfluent()
    with pdfluent.Document(_one_page_pdf()) as doc:
        with pytest.raises(TypeError):
            doc.extract_markdown(ocr="auto", ocr_function=42)
