# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Plain embedded-image API on ``Document``.

``page_images`` lists a page's stored images, ``extract_image_data``
returns one image's encoded bytes with header facts, and
``decode_image_pixels`` decodes to raw samples. The fixture is built in
the test: one page holding a raw 2x2 grey image, so the known pixels
below are the samples the builder wrote.
"""

import pytest

pdfluent = pytest.importorskip("pdfluent")

Document = pdfluent.Document
PdfluentError = pdfluent.PdfluentError

GRAY_SAMPLES = bytes([0, 128, 192, 255])


def gray_image_pdf():
    """One page with a raw 2x2 DeviceGray image under ``/G0``."""
    content = b"q\nQ\n"
    imagedict = (
        b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 "
        b"/ColorSpace /DeviceGray /BitsPerComponent 8 "
        b"/Length %d >>\nstream\n" % len(GRAY_SAMPLES)
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Contents 4 0 R /Resources 5 0 R >>",
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        b"<< /XObject << /G0 6 0 R >> >>",
        imagedict + GRAY_SAMPLES + b"\nendstream",
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


def test_page_images_lists_the_stored_image():
    with Document(gray_image_pdf()) as doc:
        images = doc.page_images(0)
    assert len(images) == 1
    image = images[0]
    assert image["width"] == 2
    assert image["height"] == 2
    assert image["bits_per_component"] == 8
    assert image["components"] == 1
    assert image["color_space"] == "DeviceGray"
    assert image["name"] == "G0"
    assert image["has_mask"] is False
    assert type(image["id"]) is int


def test_page_images_past_the_last_page_carries_the_range_code():
    with Document(gray_image_pdf()) as doc:
        with pytest.raises(PdfluentError) as excinfo:
            doc.page_images(1)
    assert excinfo.value.code == "E-PAGE-OUT-OF-RANGE"


def test_extract_image_data_returns_png_bytes_with_header():
    with Document(gray_image_pdf()) as doc:
        image_id = doc.page_images(0)[0]["id"]
        image = doc.extract_image_data(image_id)
    assert image["width"] == 2
    assert image["height"] == 2
    assert image["encoding"] == "png"
    assert image["data"][:4] == b"\x89PNG"
    assert image["masked"] is False


def test_extract_image_data_rejects_an_unknown_id():
    with Document(gray_image_pdf()) as doc:
        with pytest.raises(PdfluentError):
            doc.extract_image_data(9999)


def test_decode_image_pixels_returns_the_fixture_samples():
    with Document(gray_image_pdf()) as doc:
        image_id = doc.page_images(0)[0]["id"]
        image = doc.decode_image_pixels(image_id)
    assert image["width"] == 2
    assert image["height"] == 2
    assert image["components"] == 1
    assert image["color_space"] == "DeviceGray"
    assert bytes(image["pixels"]) == GRAY_SAMPLES


def test_decode_image_pixels_rejects_an_unknown_id():
    with Document(gray_image_pdf()) as doc:
        with pytest.raises(PdfluentError):
            doc.decode_image_pixels(9999)
