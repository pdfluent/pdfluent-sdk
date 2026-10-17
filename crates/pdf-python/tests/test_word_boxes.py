"""Word boxes at the Python surface (#663).

The word type carries exactly ``page, block, line, word, text, bbox, dir``
with the Rust ``Word``'s meaning -- ``bbox`` a list of 4 floats in PDF user
space, y up -- and both the document-level and the page-level accessor hand
it back. The fixture is built in the test, Courier at known positions, so the
known box below is arithmetic: at 12 points Courier advances 7.2 a glyph from
x 72, and the positioned word gap stands one advance wide.
"""

import pytest

pdfluent = pytest.importorskip("pdfluent")

Document = pdfluent.Document

SIZE = 12.0
ADVANCE = 0.6 * SIZE


def positioned_courier_pdf(text):
    """One page drawing ``text`` glyph by glyph at exact origins."""
    operations = []
    pos = 0.0
    for ch in text:
        if ch == " ":
            pos += 1.0
            continue
        x = 72.0 + ADVANCE * pos
        pos += 1.0
        operations.append(
            "BT /F1 %.1f Tf 1 0 0 1 %.1f 700 Tm (%s) Tj ET" % (SIZE, x, ch)
        )
    content = "\n".join(operations).encode("ascii")
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
    ]
    pdf = [b"%PDF-1.7\n"]
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(sum(map(len, pdf)))
        pdf.append(b"%d 0 obj\n" % number + body + b"\nendobj\n")
    xref_at = sum(map(len, pdf))
    pdf.append(b"xref\n0 %d\n" % (len(objects) + 1))
    pdf.append(b"0000000000 65535 f \n")
    for offset in offsets:
        pdf.append(b"%010d 00000 n \n" % offset)
    pdf.append(
        b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
        % (len(objects) + 1, xref_at)
    )
    return b"".join(pdf)


def test_word_exposes_the_seven_fields_with_their_types():
    with Document(positioned_courier_pdf("AB CD")) as doc:
        words = doc.extract_words()
    assert [w.text for w in words] == ["AB", "CD"]
    for word in words:
        assert type(word.page) is int
        assert type(word.block) is int
        assert type(word.line) is int
        assert type(word.word) is int
        assert type(word.text) is str
        assert type(word.bbox) is list
        assert len(word.bbox) == 4
        assert all(type(side) is float for side in word.bbox)
        assert type(word.dir) is float


def test_a_known_word_box_matches_its_glyphs_within_half_a_point():
    with Document(positioned_courier_pdf("AB CD")) as doc:
        words = doc.extract_words()
    assert len(words) == 2
    expected = [[72.0, 700.0, 86.4, 712.0], [93.6, 700.0, 108.0, 712.0]]
    for word, want in zip(words, expected):
        assert len(word.bbox) == 4
        for actual, side in zip(word.bbox, want):
            assert abs(actual - side) < 0.5, word.bbox


def test_document_and_page_accessors_agree():
    with Document(positioned_courier_pdf("AB CD")) as doc:
        assert doc.page_count == 1
        from_document = doc.extract_words()
        from_page = doc[0].words()
    assert [w.text for w in from_page] == [w.text for w in from_document]
    assert [w.word for w in from_page] == [0, 1]
    for word in from_page:
        assert (word.page, word.block, word.line) == (0, 0, 0)
