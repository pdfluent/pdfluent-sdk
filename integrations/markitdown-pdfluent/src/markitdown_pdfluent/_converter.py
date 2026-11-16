# Copyright (c) 2026 Innovation Trigger B.V.
#
# Licensed under the MIT License. See the LICENSE file in this repository.

from __future__ import annotations

from typing import Any, BinaryIO

from markitdown import DocumentConverter, DocumentConverterResult, StreamInfo

_PDF_EXTENSIONS = (".pdf",)
_PDF_MIMETYPES = ("application/pdf", "application/x-pdf")
_PDF_MAGIC = b"%PDF-"


class PDFluentPdfConverter(DocumentConverter):
    """Converts PDF streams to Markdown with the local PDFluent engine."""

    def accepts(self, file_stream: BinaryIO, stream_info: StreamInfo, **kwargs: Any) -> bool:
        extension = (stream_info.extension or "").lower()
        mimetype = (stream_info.mimetype or "").lower()
        if extension in _PDF_EXTENSIONS or mimetype.startswith(_PDF_MIMETYPES):
            return True
        # No hint from the caller: look at the first bytes, then put the stream back.
        if not extension and not mimetype and file_stream.seekable():
            position = file_stream.tell()
            try:
                return file_stream.read(len(_PDF_MAGIC)) == _PDF_MAGIC
            finally:
                file_stream.seek(position)
        return False

    def convert(
        self, file_stream: BinaryIO, stream_info: StreamInfo, **kwargs: Any
    ) -> DocumentConverterResult:
        from pdfluent import Document

        document = Document(file_stream.read())
        with document:
            markdown = document.extract_markdown()
        return DocumentConverterResult(markdown=markdown.strip() + "\n")
