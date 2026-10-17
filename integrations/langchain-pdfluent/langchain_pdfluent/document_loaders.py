"""A LangChain document loader over PDFluent's structured extraction (#630).

One LangChain ``Document`` per chunk of :func:`pdfluent.chunks.iter_chunks`:
a paragraph, a heading, a list item or a whole table, in reading order, each
with ``source``, ``page`` (0-based), ``kind``, ``bbox`` and -- where a learned
model decided the kind -- ``confidence`` in its metadata, so a retriever can
cite the page and a viewer can highlight the box. ``mode="markdown"`` gives one
``Document`` per page instead.

``bbox`` is a list. A vector store that only takes scalar metadata (Chroma, for
one) wants ``langchain_community.vectorstores.utils.filter_complex_metadata``
over the documents first.
"""

from __future__ import annotations

from pathlib import Path
from typing import Iterator, Optional, Union

from langchain_core.document_loaders import BaseLoader
from langchain_core.documents import Document
from pdfluent.chunks import MODES, iter_chunks


class PDFluentLoader(BaseLoader):
    """Load a PDF as structure-aware chunks with PDFluent.

    Example:
        .. code-block:: python

            from langchain_pdfluent import PDFluentLoader

            docs = PDFluentLoader("report.pdf").load()
            docs[0].metadata  # {'source': 'report.pdf', 'page': 0, 'kind': 'heading', ...}
    """

    def __init__(
        self,
        file_path: Union[str, Path],
        *,
        mode: str = "blocks",
        include_furniture: bool = False,
        first_page: int = 0,
        page_count: Optional[int] = None,
    ) -> None:
        """Initialize the loader.

        Args:
            file_path: The PDF to load.
            mode: ``"blocks"`` for one document per block (a table is one
                block), or ``"markdown"`` for one document per page.
            include_furniture: Keep the running heads and page numbers the
                document repeats on every page; left out by default.
            first_page: First page to load, counted from zero.
            page_count: How many pages from ``first_page``; ``None`` for the
                rest of the document.
        """
        if mode not in MODES:
            raise ValueError(f"mode must be one of {MODES}, not {mode!r}")
        self.file_path = str(file_path)
        self.mode = mode
        self.include_furniture = include_furniture
        self.first_page = first_page
        self.page_count = page_count

    def lazy_load(self) -> Iterator[Document]:
        """Yield one ``Document`` per chunk, in reading order."""
        for chunk in iter_chunks(
            self.file_path,
            mode=self.mode,
            include_furniture=self.include_furniture,
            first_page=self.first_page,
            page_count=self.page_count,
        ):
            yield Document(page_content=chunk.text, metadata=chunk.metadata)
