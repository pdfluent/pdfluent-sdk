# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The `pdfluent-extract` console script: files to Markdown, JSON and chunks.

`pdfluent-extract <input> [-o OUT] [--format md|json|chunks|text]
[--quality fast|balanced|best] [--pages SPEC]`

`-` as input reads stdin; `-o -` or no `-o` writes stdout. `--pages` counts
from 1 (`1-3,5`), like the main CLI. Exit 2 on a usage error, 1 on an
extraction error, with a message on stderr either way.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict, is_dataclass
from typing import List, Optional, Sequence

from pdfluent_extract import to_chunks, to_json, to_markdown

FORMATS = ("md", "json", "chunks", "text")
QUALITIES = ("fast", "balanced", "best")


def parse_pages(spec: str) -> List[int]:
    """`1-3,5` into sorted, deduplicated 0-based page indices.

    Users count from 1; the engine counts from 0. A bad spec raises
    ValueError naming it — the caller reports that as a usage error.
    """
    items = spec.split(",")
    pages: List[int] = []
    for raw in items:
        item = raw.strip()
        if not item:
            raise ValueError(f"empty item in page spec {spec!r} (want e.g. `1-3,5`)")
        if "-" in item:
            lo_text, _, hi_text = item.partition("-")
            try:
                lo, hi = int(lo_text), int(hi_text)
            except ValueError:
                raise ValueError(f"not a page range: {item!r} (in {spec!r})")
            if lo < 1 or hi < 1:
                raise ValueError(f"pages count from 1, so `0` names nothing (in {spec!r})")
            if lo > hi:
                raise ValueError(f"reversed range {item!r} (in {spec!r}; want `lo-hi`)")
            pages.extend(range(lo - 1, hi))
        else:
            try:
                page = int(item)
            except ValueError:
                raise ValueError(f"not a page number: {item!r} (in {spec!r})")
            if page < 1:
                raise ValueError(f"pages count from 1, so `0` names nothing (in {spec!r})")
            pages.append(page - 1)
    return sorted(set(pages))


def _read_input(path: str):
    """The input as the library wants it: a path stays a path (so chunks
    cite it and JSON names it), `-` reads stdin as bytes."""
    if path == "-":
        data = sys.stdin.buffer.read()
        if not data:
            raise ValueError("nothing on stdin")
        return data
    with open(path, "rb"):
        pass
    return path


def _render(
    source,
    *,
    format: str,
    quality: str,
    pages: Optional[Sequence[int]],
) -> str:
    if format == "md":
        return to_markdown(source, quality=quality, pages=pages)
    if format == "json":
        return json.dumps(to_json(source, quality=quality, pages=pages), indent=2)
    chunks = to_chunks(source, quality=quality, pages=pages)
    if format == "text":
        return "\n\n".join(chunk.text for chunk in chunks)
    return json.dumps(
        [
            asdict(chunk) if is_dataclass(chunk) else chunk
            for chunk in chunks
        ],
        indent=2,
    )


def _write_output(text: str, out: Optional[str]) -> None:
    if not text.endswith("\n"):
        text += "\n"
    if out is None or out == "-":
        sys.stdout.write(text)
        return
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(text)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="pdfluent-extract",
        description="PDF and Office files to Markdown, JSON and RAG chunks.",
    )
    parser.add_argument("input", help="input file, or - for stdin")
    parser.add_argument(
        "-o", "--output", default=None,
        help="write here instead of stdout (- for stdout)",
    )
    parser.add_argument(
        "--format", choices=FORMATS, default="md",
        help="output shape (default: md)",
    )
    parser.add_argument(
        "--quality", choices=QUALITIES, default="fast",
        help="extraction quality (default: fast)",
    )
    parser.add_argument(
        "--pages", default=None, metavar="SPEC",
        help="pages to extract, counting from 1 (e.g. 1-3,5)",
    )
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        pages = parse_pages(args.pages) if args.pages is not None else None
    except ValueError as e:
        print(f"pdfluent-extract: {e}", file=sys.stderr)
        return 2
    try:
        source = _read_input(args.input)
    except FileNotFoundError:
        print(f"pdfluent-extract: no such file: {args.input}", file=sys.stderr)
        return 1
    except OSError as e:
        print(f"pdfluent-extract: cannot read {args.input}: {e}", file=sys.stderr)
        return 1
    except ValueError as e:
        print(f"pdfluent-extract: {e}", file=sys.stderr)
        return 1
    try:
        text = _render(source, format=args.format, quality=args.quality, pages=pages)
    except Exception as e:  # the engine refusing input it cannot read
        print(f"pdfluent-extract: extraction failed: {e}", file=sys.stderr)
        return 1
    try:
        _write_output(text, args.output)
    except OSError as e:
        print(f"pdfluent-extract: cannot write {args.output}: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
