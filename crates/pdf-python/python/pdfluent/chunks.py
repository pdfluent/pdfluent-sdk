"""A document as retrieval chunks, each carrying where it came from (#630).

The LangChain loader (``langchain-pdfluent``) and the LlamaIndex reader
(``llama-index-readers-pdfluent``) both hand a pipeline this: text to embed, and
the metadata a retriever needs to cite it -- the page, the box on that page, the
kind of block and, where a learned model decided the kind, how sure it was. It
lives here, beside the binding, so the two integrations cannot drift apart on
what a chunk is, and so a pipeline on neither framework can use it as it is.

Two modes:

- ``"blocks"`` (the default): one chunk per paragraph, heading and list item
  of :meth:`Document.extract_blocks`, and ONE chunk per table rather than one
  per cell -- a cell on its own is a number without its column. Running heads
  and page numbers are left out unless ``include_furniture=True``, because a
  line every page repeats is the chunk that wins every similarity search.
- ``"markdown"``: one chunk per page, the page's
  :meth:`Document.extract_markdown`, with the per-page fields a vector-store
  pipeline ingests: ``toc_items`` and ``tables``.

The metadata of a chunk:

- ``source``: the path the document was opened from, when there was one;
- ``page``: the 0-based page index (the numbering of ``extract_blocks``);
- ``kind``: ``paragraph``, ``heading``, ``list_item``, ``table``, ``header``,
  ``footer`` -- or ``page`` in markdown mode;
- ``toc_items`` (markdown mode): the outline entries pointing at this page,
  as ``[level, title, page]`` triples -- levels from 1, pages 0-based like
  ``page`` itself;
- ``tables`` (markdown mode): one ``{"bbox", "rows", "columns"}`` per table
  on the page, grouped from the blocks the way ``"blocks"`` mode groups them.
  ``images`` and ``words`` are deliberately absent: the binding exposes
  neither the boxes behind the ``<!-- image -->`` markers nor word boxes, and
  an empty guess would be worse than no key;
- ``bbox``: ``[x0, y0, x1, y1]`` in points, y up (blocks mode only);
- ``confidence``: the learned model's probability for ``kind``, present only
  where a model decided it (absent is "not measured", not "low");
- ``section``: the text of the last heading above the chunk, when there is one;
- ``level`` on a heading, and ``continues_previous`` on the second half of a
  paragraph a page break cut in two.

With ``hierarchical=True`` every chunk also carries ``section_path``: the list
of enclosing headings from the heading blocks and their levels, for example
``["2 Methods", "2.1 Data"]`` (empty before the first heading), and a table
longer than ``max_chars`` is split by rows -- the header row repeated in each
piece, ``table_part`` naming the piece ``"i of n"``. A list item is always one
chunk and a table row is never broken across pieces, whatever the budget says.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any, Dict, Iterator, List, Optional, Tuple

from pdfluent._native import Document

__all__ = ["Chunk", "iter_chunks"]

MODES = ("blocks", "markdown")


@dataclass
class Chunk:
    """One piece of a document to embed, and where it came from."""

    text: str
    metadata: Dict[str, Any] = field(default_factory=dict)


def _union(a: List[float], b: List[float]) -> List[float]:
    return [min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3])]


def _table(cells: list) -> str:
    """A table's cells as rows of ``a | b | c``, one row per line."""
    rows: Dict[int, List[Any]] = {}
    for cell in cells:
        rows.setdefault(cell.row, []).append(cell)
    return "\n".join(
        " | ".join(c.text.strip() for c in sorted(rows[r], key=lambda c: c.column))
        for r in sorted(rows)
    )


def _enter_heading(stack: List[str], text: str, level: Optional[int]) -> List[str]:
    """The section stack below a heading: cut to its level, then pushed.

    A heading of unknown level is a top-level one: cutting nothing would nest
    the rest of the document under a guess.
    """
    depth = level or 1
    if depth < 1:
        depth = 1
    return stack[: depth - 1] + [text]


def _split_table(
    run: list, max_chars: int
) -> Optional[List[Tuple[str, list, str]]]:
    """A table run as ``(text, cells, table_part)`` pieces, or None to keep one.

    None means the table fits the budget, has no data rows to split by, or is
    a single row: splitting further would mean splitting inside a row, which
    is refused whatever the budget says.
    """
    rows: Dict[int, List[Any]] = {}
    for cell in run:
        rows.setdefault(cell.row, []).append(cell)
    ordered = [
        sorted(rows[r], key=lambda c: c.column) for r in sorted(rows)
    ]
    lines = [" | ".join(c.text.strip() for c in row) for row in ordered]
    if len("\n".join(lines)) <= max_chars:
        return None
    header, header_cells = lines[0], ordered[0]
    pieces: List[List[Tuple[str, list]]] = []
    current: List[Tuple[str, list]] = []
    size = len(header)
    for line, cells in zip(lines[1:], ordered[1:]):
        if current and size + 1 + len(line) > max_chars:
            pieces.append(current)
            current, size = [], len(header)
        current.append((line, cells))
        size += 1 + len(line)
    if current:
        pieces.append(current)
    if len(pieces) < 2:
        return None
    out: List[Tuple[str, list, str]] = []
    for at, piece in enumerate(pieces, 1):
        text = "\n".join([header] + [line for line, _ in piece])
        cells = list(header_cells)
        for _, row_cells in piece:
            cells.extend(row_cells)
        out.append((text, cells, f"{at} of {len(pieces)}"))
    return out


def _block_chunks(
    blocks: list,
    include_furniture: bool,
    hierarchical: bool = False,
    max_chars: int = 2000,
) -> Iterator[Chunk]:
    section: Optional[str] = None
    path: List[str] = []
    at = 0
    while at < len(blocks):
        block = blocks[at]
        if block.kind == "table_cell":
            # One table: a run of cells on one page whose (row, column) climbs,
            # which is how the engine's own serializers read a table.
            run = [block]
            at += 1
            while (
                at < len(blocks)
                and blocks[at].kind == "table_cell"
                and blocks[at].page == block.page
                and (blocks[at].row, blocks[at].column) > (run[-1].row, run[-1].column)
            ):
                run.append(blocks[at])
                at += 1
            split = (
                _split_table(run, max_chars) if hierarchical else None
            )
            if split is None:
                bbox = list(run[0].bbox)
                for cell in run[1:]:
                    bbox = _union(bbox, list(cell.bbox))
                metadata: Dict[str, Any] = {
                    "page": block.page,
                    "kind": "table",
                    "bbox": bbox,
                }
                if section is not None:
                    metadata["section"] = section
                if hierarchical:
                    metadata["section_path"] = list(path)
                yield Chunk(_table(run), metadata)
            else:
                for text, cells, part in split:
                    bbox = list(cells[0].bbox)
                    for cell in cells[1:]:
                        bbox = _union(bbox, list(cell.bbox))
                    metadata = {
                        "page": block.page,
                        "kind": "table",
                        "bbox": bbox,
                        "table_part": part,
                    }
                    if section is not None:
                        metadata["section"] = section
                    metadata["section_path"] = list(path)
                    yield Chunk(text, metadata)
            continue
        at += 1
        if block.kind in ("header", "footer") and not include_furniture:
            continue
        text = block.text.strip()
        if not text:
            continue
        ancestors: Optional[List[str]] = None
        if block.kind == "heading":
            section = text
            stack = _enter_heading(path, text, block.level)
            ancestors, path = stack[:-1], stack
        metadata = {
            "page": block.page,
            "kind": block.kind,
            "bbox": list(block.bbox),
        }
        if block.confidence is not None:
            metadata["confidence"] = block.confidence
        if block.kind == "heading" and block.level is not None:
            metadata["level"] = block.level
        if block.continues_previous:
            metadata["continues_previous"] = True
        if section is not None and block.kind != "heading":
            metadata["section"] = section
        if hierarchical:
            metadata["section_path"] = (
                list(ancestors) if ancestors is not None else list(path)
            )
        yield Chunk(text, metadata)


def _page_paths(blocks: list) -> Dict[int, List[str]]:
    """The section stack at the first block of each page."""
    paths: Dict[int, List[str]] = {}
    stack: List[str] = []
    for block in blocks:
        if block.page not in paths:
            paths[block.page] = list(stack)
        if block.kind == "heading":
            text = block.text.strip()
            if text:
                stack = _enter_heading(stack, text, block.level)
    return paths


def _flatten_toc(bookmarks: list, level: int = 1) -> List[List[Any]]:
    """The outline as ``[level, title, page]`` triples, in document order.

    Levels count from 1; pages stay 0-based, this binding's. An entry without
    a page -- a named destination the engine does
    not resolve yet -- points nowhere and is left out, but its children still
    count: the level is the depth in the tree, not in what survived.
    """
    flat: List[List[Any]] = []
    for bookmark in bookmarks:
        if bookmark.page is not None:
            flat.append([level, bookmark.title, bookmark.page])
        flat.extend(_flatten_toc(bookmark.children, level + 1))
    return flat


def _page_tables(blocks: list) -> Dict[int, List[Dict[str, Any]]]:
    """One ``{"bbox", "rows", "columns"}`` per table on each page.

    A table is the same run ``_block_chunks`` reads as one chunk: cells on
    one page whose ``(row, column)`` climbs. ``rows`` and ``columns`` are the
    grid's extent, so a 4-row by 3-column table reads 4 and 3.
    """
    tables: Dict[int, List[Dict[str, Any]]] = {}
    at = 0
    while at < len(blocks):
        block = blocks[at]
        if block.kind != "table_cell":
            at += 1
            continue
        run = [block]
        at += 1
        while (
            at < len(blocks)
            and blocks[at].kind == "table_cell"
            and blocks[at].page == block.page
            and (blocks[at].row, blocks[at].column) > (run[-1].row, run[-1].column)
        ):
            run.append(blocks[at])
            at += 1
        bbox = list(run[0].bbox)
        for cell in run[1:]:
            bbox = _union(bbox, list(cell.bbox))
        tables.setdefault(block.page, []).append(
            {
                "bbox": bbox,
                "rows": max(cell.row for cell in run) + 1,
                "columns": max(cell.column for cell in run) + 1,
            }
        )
    return tables


def iter_chunks(
    document: Any,
    mode: str = "blocks",
    include_furniture: bool = False,
    first_page: int = 0,
    page_count: Optional[int] = None,
    hierarchical: bool = False,
    max_chars: int = 2000,
) -> Iterator[Chunk]:
    """The document as chunks, in reading order.

    ``document`` is an open :class:`Document`, or a path or ``bytes`` to open.
    ``first_page`` and ``page_count`` choose a window of pages, as they do on
    :meth:`Document.extract_blocks`.

    ``hierarchical`` adds ``section_path`` -- the enclosing headings -- to
    every chunk and splits tables longer than ``max_chars`` by rows, repeating
    the header row in each piece. It leaves the default output untouched: with
    ``hierarchical=False`` a list item, a paragraph and a table are one chunk
    each however long they are.
    """
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}, not {mode!r}")
    if max_chars < 1:
        raise ValueError(f"max_chars must be at least 1, not {max_chars!r}")
    source: Optional[str] = None
    if isinstance(document, Document):
        opened = document
    else:
        if isinstance(document, os.PathLike):
            document = os.fspath(document)
        if isinstance(document, str):
            source = document
        opened = Document(document)
    if mode == "markdown":
        last = opened.page_count if page_count is None else first_page + page_count
        window = opened.extract_blocks(first_page=first_page, page_count=page_count)
        paths = _page_paths(window) if hierarchical else {}
        per_page = _page_tables(window)
        toc = _flatten_toc(opened.bookmarks)
        current: List[str] = []
        for page in range(first_page, min(last, opened.page_count)):
            if page in paths:
                current = paths[page]
            text = opened.extract_markdown(first_page=page, page_count=1).strip()
            if not text:
                continue
            metadata: Dict[str, Any] = {} if source is None else {"source": source}
            metadata.update(
                {
                    "page": page,
                    "kind": "page",
                    "toc_items": [entry for entry in toc if entry[2] == page],
                    "tables": per_page.get(page, []),
                }
            )
            if hierarchical:
                metadata["section_path"] = list(current)
            yield Chunk(text, metadata)
        return
    blocks = opened.extract_blocks(first_page=first_page, page_count=page_count)
    for chunk in _block_chunks(
        blocks, include_furniture, hierarchical, max_chars
    ):
        if source is not None:
            chunk.metadata = {"source": source, **chunk.metadata}
        yield chunk
