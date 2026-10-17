# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Known-images fixture: one page, two image XObjects, written by hand.

Image A: 7x5 DeviceRGB, 8 bpc, FlateDecode.
    pixel(x, y) = (x*30, y*50, (x+y)*10) mod 256.
Image B: 4x3 DeviceGray, 8 bpc, uncompressed.
    pixel(x, y) = (x*60 + y*20) mod 256.

Drawing order on the page is A then B. The expected JSON lists the images
in that order, with sha256 over the decoded pixel bytes in row-major order
(y outer, x inner, top row first as stored in the stream).

Deterministic: no timestamps, no random IDs, fixed zlib level. Running
twice yields identical bytes for both outputs.

Usage:
    python3 fixtures/gen/known_images.py
    python3 fixtures/gen/known_images.py --pdf out.pdf --json out.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import zlib

WIDTH_A = 7
HEIGHT_A = 5
WIDTH_B = 4
HEIGHT_B = 3
BPC = 8


def pixels_a() -> bytes:
    out = bytearray()
    for y in range(HEIGHT_A):
        for x in range(WIDTH_A):
            out.append((x * 30) % 256)
            out.append((y * 50) % 256)
            out.append(((x + y) * 10) % 256)
    return bytes(out)


def pixels_b() -> bytes:
    out = bytearray()
    for y in range(HEIGHT_B):
        for x in range(WIDTH_B):
            out.append((x * 60 + y * 20) % 256)
    return bytes(out)


def build_pdf(raw_a: bytes, raw_b: bytes) -> bytes:
    comp_a = zlib.compress(raw_a, 6)
    content = (
        b"q\n7 0 0 5 10 140 cm\n/ImA Do\nQ\n" b"q\n4 0 0 3 10 100 cm\n/ImB Do\nQ\n"
    )
    header = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n"
    objs: list[bytes] = []
    objs.append(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")
    objs.append(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n")
    objs.append(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] "
        b"/Resources << /XObject << /ImA 4 0 R /ImB 5 0 R >> "
        b"/ProcSet [/PDF /ImageC] >> /Contents 6 0 R >>\nendobj\n"
    )
    objs.append(
        b"4 0 obj\n<< /Type /XObject /Subtype /Image /Width 7 /Height 5 "
        b"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode "
        b"/Length "
        + str(len(comp_a)).encode("ascii")
        + b" >>\nstream\n"
        + comp_a
        + b"\nendstream\nendobj\n"
    )
    objs.append(
        b"5 0 obj\n<< /Type /XObject /Subtype /Image /Width 4 /Height 3 "
        b"/ColorSpace /DeviceGray /BitsPerComponent 8 "
        b"/Length "
        + str(len(raw_b)).encode("ascii")
        + b" >>\nstream\n"
        + raw_b
        + b"\nendstream\nendobj\n"
    )
    objs.append(
        b"6 0 obj\n<< /Length "
        + str(len(content)).encode("ascii")
        + b" >>\nstream\n"
        + content
        + b"endstream\nendobj\n"
    )
    out = bytearray(header)
    offsets = [0]
    for body in objs:
        offsets.append(len(out))
        out += body
    xref_at = len(out)
    out += b"xref\n0 7\n"
    out += b"0000000000 65535 f \n"
    for i in range(1, 7):
        out += ("%010d 00000 n \n" % offsets[i]).encode("ascii")
    out += b"trailer\n<< /Size 7 /Root 1 0 R >>\n"
    out += b"startxref\n" + str(xref_at).encode("ascii") + b"\n%%EOF\n"
    return bytes(out)


def build_expected(raw_a: bytes, raw_b: bytes) -> dict:
    return {
        "images": [
            {
                "width": WIDTH_A,
                "height": HEIGHT_A,
                "colorspace": "DeviceRGB",
                "components": 3,
                "bpc": BPC,
                "sha256": hashlib.sha256(raw_a).hexdigest(),
            },
            {
                "width": WIDTH_B,
                "height": HEIGHT_B,
                "colorspace": "DeviceGray",
                "components": 1,
                "bpc": BPC,
                "sha256": hashlib.sha256(raw_b).hexdigest(),
            },
        ]
    }


def default_paths() -> tuple[str, str]:
    here = os.path.dirname(os.path.abspath(__file__))
    fixtures = os.path.normpath(os.path.join(here, ".."))
    return (
        os.path.join(fixtures, "known-images.pdf"),
        os.path.join(fixtures, "known-images.expected.json"),
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Write known-images fixture.")
    dflt_pdf, dflt_json = default_paths()
    parser.add_argument("--pdf", default=dflt_pdf)
    parser.add_argument("--json", default=dflt_json)
    args = parser.parse_args(argv)
    raw_a = pixels_a()
    raw_b = pixels_b()
    pdf = build_pdf(raw_a, raw_b)
    expected = build_expected(raw_a, raw_b)
    with open(args.pdf, "wb") as handle:
        handle.write(pdf)
    with open(args.json, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(expected, handle, sort_keys=True, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
