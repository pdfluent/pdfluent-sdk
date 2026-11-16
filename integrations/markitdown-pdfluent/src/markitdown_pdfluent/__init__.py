"""MarkItDown plugin: PDFs to Markdown with the PDFluent engine."""

# Copyright (c) 2026 Innovation Trigger B.V.
#
# Licensed under the MIT License. See the LICENSE file in this repository.

from __future__ import annotations

from typing import Any

from markitdown import MarkItDown

from ._converter import PDFluentPdfConverter

__plugin_interface_version__ = 1

# Lower values run first. MarkItDown's built-in PDF converter uses 0, so this runs before it.
PRIORITY = -1.0

__all__ = ["PDFluentPdfConverter", "register_converters"]


def register_converters(markitdown: MarkItDown, **kwargs: Any) -> None:
    """Called by MarkItDown when plugins are enabled."""
    markitdown.register_converter(PDFluentPdfConverter(), priority=PRIORITY)
