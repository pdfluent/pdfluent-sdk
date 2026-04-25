#!/usr/bin/env python3
"""Generate large synthetic PDF stress fixtures.

The fixtures are intentionally generated on demand and ignored by git. They
combine a deep page tree, many page content streams, and large embedded font
subset streams so parser benchmarks see large, structured PDFs instead of a
single opaque padding blob.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Callable


DEFAULT_SIZES = ("100M", "500M", "1G")
DEFAULT_OUTPUT_DIR = Path("benchmarks/stress-fixtures")
MIB = 1024 * 1024
GIB = 1024 * MIB
WRITE_CHUNK = MIB


@dataclass(frozen=True)
class RawObject:
    obj_id: int
    body: bytes

    def size(self) -> int:
        return len(obj_header(self.obj_id)) + len(self.body) + len(b"\nendobj\n")

    def write(self, out: BinaryIO) -> None:
        out.write(obj_header(self.obj_id))
        out.write(self.body)
        out.write(b"\nendobj\n")


@dataclass(frozen=True)
class StreamObject:
    obj_id: int
    dictionary: str
    length: int
    writer: Callable[[BinaryIO, int], None]

    def header(self) -> bytes:
        return (
            f"{self.obj_id} 0 obj\n"
            f"<< {self.dictionary}/Length {self.length} >>\n"
            "stream\n"
        ).encode("ascii")

    def size(self) -> int:
        return len(self.header()) + self.length + len(b"\nendstream\nendobj\n")

    def write(self, out: BinaryIO) -> None:
        out.write(self.header())
        self.writer(out, self.length)
        out.write(b"\nendstream\nendobj\n")


PdfObject = RawObject | StreamObject


def obj_header(obj_id: int) -> bytes:
    return f"{obj_id} 0 obj\n".encode("ascii")


def raw(obj_id: int, body: str) -> RawObject:
    return RawObject(obj_id, body.encode("ascii"))


def parse_size(value: str) -> tuple[str, int]:
    normalized = value.strip().upper()
    aliases = {
        "100M": 100 * MIB,
        "100MB": 100 * MIB,
        "500M": 500 * MIB,
        "500MB": 500 * MIB,
        "1G": GIB,
        "1GB": GIB,
    }
    if normalized in aliases:
        return canonical_size_name(normalized), aliases[normalized]

    match = re.fullmatch(r"(\d+)([KMG])B?", normalized)
    if not match:
        raise argparse.ArgumentTypeError(
            f"invalid size {value!r}; use 100M, 500M, 1G, or <N>[KMG]"
        )
    amount = int(match.group(1))
    unit = match.group(2)
    multiplier = {"K": 1024, "M": MIB, "G": GIB}[unit]
    size = amount * multiplier
    if size < 2 * MIB:
        raise argparse.ArgumentTypeError("stress fixtures must be at least 2M")
    return canonical_size_name(normalized), size


def canonical_size_name(value: str) -> str:
    return value.replace("B", "").lower()


def write_repeated(out: BinaryIO, pattern: bytes, length: int) -> None:
    if not pattern:
        raise ValueError("pattern must not be empty")
    block = (pattern * ((WRITE_CHUNK // len(pattern)) + 1))[:WRITE_CHUNK]
    remaining = length
    while remaining > 0:
        take = min(remaining, len(block))
        out.write(block[:take])
        remaining -= take


def content_stream_writer(page_index: int, stream_index: int) -> Callable[[BinaryIO, int], None]:
    def writer(out: BinaryIO, length: int) -> None:
        text = (
            f"q 1 0 0 1 0 0 cm BT /F1 10 Tf 72 720 Td "
            f"(Stress page {page_index:04d} stream {stream_index:02d}) Tj ET Q\n"
        ).encode("ascii")
        write_repeated(out, text, length)

    return writer


def binary_stream_writer(label: str) -> Callable[[BinaryIO, int], None]:
    seed = hashlib.sha256(label.encode("ascii")).digest()
    pattern = bytearray()
    while len(pattern) < 4096:
        seed = hashlib.sha256(seed).digest()
        pattern.extend(seed)
    block = bytes(pattern)

    def writer(out: BinaryIO, length: int) -> None:
        write_repeated(out, block, length)

    return writer


def make_plan(
    target_bytes: int,
    *,
    pages: int,
    depth: int,
    streams_per_page: int,
    font_subsets: int,
) -> list[PdfObject]:
    if pages < 1:
        raise ValueError("pages must be >= 1")
    if depth < 1:
        raise ValueError("depth must be >= 1")
    if streams_per_page < 1:
        raise ValueError("streams_per_page must be >= 1")
    if font_subsets < 1:
        raise ValueError("font_subsets must be >= 1")

    obj_id = 1
    catalog_obj = obj_id
    obj_id += 1

    page_tree_objs = list(range(obj_id, obj_id + depth))
    obj_id += depth
    leaf_pages_obj = obj_id
    obj_id += 1

    base_font_obj = obj_id
    obj_id += 1

    subset_refs: list[tuple[int, int, int, int]] = []
    for _ in range(font_subsets):
        font_file_obj = obj_id
        descriptor_obj = obj_id + 1
        cid_font_obj = obj_id + 2
        type0_font_obj = obj_id + 3
        subset_refs.append((font_file_obj, descriptor_obj, cid_font_obj, type0_font_obj))
        obj_id += 4

    page_objs = list(range(obj_id, obj_id + pages))
    obj_id += pages

    content_objs: list[list[int]] = []
    for _ in range(pages):
        refs = list(range(obj_id, obj_id + streams_per_page))
        obj_id += streams_per_page
        content_objs.append(refs)

    padding_obj = obj_id
    content_total = max(pages * streams_per_page, target_bytes * 35 // 100)
    font_total = max(font_subsets, target_bytes * 45 // 100)
    content_len = max(1, content_total // (pages * streams_per_page))
    font_len = max(1, font_total // font_subsets)

    objects: list[PdfObject] = [
        raw(
            catalog_obj,
            f"<< /Type /Catalog /Pages {page_tree_objs[0]} 0 R /PieceInfo "
            f"<< /StressPadding << /Private {padding_obj} 0 R >> >> >>",
        )
    ]

    for index, tree_obj in enumerate(page_tree_objs):
        kid = page_tree_objs[index + 1] if index + 1 < len(page_tree_objs) else leaf_pages_obj
        parent = "" if index == 0 else f" /Parent {page_tree_objs[index - 1]} 0 R"
        objects.append(raw(tree_obj, f"<< /Type /Pages{parent} /Kids [{kid} 0 R] /Count {pages} >>"))

    kids = " ".join(f"{page_obj} 0 R" for page_obj in page_objs)
    objects.append(
        raw(
            leaf_pages_obj,
            f"<< /Type /Pages /Parent {page_tree_objs[-1]} 0 R /Kids [{kids}] /Count {pages} >>",
        )
    )
    objects.append(raw(base_font_obj, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"))

    for subset_index, (font_file_obj, descriptor_obj, cid_font_obj, type0_font_obj) in enumerate(subset_refs):
        subset_name = f"ST{subset_index:04d}+StressSubset{subset_index}"
        objects.append(
            StreamObject(
                font_file_obj,
                f"/Length1 {font_len} ",
                font_len,
                binary_stream_writer(f"font-subset-{subset_index}"),
            )
        )
        objects.append(
            raw(
                descriptor_obj,
                f"<< /Type /FontDescriptor /FontName /{subset_name} /Flags 4 "
                "/FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 800 "
                f"/Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {font_file_obj} 0 R >>",
            )
        )
        objects.append(
            raw(
                cid_font_obj,
                f"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{subset_name} "
                f"/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> "
                f"/FontDescriptor {descriptor_obj} 0 R /W [0 [500 500 500 500]] >>",
            )
        )
        objects.append(
            raw(
                type0_font_obj,
                f"<< /Type /Font /Subtype /Type0 /BaseFont /{subset_name} "
                f"/Encoding /Identity-H /DescendantFonts [{cid_font_obj} 0 R] >>",
            )
        )

    font_entries = [f"/F1 {base_font_obj} 0 R"]
    font_entries.extend(f"/FS{idx} {refs[3]} 0 R" for idx, refs in enumerate(subset_refs))
    resources = f"<< /Font << {' '.join(font_entries)} >> >>"

    for page_index, page_obj in enumerate(page_objs):
        contents = " ".join(f"{content_obj} 0 R" for content_obj in content_objs[page_index])
        objects.append(
            raw(
                page_obj,
                f"<< /Type /Page /Parent {leaf_pages_obj} 0 R /MediaBox [0 0 612 792] "
                f"/Resources {resources} /Contents [{contents}] >>",
            )
        )

    for page_index, refs in enumerate(content_objs):
        for stream_index, content_obj in enumerate(refs):
            objects.append(
                StreamObject(
                    content_obj,
                    "",
                    content_len,
                    content_stream_writer(page_index, stream_index),
                )
            )

    objects.append(StreamObject(padding_obj, "/Type /EmbeddedFile ", 0, binary_stream_writer("padding")))
    objects.sort(key=lambda obj: obj.obj_id)

    padding_len = max(0, target_bytes - compute_total_size(objects, catalog_obj))
    for _ in range(20):
        objects[-1] = StreamObject(
            padding_obj,
            "/Type /EmbeddedFile ",
            padding_len,
            binary_stream_writer("padding"),
        )
        total = compute_total_size(objects, catalog_obj)
        delta = target_bytes - total
        if delta == 0:
            return objects
        padding_len += delta
        if padding_len < 0:
            raise ValueError("target size is too small for the selected stress structure")

    total = compute_total_size(objects, catalog_obj)
    raise RuntimeError(f"could not converge on exact target size: target={target_bytes} actual={total}")


def compute_total_size(objects: list[PdfObject], catalog_obj: int) -> int:
    offset = len(pdf_header())
    max_obj = max(obj.obj_id for obj in objects)
    for obj in objects:
        offset += obj.size()
    return offset + len(xref_and_trailer(offset, max_obj, {}, catalog_obj))


def pdf_header() -> bytes:
    return b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"


def xref_and_trailer(xref_offset: int, max_obj: int, offsets: dict[int, int], catalog_obj: int) -> bytes:
    out = bytearray()
    out.extend(f"xref\n0 {max_obj + 1}\n".encode("ascii"))
    out.extend(b"0000000000 65535 f \n")
    for obj_id in range(1, max_obj + 1):
        out.extend(f"{offsets.get(obj_id, 0):010d} 00000 n \n".encode("ascii"))
    out.extend(
        (
            f"trailer\n<< /Size {max_obj + 1} /Root {catalog_obj} 0 R >>\n"
            f"startxref\n{xref_offset}\n%%EOF\n"
        ).encode("ascii")
    )
    return bytes(out)


def write_pdf(path: Path, objects: list[PdfObject], catalog_obj: int) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.parent.mkdir(parents=True, exist_ok=True)
    if tmp.exists():
        tmp.unlink()

    offsets: dict[int, int] = {}
    with tmp.open("wb") as out:
        out.write(pdf_header())
        for obj in objects:
            offsets[obj.obj_id] = out.tell()
            obj.write(out)
        xref_offset = out.tell()
        out.write(xref_and_trailer(xref_offset, max(obj.obj_id for obj in objects), offsets, catalog_obj))

    os.replace(tmp, path)


def validate_pdf(path: Path, expected_size: int) -> None:
    actual_size = path.stat().st_size
    if actual_size != expected_size:
        raise RuntimeError(f"{path} has {actual_size} bytes; expected {expected_size}")

    with path.open("rb") as pdf:
        header = pdf.read(8)
        if header != b"%PDF-1.7":
            raise RuntimeError(f"{path} does not start with a PDF-1.7 header")
        tail_size = min(actual_size, MIB)
        pdf.seek(actual_size - tail_size)
        tail = pdf.read()

    startxref_marker = tail.rfind(b"startxref\n")
    if startxref_marker < 0 or not tail.rstrip().endswith(b"%%EOF"):
        raise RuntimeError(f"{path} is missing startxref/EOF markers")
    start = startxref_marker + len(b"startxref\n")
    end = tail.find(b"\n", start)
    xref_offset = int(tail[start:end])
    with path.open("rb") as pdf:
        pdf.seek(xref_offset)
        if pdf.read(4) != b"xref":
            raise RuntimeError(f"{path} startxref does not point to an xref table")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as pdf:
        while True:
            chunk = pdf.read(WRITE_CHUNK)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def generate(size_name: str, target_bytes: int, args: argparse.Namespace) -> Path:
    output = args.output_dir / f"stress-{size_name}.pdf"
    if output.exists() and not args.force:
        validate_pdf(output, target_bytes)
        print(f"exists: {output} ({target_bytes / MIB:.1f} MiB, sha256={sha256_file(output)[:16]})")
        return output

    objects = make_plan(
        target_bytes,
        pages=args.pages,
        depth=args.depth,
        streams_per_page=args.content_streams_per_page,
        font_subsets=args.font_subsets,
    )
    write_pdf(output, objects, catalog_obj=1)
    validate_pdf(output, target_bytes)
    print(f"generated: {output} ({target_bytes / MIB:.1f} MiB, sha256={sha256_file(output)[:16]})")
    return output


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate ignored synthetic PDF stress fixtures for parser benchmarks.",
    )
    parser.add_argument(
        "--size",
        action="append",
        default=None,
        help="Fixture size to generate. Use 100M, 500M, 1G, all, or repeat the flag. Default: all.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help=f"Output directory. Default: {DEFAULT_OUTPUT_DIR}",
    )
    parser.add_argument("--force", action="store_true", help="Regenerate existing fixtures.")
    parser.add_argument("--pages", type=int, default=256, help="Number of synthetic pages. Default: 256.")
    parser.add_argument("--depth", type=int, default=32, help="Nested page-tree depth. Default: 32.")
    parser.add_argument(
        "--content-streams-per-page",
        type=int,
        default=4,
        help="Content streams per page. Default: 4.",
    )
    parser.add_argument("--font-subsets", type=int, default=8, help="Large embedded font subsets. Default: 8.")
    return parser.parse_args()


def requested_sizes(values: list[str] | None) -> list[tuple[str, int]]:
    if not values:
        values = list(DEFAULT_SIZES)

    sizes: list[tuple[str, int]] = []
    for value in values:
        if value.strip().lower() == "all":
            sizes.extend(parse_size(default_size) for default_size in DEFAULT_SIZES)
        else:
            sizes.append(parse_size(value))
    return sizes


def main() -> int:
    args = parse_args()
    try:
        for size_name, target_bytes in requested_sizes(args.size):
            generate(size_name, target_bytes, args)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
