"""A Haystack converter over PDFluent: PDFs to structure-aware ``Document`` chunks.

Local mode reads the PDF with the ``pdfluent`` engine on this machine. API mode (an ``api_key`` or the
``PDFLUENT_API_KEY`` environment variable) sends the PDF to the hosted PDFluent API for that call and
gets the chunks back; the service keeps nothing. One ``Document`` per chunk, in reading order, with
``source``, ``page`` (counted from 0), ``kind`` and ``bbox`` in its ``meta``.
"""

# Copyright (c) 2026 Innovation Trigger B.V.
#
# Licensed under the MIT License. See the LICENSE file in this repository.

from __future__ import annotations

import os
import tempfile
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Union

from haystack import Document, component, default_from_dict, default_to_dict
from haystack.dataclasses import ByteStream

from . import _api


@component
class PDFluentConverter:
    """Convert PDFs to chunked ``Document`` objects with PDFluent.

    Usage:

    .. code-block:: python

        from pdfluent_haystack import PDFluentConverter

        converter = PDFluentConverter()                     # local engine
        converter = PDFluentConverter(api_key="...")        # hosted API
        docs = converter.run(sources=["report.pdf"])["documents"]
    """

    def __init__(
        self,
        *,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        quality: str = "fast",
        pages: Optional[Sequence[int]] = None,
        chunking_strategy: str = "by_title",
        max_characters: int = 1000,
        overlap: int = 100,
        new_after_n_chars: Optional[int] = None,
        combine_text_under_n_chars: int = 0,
        max_chars: int = 2000,
        embed: bool = False,
        embedding_model: Optional[str] = None,
        embedding_region: str = "eu",
        byok_embedding_key: Optional[str] = None,
        byok_embedding_base_url: Optional[str] = None,
        flatten_metadata: bool = False,
    ) -> None:
        """Set up the converter.

        Args:
            api_key: Use the hosted PDFluent API (API mode). ``PDFLUENT_API_KEY`` does the same.
            api_url: Another base URL for the hosted API (API mode only).
            quality: ``"fast"`` (default) or ``"balanced"`` (API mode and the local engine).
            pages: 0-based pages to convert; ``None`` for all.
            chunking_strategy: ``"by_title"`` (default), ``"basic"``, ``"by_page"`` or
                ``"structure"`` (API mode only).
            max_characters: Largest chunk in characters (API mode only).
            overlap: Characters shared by pieces of a long element (API mode only).
            new_after_n_chars: Close a chunk at this size (API mode only).
            combine_text_under_n_chars: Merge short sections (API mode only).
            max_chars: Size budget for a split table (local mode only).
            embed: API mode only. Also embed every chunk through the hosted API (OpenRouter) and put
                the vector on the document. The call goes to the EU region unless
                ``embedding_region="global"``, which sends the text outside the EU and says so in the
                metadata (``data_left_eu``).
            embedding_model: The model for ``embed=True``. Default: the service's default
                (``mistral-embed``). With your own key it is required and passed on as you wrote it.
            embedding_region: ``"eu"`` (default) or ``"global"``.
            byok_embedding_key: Your own embedding provider key (BYOK), for this embeddings call
                only. Or set ``PDFLUENT_BYOK_EMBEDDING_KEY``. It is sent in a request header, used
                for that call by the service, and never stored, logged or written to disk. With it
                you pay the processing credits only.
            byok_embedding_base_url: The OpenAI-compatible base URL of your provider (public https).
            flatten_metadata: Make every metadata value a scalar, for vector stores that reject lists
                (Chroma, Pinecone, Milvus): ``bbox`` becomes ``bbox_x0`` to ``bbox_y1``, ``section_path``
                one string, other lists and dicts JSON text. See ``_api.flatten_metadata``.
        """
        self._api_key = _api.resolve_key(api_key)
        self.api_url = api_url
        self.quality = quality
        self.pages = None if pages is None else list(pages)
        self.chunking_strategy = chunking_strategy
        self.max_characters = max_characters
        self.overlap = overlap
        self.new_after_n_chars = new_after_n_chars
        self.combine_text_under_n_chars = combine_text_under_n_chars
        self.max_chars = max_chars
        self.embed = embed
        self.embedding_model = embedding_model
        self.embedding_region = embedding_region
        self.flatten_metadata = flatten_metadata
        # The key is held here for the embeddings call and shows up in no repr.
        if (byok_embedding_key or byok_embedding_base_url) and not embed:
            raise ValueError("a BYOK embedding key is only used with embed=True")
        self._byok = _api.Byok.resolve(byok_embedding_key, byok_embedding_base_url) if embed else None
        # A saved pipeline holds no key, so it can load without one; running it then says what is missing.

    def _meta(self, meta: Dict[str, Any]) -> Dict[str, Any]:
        return _api.flatten_metadata(meta) if self.flatten_metadata else meta

    def to_dict(self) -> Dict[str, Any]:
        """The pipeline form: settings only. Neither the PDFluent key nor a BYOK key is written into
        a saved pipeline; both come from the environment (or the constructor) when it is loaded."""
        return default_to_dict(
            self,
            api_url=self.api_url,
            quality=self.quality,
            pages=self.pages,
            chunking_strategy=self.chunking_strategy,
            max_characters=self.max_characters,
            overlap=self.overlap,
            new_after_n_chars=self.new_after_n_chars,
            combine_text_under_n_chars=self.combine_text_under_n_chars,
            max_chars=self.max_chars,
            embed=self.embed,
            embedding_model=self.embedding_model,
            embedding_region=self.embedding_region,
            flatten_metadata=self.flatten_metadata,
        )

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "PDFluentConverter":
        return default_from_dict(cls, data)

    def __repr__(self) -> str:  # the key stays out of logs and tracebacks
        return f"PDFluentConverter(api={self._api_key is not None})"

    @component.output_types(documents=List[Document])
    def run(
        self,
        sources: List[Union[str, Path, ByteStream]],
        meta: Optional[Union[Dict[str, Any], List[Dict[str, Any]]]] = None,
    ) -> Dict[str, List[Document]]:
        """Convert ``sources`` (paths or ``ByteStream`` objects) to ``Document`` chunks.

        ``meta`` is one dict for every source or one dict per source; it is merged into the
        ``meta`` of that source's documents.
        """
        if self.embed and self._api_key is None:
            raise ValueError("embed=True needs API mode: pass api_key or set PDFLUENT_API_KEY")
        if isinstance(meta, list) and len(meta) != len(sources):
            raise ValueError("meta as a list needs one entry per source")
        documents: List[Document] = []
        for index, source in enumerate(sources):
            extra = (meta[index] if isinstance(meta, list) else meta) or {}
            documents.extend(self._convert(source, extra))
        return {"documents": documents}

    def _convert(self, source: Union[str, Path, ByteStream], extra: Dict[str, Any]) -> List[Document]:
        if isinstance(source, ByteStream):
            name = str(source.meta.get("file_path", "stream"))
            data = source.data
        else:
            name = str(source)
            with open(source, "rb") as handle:
                data = handle.read()
        if self._api_key is not None:
            options = _api.api_options(
                pages=self.pages, quality=self.quality, chunking_strategy=self.chunking_strategy,
                max_characters=self.max_characters, overlap=self.overlap,
                new_after_n_chars=self.new_after_n_chars,
                combine_text_under_n_chars=self.combine_text_under_n_chars,
                embedding_model=self.embedding_model if self.embed else None,
                embedding_region=self.embedding_region if self.embed else None,
            )
            if self.embed:
                answer = _api.fetch_embeddings_bytes(
                    data, api_key=self._api_key, api_url=self.api_url, options=options, byok=self._byok
                )
                extra_meta = _api.embedding_metadata(answer)
                return [
                    Document(
                        content=c["text"],
                        embedding=c["embedding"],
                        meta=self._meta({**_api.chunk_metadata(name, c), **extra_meta, **extra}),
                    )
                    for c in answer["chunks"]
                ]
            chunks = _api.fetch_chunks_bytes(data, api_key=self._api_key, api_url=self.api_url, options=options)
            return [
                Document(content=c["text"], meta=self._meta({**_api.chunk_metadata(name, c), **extra})) for c in chunks
            ]
        return self._local(name, data, extra)

    def _local(self, name: str, data: bytes, extra: Dict[str, Any]) -> List[Document]:
        from pdfluent.chunks import iter_chunks  # imported here: API mode needs no engine

        # The engine reads files, so a stream is written to a temporary file for the call.
        handle, path = tempfile.mkstemp(suffix=".pdf")
        try:
            with os.fdopen(handle, "wb") as out:
                out.write(data)
            return [
                Document(content=chunk.text, meta=self._meta({**chunk.metadata, "source": name, **extra}))
                for chunk in iter_chunks(
                    path, mode="blocks", quality=self.quality, pages=self.pages, max_chars=self.max_chars
                )
            ]
        finally:
            os.unlink(path)
