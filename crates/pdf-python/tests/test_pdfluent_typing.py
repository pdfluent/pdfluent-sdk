"""mypy --strict CI target for pdfluent type stubs.

This file is NOT a pytest test suite.  It is a statically-checked example
file whose sole purpose is to pass:

    mypy --strict tests/test_pdfluent_typing.py

Every line that assigns a typed variable or calls a typed function is a
type-check assertion.  The file must also be importable (no runtime errors
from the type annotations themselves).

Run from crates/pdf-python/:
    mypy --strict --python-path python tests/test_pdfluent_typing.py
"""

from __future__ import annotations

from typing import List, Optional

from pdfluent import (
    Annotation,
    Bookmark,
    ComplianceIssue,
    ComplianceReport,
    Document,
    DocumentInfo,
    FormField,
    LicenseInfo,
    Page,
    PageGeometry,
    RedactReport,
    RenderedImage,
    TextBlock,
    TextSpan,
    activate_license,
    decrypt_pdf,
    merge_pdfs,
    open_pdf,
    validate_pdfa,
)
from pdfluent import (
    PdfluentEncryptedError,
    PdfluentError,
    PdfluentGeometryError,
    PdfluentIoError,
    PdfluentLicenseError,
    PdfluentLimitError,
    PdfluentPageRangeError,
    PdfluentParseError,
    PdfluentRenderError,
    PdfluentValidationError,
)

# ---------------------------------------------------------------------------
# Document lifecycle
# ---------------------------------------------------------------------------

def use_document_from_path(path: str) -> int:
    doc: Document = Document(path)
    count: int = doc.page_count
    return count


def use_document_from_bytes(data: bytes) -> int:
    doc: Document = Document(data)
    return doc.page_count


def use_context_manager(path: str) -> int:
    with Document(path) as doc:
        count: int = doc.page_count
    return count


def use_open_pdf(path: str) -> Document:
    doc: Document = open_pdf(path)
    return doc


def use_open_pdf_password(path: str, pw: str) -> Document:
    doc: Document = open_pdf(path, password=pw)
    return doc


# ---------------------------------------------------------------------------
# Document metadata and bookmarks
# ---------------------------------------------------------------------------

def use_metadata(doc: Document) -> None:
    info: DocumentInfo = doc.metadata
    title: Optional[str] = info.title
    author: Optional[str] = info.author
    subject: Optional[str] = info.subject
    keywords: Optional[str] = info.keywords
    creator: Optional[str] = info.creator
    producer: Optional[str] = info.producer
    _repr: str = repr(info)
    _ = (title, author, subject, keywords, creator, producer, _repr)


def use_bookmarks(doc: Document) -> None:
    bmarks: List[Bookmark] = doc.bookmarks
    for bm in bmarks:
        _title: str = bm.title
        _page: Optional[int] = bm.page
        _children: List[Bookmark] = bm.children
        _repr: str = repr(bm)
        _ = (_title, _page, _children, _repr)


# ---------------------------------------------------------------------------
# Page access
# ---------------------------------------------------------------------------

def use_page_indexing(doc: Document) -> None:
    page: Page = doc[0]
    last: Page = doc[-1]
    _ = (page, last)


def use_page_iteration(doc: Document) -> List[Page]:
    pages: List[Page] = list(doc)
    return pages


def use_len(doc: Document) -> int:
    n: int = len(doc)
    return n


# ---------------------------------------------------------------------------
# Page properties
# ---------------------------------------------------------------------------

def use_page_properties(page: Page) -> None:
    idx: int = page.index
    w: float = page.width
    h: float = page.height
    rot: int = page.rotation
    geom: PageGeometry = page.geometry
    _repr: str = repr(page)
    _ = (idx, w, h, rot, geom, _repr)


def use_page_geometry(geom: PageGeometry) -> None:
    mb: tuple[float, float, float, float] = geom.media_box
    cb: tuple[float, float, float, float] = geom.crop_box
    rot: int = geom.rotation
    w: float = geom.width
    h: float = geom.height
    dims: tuple[int, int] = geom.pixel_dimensions(150.0)
    _repr: str = repr(geom)
    _ = (mb, cb, rot, w, h, dims, _repr)


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------

def use_render(page: Page) -> RenderedImage:
    img: RenderedImage = page.render()
    return img


def use_render_options(page: Page) -> RenderedImage:
    img: RenderedImage = page.render(
        dpi=300.0,
        width=1024,
        height=768,
        background=(0.0, 0.0, 0.0, 1.0),
    )
    return img


def use_thumbnail(page: Page) -> RenderedImage:
    thumb: RenderedImage = page.thumbnail(max_dimension=128)
    return thumb


def use_rendered_image(img: RenderedImage) -> None:
    w: int = img.width
    h: int = img.height
    px: bytes = img.pixels
    _repr: str = repr(img)
    _ = (w, h, px, _repr)


def use_render_all(doc: Document) -> List[RenderedImage]:
    imgs: List[RenderedImage] = doc.render_all(dpi=72.0)
    return imgs


# ---------------------------------------------------------------------------
# Text extraction
# ---------------------------------------------------------------------------

def use_extract_text(page: Page) -> str:
    text: str = page.extract_text()
    return text


def use_extract_text_doc(doc: Document) -> str:
    text: str = doc.extract_text(0)
    return text


def use_text_blocks(page: Page) -> List[TextBlock]:
    blocks: List[TextBlock] = page.extract_text_blocks()
    for block in blocks:
        _text: str = block.text
        _spans: List[TextSpan] = block.spans
        _repr: str = repr(block)
        _str: str = str(block)
        _ = (_text, _spans, _repr, _str)
    return blocks


def use_text_span(span: TextSpan) -> None:
    _text: str = span.text
    _x: float = span.x
    _y: float = span.y
    _fs: float = span.font_size
    # G1 fields — Optional, must handle None
    fn: Optional[str] = span.font_name
    bold: Optional[bool] = span.is_bold
    italic: Optional[bool] = span.is_italic
    color: Optional[tuple[float, float, float]] = span.color
    _repr: str = repr(span)
    if fn is not None:
        _fn_upper: str = fn.upper()
    if bold is not None:
        _bold_not: bool = not bold
    if italic is not None:
        _italic_not: bool = not italic
    if color is not None:
        _r: float = color[0]
    _ = (_text, _x, _y, _fs, _repr)


def use_search(doc: Document) -> List[int]:
    pages: List[int] = doc.search("invoice")
    return pages


# ---------------------------------------------------------------------------
# Form fields
# ---------------------------------------------------------------------------

def use_form_fields(doc: Document) -> List[FormField]:
    fields: List[FormField] = doc.get_form_fields()
    for f in fields:
        _name: str = f.name
        _ft: str = f.field_type
        _val: Optional[str] = f.value
        _pg: Optional[int] = f.page
        _repr: str = repr(f)
        _ = (_name, _ft, _val, _pg, _repr)
    return fields


def use_set_form_field(doc: Document) -> bool:
    result: bool = doc.set_form_field("Name", "Jane Doe")
    return result


# ---------------------------------------------------------------------------
# Annotations
# ---------------------------------------------------------------------------

def use_annotations(doc: Document) -> List[Annotation]:
    annots: List[Annotation] = doc.get_annotations(0)
    for a in annots:
        _pg: int = a.page
        _tp: str = a.annot_type
        _rect: tuple[float, float, float, float] = a.rect
        _cnt: Optional[str] = a.contents
        _auth: Optional[str] = a.author
        _repr: str = repr(a)
        _ = (_pg, _tp, _rect, _cnt, _auth, _repr)
    return annots


def use_add_annotation(doc: Document) -> None:
    doc.add_annotation(0, "highlight", (72.0, 700.0, 300.0, 720.0))
    doc.add_annotation(0, "freetext", (72.0, 600.0, 300.0, 650.0), content="Note")


# ---------------------------------------------------------------------------
# Redaction
# ---------------------------------------------------------------------------

def use_redact(doc: Document) -> RedactReport:
    report: RedactReport = doc.redact_text("confidential")
    _mf: int = report.matches_found
    _ar: int = report.areas_redacted
    _pa: int = report.pages_affected
    _repr: str = repr(report)
    _ = (_mf, _ar, _pa, _repr)
    return report


def use_redact_page(doc: Document) -> RedactReport:
    report: RedactReport = doc.redact_text("confidential", page=0)
    return report


# ---------------------------------------------------------------------------
# Encryption / decryption
# ---------------------------------------------------------------------------

def use_encrypt(doc: Document, tmp: str) -> None:
    doc.encrypt(tmp + "/enc.pdf", "secret")
    doc.encrypt(tmp + "/enc2.pdf", "secret", owner_password="owner")


def use_decrypt_method(doc: Document, tmp: str) -> None:
    doc.decrypt(tmp + "/dec.pdf", "secret")


def use_decrypt_pdf(tmp: str) -> None:
    decrypt_pdf(tmp + "/enc.pdf", tmp + "/dec.pdf", "secret")


def use_save(doc: Document, path: str) -> None:
    doc.save(path)


# ---------------------------------------------------------------------------
# Merge
# ---------------------------------------------------------------------------

def use_merge(paths: List[str], output: str) -> None:
    merge_pdfs(paths, output)


# ---------------------------------------------------------------------------
# PDF/A validation
# ---------------------------------------------------------------------------

def use_validate_pdfa(path: str) -> ComplianceReport:
    report: ComplianceReport = validate_pdfa(path)
    _ok: bool = report.is_compliant
    _ec: int = report.error_count
    _wc: int = report.warning_count
    _level: Optional[str] = report.pdfa_level
    _repr: str = repr(report)
    issues: List[ComplianceIssue] = report.issues
    for issue in issues:
        _rule: str = issue.rule
        _sev: str = issue.severity
        _msg: str = issue.message
        _loc: Optional[str] = issue.location
        _irepr: str = repr(issue)
        _ = (_rule, _sev, _msg, _loc, _irepr)
    _ = (_ok, _ec, _wc, _level, _repr, issues)
    return report


# ---------------------------------------------------------------------------
# License activation
# ---------------------------------------------------------------------------

def use_activate_license(key: str) -> Optional[LicenseInfo]:
    try:
        info: LicenseInfo = activate_license(key)
        _licensee: str = info.licensee
        _company: str = info.company
        _tier: str = info.tier
        _exp: int = info.expires_at
        _seats: int = info.seats
        _ = (_licensee, _company, _tier, _exp, _seats)
        return info
    except PdfluentLicenseError:
        return None


# ---------------------------------------------------------------------------
# Exception hierarchy — every subclass must be catchable via base
# ---------------------------------------------------------------------------

def use_exception_hierarchy(path: str) -> None:
    try:
        Document(path)
    except PdfluentParseError:
        pass
    except PdfluentEncryptedError:
        pass
    except PdfluentIoError:
        pass
    except PdfluentPageRangeError:
        pass
    except PdfluentRenderError:
        pass
    except PdfluentValidationError:
        pass
    except PdfluentGeometryError:
        pass
    except PdfluentLimitError:
        pass
    except PdfluentLicenseError:
        pass
    except PdfluentError:
        pass
