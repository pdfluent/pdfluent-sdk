# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""PDF and Office files to Markdown, JSON and RAG chunks, in three calls.

This package is pure Python: the engine underneath is `pdfluent`, which pip
pulls in automatically (1.1.0 or later, on every platform the matrix covers).
There is nothing to compile and no system dependency to install.

>>> from pdfluent_extract import to_markdown
>>> print(to_markdown("report.pdf"))
"""

from __future__ import annotations

import json
import os
from typing import Any, BinaryIO, Dict, List, Optional, Sequence, Tuple, Union

import pdfluent
from pdfluent.chunks import Chunk, iter_chunks

__all__ = ["Chunk", "to_chunks", "to_json", "to_markdown"]

#: The only quality level v1 accepts. Anything else is refused by name —
#: there is no hidden fallback to a slower or a weaker pipeline.
QUALITIES = ("fast",)

#: Tokens to characters for the chunk budget: ~4 characters per token for
#: English prose. A heuristic, stated here rather than smuggled into a default.
CHARS_PER_TOKEN = 4

Source = Union[str, bytes, "os.PathLike[str]", BinaryIO]


def _check_quality(quality: str) -> None:
    if quality not in QUALITIES:
        allowed = ", ".join(repr(q) for q in QUALITIES)
        raise ValueError(f"quality must be one of {allowed}, not {quality!r}")


def _read_source(source: Source) -> Tuple[bytes, Optional[str], Optional[str]]:
    """The source as ``(data, path, name)``.

    ``path`` is set for path inputs (so chunks can cite it); ``name`` is the
    file stem for paths, ``None`` otherwise. File-like objects are read fully;
    the stream position is left wherever the read ends.
    """
    if isinstance(source, os.PathLike):
        source = os.fspath(source)
    if isinstance(source, str):
        with open(source, "rb") as fh:
            data = fh.read()
        stem = os.path.splitext(os.path.basename(source))[0]
        return data, source, stem or "document"
    if isinstance(source, (bytes, bytearray)):
        return bytes(source), None, None
    read = getattr(source, "read", None)
    if callable(read):
        data = read()
        if isinstance(data, bytearray):
            data = bytes(data)
        if not isinstance(data, bytes):
            raise TypeError(
                "file-like sources must read bytes, "
                f"not {type(data).__name__}"
            )
        return data, None, None
    raise TypeError(
        "source must be a path, bytes or a file-like object, "
        f"not {type(source).__name__}"
    )


def _kind(data: bytes) -> str:
    """``"pdf"``, ``"office"`` or ``"unknown"``, by magic bytes.

    The extension is never consulted: a workbook saved as `.pdf` extracts as
    a workbook, and a PDF saved as `.bin` extracts as a PDF. The PDF header
    is searched in the first 1024 bytes rather than prefix-matched — the PDF
    spec tolerates leading garbage there. The ZIP match stays a prefix match:
    Office files open with the local file header, and a PDF must win over a
    ZIP that merely contains the header bytes further in.
    """
    if b"%PDF-" in data[:1024]:
        return "pdf"
    if data.startswith(b"PK"):
        return "office"
    return "unknown"


def _require_office() -> Any:
    extract_office = getattr(pdfluent, "extract_office", None)
    if extract_office is None:
        raise NotImplementedError(
            "Office files need pdfluent>=1.1.0, which exposes "
            "pdfluent.extract_office; the installed pdfluent "
            f"{getattr(pdfluent, '__version__', 'unknown')} does not have it"
        )
    return extract_office


def _check_pages_for(pages: Optional[Sequence[int]], kind: str) -> None:
    if pages is not None and kind != "pdf":
        raise ValueError("pages selects PDF pages; Office files have no pages to select")


def to_markdown(
    source: Source,
    *,
    quality: str = "fast",
    pages: Optional[Sequence[int]] = None,
) -> str:
    """The file as Markdown, in reading order.

    ``source`` is a path, ``bytes`` or a file-like object; PDF and Office
    (``.docx``/``.pptx``/``.xlsx``) are routed by magic bytes. ``pages`` is a
    list of 0-based PDF page indices (``None`` for the whole file).
    """
    _check_quality(quality)
    data, _, _ = _read_source(source)
    kind = _kind(data)
    _check_pages_for(pages, kind)
    if kind == "pdf":
        with pdfluent.Document(data) as doc:
            return doc.extract_markdown(
                pages=list(pages) if pages is not None else None
            )
    if kind == "office":
        return _require_office()(data, format="markdown")
    raise pdfluent.PdfluentParseError(
        "not a PDF or Office file: the bytes start with neither %PDF- nor a ZIP header"
    )


def to_json(
    source: Source,
    *,
    quality: str = "fast",
    pages: Optional[Sequence[int]] = None,
) -> Dict[str, Any]:
    """The file as a Docling ``DoclingDocument``, as a dict.

    Same routing as :func:`to_markdown`; the JSON is what
    ``DoclingDocument.model_validate_json`` reads. ``name`` is the file's stem
    for path inputs, ``"document"`` otherwise.
    """
    _check_quality(quality)
    data, _, name = _read_source(source)
    kind = _kind(data)
    _check_pages_for(pages, kind)
    if kind == "pdf":
        with pdfluent.Document(data) as doc:
            return json.loads(
                doc.extract_docling(
                    name=name or "document",
                    pages=list(pages) if pages is not None else None,
                )
            )
    if kind == "office":
        return json.loads(
            _require_office()(data, format="docling", name=name or "document")
        )
    raise pdfluent.PdfluentParseError(
        "not a PDF or Office file: the bytes start with neither %PDF- nor a ZIP header"
    )


def to_chunks(
    source: Source,
    *,
    quality: str = "fast",
    max_tokens: int = 512,
    pages: Optional[Sequence[int]] = None,
) -> List[Chunk]:
    """The file as RAG chunks, in reading order.

    One chunk per paragraph, heading and list item, and one per table (a cell
    on its own is a number without its column). Every chunk carries ``page``,
    ``section_path`` (the enclosing headings) and ``bbox`` in its metadata,
    plus ``source`` for path inputs.

    ``max_tokens`` budgets each chunk (about four characters to the token);
    tables longer than the budget split by rows with the header repeated.
    ``pages`` lists 0-based page indices; chunks come back per page in the
    listed order. PDF only in v1.
    """
    _check_quality(quality)
    if not isinstance(max_tokens, int) or max_tokens < 1:
        raise ValueError(f"max_tokens must be a positive int, not {max_tokens!r}")
    data, path, _ = _read_source(source)
    kind = _kind(data)
    if kind != "pdf":
        if kind == "office":
            raise NotImplementedError(
                "to_chunks reads PDF files in v1; Office chunking is not scheduled yet"
            )
        raise pdfluent.PdfluentParseError(
            "not a PDF or Office file: the bytes start with neither %PDF- nor a ZIP header"
        )
    max_chars = max_tokens * CHARS_PER_TOKEN
    chunks: List[Chunk] = []
    with pdfluent.Document(data) as doc:
        if pages is None:
            chunks = list(iter_chunks(doc, hierarchical=True, max_chars=max_chars))
        else:
            for page in pages:
                chunks.extend(
                    iter_chunks(
                        doc,
                        hierarchical=True,
                        max_chars=max_chars,
                        first_page=page,
                        page_count=1,
                    )
                )
    if path is not None:
        for chunk in chunks:
            chunk.metadata = {"source": path, **chunk.metadata}
    return chunks
