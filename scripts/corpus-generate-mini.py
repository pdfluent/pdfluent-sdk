#!/usr/bin/env python3
"""Generate a minimal test corpus for smoke-testing corpus scripts.

Creates ~10 small synthetic PDFs with specific features.
These are committed to git in tests/corpus-mini/.

Usage:
    ./scripts/corpus-generate-mini.py [OUTPUT_DIR]

Default: tests/corpus-mini/
"""

import struct
import sys
import zlib
from pathlib import Path


def pdf_header(version: str = "1.7") -> bytes:
    return f"%PDF-{version}\n%\xe2\xe3\xcf\xd3\n".encode("latin-1")


def pdf_object(obj_num: int, content: str) -> bytes:
    return f"{obj_num} 0 obj\n{content}\nendobj\n".encode()


def pdf_stream(obj_num: int, dictionary: str, data: bytes) -> bytes:
    return (
        f"{obj_num} 0 obj\n{dictionary}/Length {len(data)}>>\nstream\n".encode()
        + data
        + b"\nendstream\nendobj\n"
    )


def build_xref_trailer(offsets: list[int], root_ref: str, size: int) -> bytes:
    xref = b"xref\n"
    xref += f"0 {len(offsets) + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 g \n".encode()  # Changed to 'g' for generation
    # Actually, for active objects it should be 'n' (in use)
    # Let me fix this
    return xref  # Will be rebuilt below


def make_simple_pdf(pages: int = 1, version: str = "1.7", extra_catalog: str = "",
                    extra_objects: list[tuple[int, str]] | None = None,
                    page_content: bytes = b"BT /F1 12 Tf 100 700 Td (Test page) Tj ET") -> bytes:
    """Build a minimal valid PDF with the given properties."""
    parts = []
    offsets = []
    obj_count = 0

    def add_obj(content: str) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_object(obj_count, content))
        return obj_count

    def add_stream(dictionary: str, data: bytes) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_stream(obj_count, dictionary, data))
        return obj_count

    parts.append(pdf_header(version))

    # Font
    font_obj = add_obj("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>")

    # Content stream(s)
    content_objs = []
    for _ in range(pages):
        cobj = add_stream("<</", page_content)
        content_objs.append(cobj)

    # Pages
    page_objs = []
    pages_obj_num = obj_count + pages + 1  # Reserve number for Pages object
    for i in range(pages):
        pobj = add_obj(
            f"<</Type /Page /Parent {pages_obj_num} 0 R "
            f"/MediaBox [0 0 612 792] "
            f"/Contents {content_objs[i]} 0 R "
            f"/Resources <</Font <</F1 {font_obj} 0 R>>>>>>"
            f">>"
        )
        page_objs.append(pobj)

    # Pages object
    kids = " ".join(f"{p} 0 R" for p in page_objs)
    actual_pages_num = add_obj(
        f"<</Type /Pages /Kids [{kids}] /Count {pages}>>"
    )
    assert actual_pages_num == pages_obj_num

    # Extra objects
    if extra_objects:
        for _, content in extra_objects:
            add_obj(content)

    # Catalog
    catalog = add_obj(
        f"<</Type /Catalog /Pages {pages_obj_num} 0 R{extra_catalog}>>"
    )

    # Xref table
    xref_offset = sum(len(p) for p in parts)
    xref = f"xref\n0 {obj_count + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode()

    trailer = (
        f"trailer\n<</Size {obj_count + 1} /Root {catalog} 0 R>>\n"
        f"startxref\n{xref_offset}\n%%EOF\n"
    ).encode()

    parts.append(xref)
    parts.append(trailer)
    return b"".join(parts)


def generate_simple(out: Path):
    """1-page PDF, no special features."""
    out.write_bytes(make_simple_pdf())


def generate_acroform(out: Path):
    """PDF with AcroForm fields."""
    # Add an AcroForm reference in catalog
    data = make_simple_pdf(
        extra_catalog=" /AcroForm <</Fields [] /DR <</Font <</Helv 1 0 R>>>> /DA (/Helv 10 Tf 0 g)>>"
    )
    out.write_bytes(data)


def generate_multi_page(out: Path):
    """50-page PDF."""
    out.write_bytes(make_simple_pdf(pages=50))


def generate_xfa_form(out: Path):
    """PDF that contains /XFA key (minimal stub)."""
    xfa_xml = b"<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"></xdp:xdp>"
    # Build PDF with XFA reference
    parts = []
    offsets = []
    obj_count = 0

    def add_obj(content: str) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_object(obj_count, content))
        return obj_count

    def add_stream(dictionary: str, data: bytes) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_stream(obj_count, dictionary, data))
        return obj_count

    parts.append(pdf_header())

    font_obj = add_obj("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>")
    content_obj = add_stream("<</", b"BT /F1 12 Tf 100 700 Td (XFA Form) Tj ET")
    xfa_stream = add_stream("<</", xfa_xml)
    page_obj = add_obj(
        f"<</Type /Page /Parent 5 0 R "
        f"/MediaBox [0 0 612 792] /Contents {content_obj} 0 R "
        f"/Resources <</Font <</F1 {font_obj} 0 R>>>>>>"
    )
    pages_obj = add_obj(f"<</Type /Pages /Kids [{page_obj} 0 R] /Count 1>>")
    catalog = add_obj(
        f"<</Type /Catalog /Pages {pages_obj} 0 R "
        f"/AcroForm <</XFA {xfa_stream} 0 R /Fields []>>>>"
    )

    xref_offset = sum(len(p) for p in parts)
    xref = f"xref\n0 {obj_count + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode()
    trailer = (
        f"trailer\n<</Size {obj_count + 1} /Root {catalog} 0 R>>\n"
        f"startxref\n{xref_offset}\n%%EOF\n"
    ).encode()
    parts.append(xref)
    parts.append(trailer)
    out.write_bytes(b"".join(parts))


def generate_signed(out: Path):
    """PDF with /Sig and /ByteRange markers (stub, not cryptographically valid)."""
    data = make_simple_pdf(
        extra_catalog=(
            " /AcroForm <</Fields [99 0 R] /SigFlags 3>>"
        ),
        extra_objects=[
            (99, "<</Type /Annot /Subtype /Widget /FT /Sig /T (Signature1) "
                 "/V <</Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached "
                 "/ByteRange [0 100 200 300] /Contents <00>>>>>"
                 "/Rect [0 0 0 0]>>")
        ],
    )
    out.write_bytes(data)


def generate_pdfa(out: Path):
    """PDF claiming PDF/A-2b compliance via XMP metadata."""
    xmp = (
        '<?xpacket begin="\xef\xbb\xbf" id="W5M0MpCehiHzreSzNTczkc9d"?>'
        '<x:xmpmeta xmlns:x="adobe:ns:meta/">'
        '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
        '<rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">'
        '<pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>'
        '</rdf:Description>'
        '</rdf:RDF></x:xmpmeta><?xpacket end="w"?>'
    ).encode("utf-8")

    # Build with metadata stream
    parts = []
    offsets = []
    obj_count = 0

    def add_obj(content: str) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_object(obj_count, content))
        return obj_count

    def add_stream(dictionary: str, data: bytes) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_stream(obj_count, dictionary, data))
        return obj_count

    parts.append(pdf_header())
    font_obj = add_obj("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>")
    content_obj = add_stream("<</", b"BT /F1 12 Tf 100 700 Td (PDF/A-2b) Tj ET")
    meta_obj = add_stream("<</Type /Metadata /Subtype /XML", xmp)
    page_obj = add_obj(
        f"<</Type /Page /Parent 5 0 R /MediaBox [0 0 612 792] "
        f"/Contents {content_obj} 0 R /Resources <</Font <</F1 {font_obj} 0 R>>>>>>"
    )
    pages_obj = add_obj(f"<</Type /Pages /Kids [{page_obj} 0 R] /Count 1>>")
    catalog = add_obj(
        f"<</Type /Catalog /Pages {pages_obj} 0 R /Metadata {meta_obj} 0 R>>"
    )

    xref_offset = sum(len(p) for p in parts)
    xref = f"xref\n0 {obj_count + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode()
    trailer = (
        f"trailer\n<</Size {obj_count + 1} /Root {catalog} 0 R>>\n"
        f"startxref\n{xref_offset}\n%%EOF\n"
    ).encode()
    parts.append(xref)
    parts.append(trailer)
    out.write_bytes(b"".join(parts))


def generate_encrypted(out: Path):
    """PDF with /Encrypt dictionary (stub, not actually encrypted)."""
    data = make_simple_pdf()
    # Inject /Encrypt marker into the trailer
    data = data.replace(
        b"trailer\n<<",
        b"trailer\n<</Encrypt <</Filter /Standard /V 2 /R 3 /O <00> /U <00> /P -3904>>",
    )
    out.write_bytes(data)


def generate_scanned(out: Path):
    """PDF with an image XObject instead of text (simulates scan)."""
    # 10x10 white pixel image (raw RGB)
    width, height = 10, 10
    img_data = b"\xff\xff\xff" * width * height

    parts = []
    offsets = []
    obj_count = 0

    def add_obj(content: str) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_object(obj_count, content))
        return obj_count

    def add_stream(dictionary: str, data: bytes) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_stream(obj_count, dictionary, data))
        return obj_count

    parts.append(pdf_header())
    img_obj = add_stream(
        f"<</Type /XObject /Subtype /Image /Width {width} /Height {height} "
        f"/ColorSpace /DeviceRGB /BitsPerComponent 8",
        img_data,
    )
    content = f"q {width} 0 0 {height} 100 600 cm /Im1 Do Q".encode()
    content_obj = add_stream("<</", content)
    page_obj = add_obj(
        f"<</Type /Page /Parent 4 0 R /MediaBox [0 0 612 792] "
        f"/Contents {content_obj} 0 R "
        f"/Resources <</XObject <</Im1 {img_obj} 0 R>>>>>>"
    )
    pages_obj = add_obj(f"<</Type /Pages /Kids [{page_obj} 0 R] /Count 1>>")
    catalog = add_obj(f"<</Type /Catalog /Pages {pages_obj} 0 R>>")

    xref_offset = sum(len(p) for p in parts)
    xref = f"xref\n0 {obj_count + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode()
    trailer = (
        f"trailer\n<</Size {obj_count + 1} /Root {catalog} 0 R>>\n"
        f"startxref\n{xref_offset}\n%%EOF\n"
    ).encode()
    parts.append(xref)
    parts.append(trailer)
    out.write_bytes(b"".join(parts))


def generate_malformed(out: Path):
    """Deliberately corrupt PDF (truncated xref)."""
    data = make_simple_pdf()
    # Truncate the last 50 bytes to corrupt the xref/trailer
    out.write_bytes(data[:-50])


def generate_zugferd(out: Path):
    """PDF with ZUGFeRD/Factur-X XML attachment marker."""
    zugferd_xml = (
        b'<?xml version="1.0" encoding="UTF-8"?>'
        b'<rsm:CrossIndustryInvoice '
        b'xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100">'
        b'</rsm:CrossIndustryInvoice>'
    )

    parts = []
    offsets = []
    obj_count = 0

    def add_obj(content: str) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_object(obj_count, content))
        return obj_count

    def add_stream(dictionary: str, data: bytes) -> int:
        nonlocal obj_count
        obj_count += 1
        offsets.append(sum(len(p) for p in parts))
        parts.append(pdf_stream(obj_count, dictionary, data))
        return obj_count

    parts.append(pdf_header())

    font_obj = add_obj("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>")
    content_obj = add_stream("<</", b"BT /F1 12 Tf 100 700 Td (ZUGFeRD Invoice) Tj ET")

    # Embedded file stream
    ef_stream = add_stream(
        "<</Type /EmbeddedFile /Subtype /text#2Fxml",
        zugferd_xml,
    )
    filespec = add_obj(
        f"<</Type /Filespec /F (factur-x.xml) /UF (factur-x.xml) "
        f"/EF <</F {ef_stream} 0 R>> /AFRelationship /Alternative>>"
    )

    # XMP with PDF/A-3 + Factur-X metadata
    xmp = (
        '<?xpacket begin="\xef\xbb\xbf" id="W5M0MpCehiHzreSzNTczkc9d"?>'
        '<x:xmpmeta xmlns:x="adobe:ns:meta/">'
        '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
        '<rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">'
        '<pdfaid:part>3</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>'
        '</rdf:Description>'
        '</rdf:RDF></x:xmpmeta><?xpacket end="w"?>'
    ).encode("utf-8")
    meta_obj = add_stream("<</Type /Metadata /Subtype /XML", xmp)

    page_obj = add_obj(
        f"<</Type /Page /Parent {obj_count + 2} 0 R /MediaBox [0 0 612 792] "
        f"/Contents {content_obj} 0 R /Resources <</Font <</F1 {font_obj} 0 R>>>>>>"
    )
    pages_obj = add_obj(f"<</Type /Pages /Kids [{page_obj} 0 R] /Count 1>>")
    catalog = add_obj(
        f"<</Type /Catalog /Pages {pages_obj} 0 R "
        f"/Metadata {meta_obj} 0 R "
        f"/Names <</EmbeddedFiles <</Names [(factur-x.xml) {filespec} 0 R]>>>> "
        f"/AF [{filespec} 0 R]>>"
    )

    xref_offset = sum(len(p) for p in parts)
    xref = f"xref\n0 {obj_count + 1}\n".encode()
    xref += b"0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode()
    trailer = (
        f"trailer\n<</Size {obj_count + 1} /Root {catalog} 0 R>>\n"
        f"startxref\n{xref_offset}\n%%EOF\n"
    ).encode()
    parts.append(xref)
    parts.append(trailer)
    out.write_bytes(b"".join(parts))


def main():
    output_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tests/corpus-mini")
    output_dir.mkdir(parents=True, exist_ok=True)

    generators = [
        ("simple.pdf", generate_simple),
        ("acroform.pdf", generate_acroform),
        ("signed-rsa.pdf", generate_signed),
        ("pdfa-2b.pdf", generate_pdfa),
        ("multi-page.pdf", generate_multi_page),
        ("encrypted.pdf", generate_encrypted),
        ("scanned.pdf", generate_scanned),
        ("xfa-form.pdf", generate_xfa_form),
        ("malformed.pdf", generate_malformed),
        ("zugferd.pdf", generate_zugferd),
    ]

    print(f"Generating mini test corpus in {output_dir}/")
    for name, gen_fn in generators:
        path = output_dir / name
        gen_fn(path)
        size = path.stat().st_size
        print(f"  {name}: {size} bytes")

    print(f"\nGenerated {len(generators)} test PDFs.")


if __name__ == "__main__":
    main()
