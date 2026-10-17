"""A LlamaIndex reader over PDFluent's structured extraction (#630).

One LlamaIndex ``Document`` per chunk of :func:`pdfluent.chunks.iter_chunks`:
a paragraph, a heading, a list item or a whole table, in reading order, each
with ``source``, ``page`` (0-based), ``kind``, ``bbox`` and -- where a learned
model decided the kind -- ``confidence`` in its metadata. ``mode="markdown"``
gives one ``Document`` per page instead.

The geometry and the confidence are for citing and highlighting, not for
reading, so they are excluded from what the embedding model and the LLM see;
``source``, ``page``, ``kind`` and ``section`` stay in.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Union

from llama_index.core.readers.base import BaseReader
from llama_index.core.schema import Document
from pdfluent.chunks import MODES, iter_chunks

# Metadata a retriever cites with and a model has no use reading.
HIDDEN_FROM_MODELS = ["bbox", "confidence", "level", "continues_previous"]


class PDFluentReader(BaseReader):
    """Read a PDF as structure-aware documents with PDFluent.

    Works on its own and as a ``file_extractor`` of ``SimpleDirectoryReader``.

    Example:
        .. code-block:: python

            from llama_index.readers.pdfluent import PDFluentReader

            documents = PDFluentReader().load_data("report.pdf")
    """

    def __init__(
        self,
        *,
        mode: str = "blocks",
        include_furniture: bool = False,
        first_page: int = 0,
        page_count: Optional[int] = None,
    ) -> None:
        """Initialize the reader.

        Args:
            mode: ``"blocks"`` for one document per block (a table is one
                block), or ``"markdown"`` for one document per page.
            include_furniture: Keep the running heads and page numbers the
                document repeats on every page; left out by default.
            first_page: First page to read, counted from zero.
            page_count: How many pages from ``first_page``; ``None`` for the
                rest of the document.
        """
        if mode not in MODES:
            raise ValueError(f"mode must be one of {MODES}, not {mode!r}")
        self.mode = mode
        self.include_furniture = include_furniture
        self.first_page = first_page
        self.page_count = page_count

    def lazy_load_data(
        self,
        file: Union[str, Path],
        extra_info: Optional[Dict[str, Any]] = None,
        **_: Any,
    ) -> Iterable[Document]:
        """Yield one ``Document`` per chunk of ``file``, in reading order.

        ``extra_info`` is merged into every document's metadata, which is how
        ``SimpleDirectoryReader`` passes its file metadata.
        """
        for chunk in iter_chunks(
            str(file),
            mode=self.mode,
            include_furniture=self.include_furniture,
            first_page=self.first_page,
            page_count=self.page_count,
        ):
            metadata = {**(extra_info or {}), **chunk.metadata}
            yield Document(
                text=chunk.text,
                metadata=metadata,
                excluded_embed_metadata_keys=list(HIDDEN_FROM_MODELS),
                excluded_llm_metadata_keys=list(HIDDEN_FROM_MODELS),
            )

    def load_data(
        self,
        file: Union[str, Path],
        extra_info: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> List[Document]:
        """Every ``Document`` of ``file``, in reading order."""
        return list(self.lazy_load_data(file, extra_info, **kwargs))
