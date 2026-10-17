#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Build fixtures/outlined.pdf: three text pages under a two-level outline.

The chunk suites of both bindings read this file: a page chunk's ``toc_items``
holds the outline entries that point at its page, so the outline below is the
contract -- one entry per page, plus a nested child on page 1 to pin the level:

- "Chapter 1" -> page 0 (an explicit ``/Dest``),
- "Chapter 2" -> page 1 (an explicit ``/A << /S /GoTo /D ... >>``),
  - "Section 2.1" -> page 1 (a ``/Dest`` again, at level 2),
- "Appendix A" -> page 2 (a ``/Dest``).

Like scripts/corpus-generate-mini.py this computes every /Length and every
xref offset itself, so the <<//Length defect (#203) cannot arise along this
road, and the output is byte-deterministic: no timestamps, no random ids.

Usage:
    python3 scripts/make-outlined-fixture.py [OUTPUT]
Default: fixtures/outlined.pdf from the repository root.
"""

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_OUT = REPO / "fixtures" / "outlined.pdf"

DATE = "D:20260101000000Z"
ID = b"pdfluent-outlined-fixture-0001"

PAGES = [
    ("Chapter 1", "The first chapter states the problem in one paragraph."),
    ("Chapter 2", "The second chapter works through it in two sections."),
    ("Appendix A", "The appendix holds what fitted nowhere else."),
]


def text_stream(title: str, body: str) -> bytes:
    """One page's content: a large bold title over two body lines."""
    lines = [
        "BT /F2 20 Tf 72 720 Td (%s) Tj ET" % title,
        "BT /F1 12 Tf 72 690 Td (%s) Tj ET" % body,
        "BT /F1 12 Tf 72 674 Td (Second line, so the page holds a paragraph.) Tj ET",
    ]
    return ("\n".join(lines) + "\n").encode("ascii")


def build() -> bytes:
    objects: list[bytes] = []

    def obj(body: bytes) -> int:
        objects.append(body)
        return len(objects)

    # Fonts first: their numbers are referenced by every page.
    regular = obj(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica"
        b" /Encoding /WinAnsiEncoding >>"
    )
    strong = obj(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold"
        b" /Encoding /WinAnsiEncoding >>"
    )

    page_ids: list[int] = []
    content_ids: list[int] = []
    for title, body in PAGES:
        data = text_stream(title, body)
        content_ids.append(
            obj(b"<< /Length %d >>\nstream\n" % len(data) + data + b"endstream")
        )
    # The page dicts reference contents and fonts; the /Pages parent is patched
    # in after, once its own number is known.
    for content in content_ids:
        page_ids.append(
            obj(
                b"<< /Type /Page /Parent 0 0 R /MediaBox [0 0 612 792]"
                b" /Contents %d 0 R /Resources << /Font << /F1 %d 0 R /F2 %d 0 R >> >> >>"
                % (content, regular, strong)
            )
        )
    pages_id = obj(
        b"<< /Type /Pages /Kids [%s] /Count %d >>"
        % (
            " ".join("%d 0 R" % n for n in page_ids).encode("ascii"),
            len(page_ids),
        )
    )
    for number in page_ids:
        objects[number - 1] = objects[number - 1].replace(
            b"/Parent 0 0 R", b"/Parent %d 0 R" % pages_id
        )

    # The outline. Item numbers are laid out in document order: Chapter 1,
    # Chapter 2 (with its child Section 2.1), Appendix A.
    first = len(objects) + 2  # outlines dict takes the next number
    c1, c2, child, appx = first, first + 1, first + 2, first + 3
    outlines_id = obj(
        b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count 4 >>"
        % (c1, appx)
    )
    assert outlines_id == first - 1
    assert obj(
        b"<< /Title (Chapter 1) /Parent %d 0 R /Next %d 0 R /Dest [%d 0 R /Fit] >>"
        % (outlines_id, c2, page_ids[0])
    ) == c1
    assert obj(
        b"<< /Title (Chapter 2) /Parent %d 0 R /Prev %d 0 R /Next %d 0 R"
        b" /First %d 0 R /Last %d 0 R /Count 1"
        b" /A << /S /GoTo /D [%d 0 R /Fit] >> >>"
        % (outlines_id, c1, appx, child, child, page_ids[1])
    ) == c2
    assert obj(
        b"<< /Title (Section 2.1) /Parent %d 0 R /Dest [%d 0 R /Fit] >>"
        % (c2, page_ids[1])
    ) == child
    assert obj(
        b"<< /Title (Appendix A) /Parent %d 0 R /Prev %d 0 R /Dest [%d 0 R /Fit] >>"
        % (outlines_id, c2, page_ids[2])
    ) == appx

    catalog_id = obj(
        b"<< /Type /Catalog /Pages %d 0 R /Outlines %d 0 R >>"
        % (pages_id, outlines_id)
    )
    info_id = obj(
        b"<< /Producer (PDFluent) /Creator (scripts/make-outlined-fixture.py)"
        b" /Title (PDFluent outlined-chunks fixture)"
        b" /CreationDate (%s) /ModDate (%s) >>" % (DATE.encode(), DATE.encode())
    )

    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".decode("latin-1").encode("latin-1"))
    offsets = [0]
    for number, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    xref_at = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for offset in offsets[1:]:
        out += b"%010d 00000 n \n" % offset
    out += (
        b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R /ID [<%s> <%s>] >>\n"
        % (len(objects) + 1, catalog_id, info_id, ID.hex().encode(), ID.hex().encode())
        + b"startxref\n%d\n%%%%EOF\n" % xref_at
    )
    return bytes(out)


def main(argv: list[str]) -> int:
    target = Path(argv[1]) if len(argv) > 1 else DEFAULT_OUT
    data = build()
    # Deterministic: building twice is byte-identical.
    assert build() == data, "the generator is not deterministic"
    target.write_bytes(data)
    print(f"wrote {target} ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
