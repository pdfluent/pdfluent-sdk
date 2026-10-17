"""An MCP server over PDFluent's structured extraction (#625).

Three tools, over the local PDF an agent names by path:

- ``extract_markdown``: the document as markdown in reading order -- headings,
  list items, tables -- which is what an agent reads or quotes.
- ``extract_blocks``: the same document as blocks, each with its kind, text,
  page and box, and ``confidence`` where a learned model decided the kind --
  what an agent cites, filters or routes.
- ``vision_advice``: which pages are worth sending to a vision model, and why,
  so an agent spends a vision call only where the text layer cannot answer.

It runs over stdio, one process per client, and reads files the agent's own
user can already read: there is no network listener and no upload.
"""

from __future__ import annotations

import functools
import os
from typing import Any, Callable, Dict, List, Optional, TypeVar

from mcp.server.mcpserver import MCPServer
from mcp.server.mcpserver.exceptions import ToolError

from pdfluent import Document, PdfluentError

T = TypeVar("T")

INSTRUCTIONS = (
    "Read local PDF files. Use extract_markdown to read or quote a document, "
    "extract_blocks when you need the page and box of what you cite or the kind "
    "of each block, and vision_advice before spending a vision call: it names the "
    "pages whose text layer is missing. A block's confidence is null where no "
    "model decided its kind; null means unknown, not low."
)

server = MCPServer(name="pdfluent", instructions=INSTRUCTIONS)


def _readable(tool: Callable[..., T]) -> Callable[..., T]:
    """A PDFluent error as a message the agent reads.

    The server hides the text of an exception it did not expect, which is right
    for a bug and wrong for "this file is not a PDF": the agent can act on that,
    so it is raised as a `ToolError`, whose message reaches the client.
    """

    @functools.wraps(tool)
    def wrapped(*args: Any, **kwargs: Any) -> T:
        try:
            return tool(*args, **kwargs)
        except PdfluentError as exc:
            raise ToolError(f"{type(exc).__name__}: {exc}") from exc

    return wrapped


def _open(path: str) -> Document:
    """The document at ``path``, with a message an agent can act on if not."""
    path = os.path.expanduser(path)
    if not os.path.isfile(path):
        raise ToolError(f"no file at {path!r}; pass the path of a local PDF")
    return Document(path)


def _block(block: Any) -> Dict[str, Any]:
    """One block as the fields it carries, and none it does not."""
    out: Dict[str, Any] = {
        "kind": block.kind,
        "text": block.text,
        "page": block.page,
        "bbox": list(block.bbox),
        # Null where no model spoke about this block: unknown, not low.
        "confidence": block.confidence,
    }
    for name in ("level", "ordinal", "row", "column", "row_span", "column_span", "header"):
        value = getattr(block, name)
        if value is not None:
            out[name] = value
    if block.continues_previous:
        out["continues_previous"] = True
    return out


def _advice(advice: Any) -> Dict[str, Any]:
    return {
        "page": advice.page,
        "needs_vision": advice.needs_vision,
        "no_text_layer": advice.no_text_layer,
        "low_confidence_table": advice.low_confidence_table,
        "figure": advice.figure,
    }


@server.tool()
@_readable
def extract_markdown(path: str, first_page: int = 0, page_count: Optional[int] = None) -> str:
    """A local PDF as markdown, in reading order.

    Headings become ``#`` to ``###``, list items ``-`` and ``1.``, a table a pipe
    table (or HTML where markdown cannot hold its shape), and a change of page an
    HTML comment. Running heads and page numbers are left out.

    Args:
        path: The PDF's path on this machine.
        first_page: The first page to return, 0-based.
        page_count: How many pages to return; omit for the rest of the document.
            The whole document is still read, because what a block is gets
            decided against the rest of it.
    """
    return _open(path).extract_markdown(first_page=first_page, page_count=page_count)


@server.tool()
@_readable
def extract_blocks(
    path: str,
    first_page: int = 0,
    page_count: Optional[int] = None,
    include_furniture: bool = False,
) -> Dict[str, Any]:
    """A local PDF as blocks, in reading order, with where each one came from.

    Each block has ``kind`` (paragraph, heading, list_item, table_cell, header,
    footer), ``text``, ``page`` (0-based), ``bbox`` ([x0, y0, x1, y1] in PDF
    points, origin bottom-left) and ``confidence``: the probability of the
    learned model that decided the block's kind, or null where no model did --
    null means unknown, not low. Headings carry ``level``, list items ``level``
    and ``ordinal``, table cells ``row``, ``column``, ``row_span``,
    ``column_span`` and ``header``.

    ``pages_needing_vision`` lists the pages in the window whose text layer is
    missing: nothing this returns for them is the page's content.

    Args:
        path: The PDF's path on this machine.
        first_page: The first page to return, 0-based.
        page_count: How many pages to return; omit for the rest of the document.
        include_furniture: Keep running heads and page numbers (kinds header and
            footer); they are left out by default because they repeat on every
            page and crowd out the content.
    """
    doc = _open(path)
    blocks = [
        _block(b)
        for b in doc.extract_blocks(first_page=first_page, page_count=page_count)
        if include_furniture or b.kind not in ("header", "footer")
    ]
    last = doc.page_count if page_count is None else first_page + page_count
    needing = [
        a.page for a in doc.vision_advice() if a.needs_vision and first_page <= a.page < last
    ]
    return {"blocks": blocks, "pages_needing_vision": needing}


@server.tool()
@_readable
def vision_advice(path: str) -> List[Dict[str, Any]]:
    """Which pages of a local PDF are worth sending to a vision model, and why.

    One entry per page. ``needs_vision`` is true when any reason holds. Today
    only ``no_text_layer`` is measured -- the page draws ink but yields no text,
    as a scan does; ``low_confidence_table`` and ``figure`` are always false
    until the engine can measure them.

    Args:
        path: The PDF's path on this machine.
    """
    return [_advice(a) for a in _open(path).vision_advice()]


def main() -> None:
    """Run the server over stdio, which is how Claude Code and Codex start it."""
    server.run("stdio")


if __name__ == "__main__":
    main()
