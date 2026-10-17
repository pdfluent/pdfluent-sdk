# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Turn documents into markdown, text and JSON for language-model pipelines.

Two converters sit behind one surface: the layout engine, which parses
each page into classified boxes before rendering, and the legacy engine,
which converts page text directly. :func:`use_layout` picks between
them; a fresh process starts in layout mode.
"""

from . import batch_converter, helpers, ocr, worker_sizing, versions_file
from ._chunks import get_key_values, to_json
from ._errors import EmptyFileError, FileDataError
from ._inputs import convert_batch
from ._markdown import to_markdown
from ._mode import set_layout_mode
from ._tables import to_text
from .versions_file import VERSION, VERSION_TUPLE

version = VERSION
version_tuple = VERSION_TUPLE


def use_layout(yes):
    """Select the layout engine for truthy values, legacy otherwise."""
    set_layout_mode(yes)


def LlamaMarkdownReader(*args, **kwargs):
    """Placeholder reader hook; this build provides no reader."""
    raise NotImplementedError("this build provides no external reader hook")


__all__ = [
    "to_markdown",
    "to_text",
    "to_json",
    "use_layout",
    "convert_batch",
    "get_key_values",
    "LlamaMarkdownReader",
    "VERSION",
    "version",
    "VERSION_TUPLE",
    "version_tuple",
    "batch_converter",
    "helpers",
    "ocr",
    "worker_sizing",
    "versions_file",
    "FileDataError",
    "EmptyFileError",
]
