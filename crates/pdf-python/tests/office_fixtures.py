# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Minimal Office files the tests build themselves: no downloads, no binaries.

One builder per format, each holding exactly the parts the matching
`*-extract` crate reads, plus `[Content_Types].xml` so the binding routes on
the container's content types. Each carries a known sentence the tests assert
on. Mirrors the Rust builders in `crates/{docx,pptx,xlsx}-extract/tests/`.
"""

from __future__ import annotations

import io
import zipfile

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
P_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
S_NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"

DOCX_SENTENCE = "The quarterly figures are final."
PPTX_SENTENCE = "Quarterly review."
XLSX_SENTENCE = "Revenue"


def _container(parts: dict[str, str]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for name, content in parts.items():
            zf.writestr(name, content)
    return buf.getvalue()


def _content_types(override: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
        f"{override}\n"
        "</Types>"
    )


def docx_bytes() -> bytes:
    """A one-paragraph Word file holding ``DOCX_SENTENCE``."""
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:document xmlns:w="{W_NS}">\n'
        "<w:body>"
        '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>'
        "<w:r><w:t>Quarterly report</w:t></w:r></w:p>"
        f"<w:p><w:r><w:t>{DOCX_SENTENCE}</w:t></w:r></w:p>"
        "</w:body></w:document>"
    )
    return _container(
        {
            "[Content_Types].xml": _content_types(
                '<Override PartName="/word/document.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.wordprocessingml.document.main+xml"/>'
            ),
            "word/document.xml": document,
        }
    )


def docx_with_chart_bytes() -> bytes:
    """A Word file with an embedded chart and spreadsheet part.

    Real `.docx` files name `spreadsheetml` for an embedded worksheet and
    `drawingml` for a chart. Routing must follow the main document part
    (`/word/document.xml`), not a substring over the container: the old
    check sent this file to the xlsx extractor.
    """
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:document xmlns:w="{W_NS}">\n'
        "<w:body>"
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>"
        "<w:r><w:t>Quarterly report</w:t></w:r></w:p>"
        f"<w:p><w:r><w:t>{DOCX_SENTENCE}</w:t></w:r></w:p>"
        "</w:body></w:document>"
    )
    return _container(
        {
            "[Content_Types].xml": _content_types(
                '<Override PartName="/word/document.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.wordprocessingml.document.main+xml"/>\n'
                '<Override PartName="/word/embeddings/Microsoft_Excel_Worksheet1.xlsx" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.spreadsheetml.sheet"/>\n'
                '<Override PartName="/word/charts/chart1.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.drawingml.chart+xml"/>'
            ),
            "word/document.xml": document,
        }
    )


def pptx_bytes() -> bytes:
    """A one-slide deck: a title and one body paragraph holding ``PPTX_SENTENCE``."""
    slide = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<p:sld xmlns:p="{P_NS}" xmlns:a="{A_NS}">'
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/>'
        "<p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"
        "<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/>"
        '<a:ext cx="9144000" cy="6858000"/></a:xfrm></p:grpSpPr>'
        '<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/>'
        '<p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>'
        "<p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>"
        f"<a:p><a:r><a:t>{PPTX_SENTENCE}</a:t></a:r></a:p>"
        "</p:txBody></p:sp>"
        '<p:sp><p:nvSpPr><p:cNvPr id="3" name="Body"/>'
        "<p:nvPr/></p:nvSpPr>"
        "<p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>"
        "<a:p><a:r><a:t>Revenue is up year over year.</a:t></a:r></a:p>"
        "</p:txBody></p:sp>"
        "</p:spTree></p:cSld></p:sld>"
    )
    presentation = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<p:presentation xmlns:p="{P_NS}" xmlns:r="{R_NS}">'
        '<p:sldSz cx="9144000" cy="6858000"/>'
        '<p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>'
        "</p:presentation>"
    )
    pres_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId2" Type="{R_NS}/slide" Target="slides/slide1.xml"/>'
        "</Relationships>"
    )
    return _container(
        {
            "[Content_Types].xml": _content_types(
                '<Override PartName="/ppt/presentation.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.presentationml.presentation.main+xml"/>'
            ),
            "ppt/presentation.xml": presentation,
            "ppt/_rels/presentation.xml.rels": pres_rels,
            "ppt/slides/slide1.xml": slide,
        }
    )


def xlsx_bytes() -> bytes:
    """A one-sheet workbook: a header row and one data row holding ``XLSX_SENTENCE``."""
    workbook = (
        '<?xml version="1.0"?>\n'
        f'<workbook xmlns="{S_NS}" xmlns:r="{R_NS}">'
        '<sheets><sheet name="Q1" sheetId="1" r:id="rId1"/></sheets>'
        "</workbook>"
    )
    rels = (
        '<?xml version="1.0"?>\n'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId1" Type="{R_NS}/worksheet" Target="worksheets/sheet1.xml"/>'
        "</Relationships>"
    )
    sheet = (
        '<?xml version="1.0"?>\n'
        f'<worksheet xmlns="{S_NS}">'
        "<sheetData>"
        '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>'
        '<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>42</v></c></row>'
        "</sheetData></worksheet>"
    )
    shared = (
        '<?xml version="1.0"?>\n'
        f'<sst xmlns="{S_NS}" count="3" uniqueCount="3">'
        f"<si><t>{XLSX_SENTENCE}</t></si>"
        "<si><t>Units</t></si>"
        "<si><t>North</t></si>"
        "</sst>"
    )
    return _container(
        {
            "[Content_Types].xml": _content_types(
                '<Override PartName="/xl/workbook.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument'
                '.spreadsheetml.sheet.main+xml"/>'
            ),
            "xl/workbook.xml": workbook,
            "xl/_rels/workbook.xml.rels": rels,
            "xl/worksheets/sheet1.xml": sheet,
            "xl/sharedStrings.xml": shared,
        }
    )
