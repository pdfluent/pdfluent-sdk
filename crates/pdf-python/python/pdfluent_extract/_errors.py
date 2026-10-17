# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Shared input checks for every door entry point.

Contract for lanes: each entry that takes a document calls
:func:`check_call` first and returns its result unless it is
:data:`CONTINUE`. Lanes add their own option and input rules with
:func:`register_document_rule` and :func:`register_option_rule` from
their own module, so no two lanes edit the same file.

The core rules below pin the bad-input behaviour: a missing path
raises ``FileNotFoundError``, a zero-byte file ``EmptyFileError``,
unrecognised bytes ``FileDataError``, and a locked document raises
``TypeError`` in layout mode and ``ValueError`` in legacy mode.
"""

import os

_PDF_MAGIC = b"%PDF-"
_ZIP_MAGICS = (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08")
_PNG_MAGIC = b"\x89PNG"


class FileDataError(Exception):
    """Raised when input bytes do not open as a known document."""


class EmptyFileError(FileDataError):
    """Raised when the input file exists but holds zero bytes."""


class _ContinueType(object):
    def __repr__(self):
        return "CONTINUE"


#: Sentinel returned by :func:`check_call` when the entry should run.
CONTINUE = _ContinueType()

_DOCUMENT_RULES = []
_OPTION_RULES = {}


def register_document_rule(func):
    """Add a lane rule over the document argument.

    ``func`` takes ``(doc, layout=...)`` and either raises, returns a
    finished output value, or returns :data:`CONTINUE`.
    """
    _DOCUMENT_RULES.append(func)
    return func


def register_option_rule(entry, func):
    """Add a lane rule over one entry's option mapping.

    ``func`` takes ``(options, layout=...)`` with the same protocol as
    :func:`register_document_rule`.
    """
    _OPTION_RULES.setdefault(entry, []).append(func)
    return func


def _read_path_bytes(doc):
    # Raw bytes are document content, never a path: the lane rules decide
    # what they mean (the inputs lane refuses them outright).
    if isinstance(doc, (bytes, bytearray)):
        return None
    try:
        path = os.fspath(doc)
    except TypeError:
        return None
    if not isinstance(path, (str, bytes)):
        return None
    if not os.path.exists(path):
        raise FileNotFoundError("no such input file: %r" % (doc,))
    if not os.path.isfile(path):
        raise FileDataError("input path is not a file: %r" % (doc,))
    with open(path, "rb") as handle:
        return handle.read()


def _check_core_document(doc, layout):
    data = _read_path_bytes(doc)
    if data is None:
        return CONTINUE
    if len(data) == 0:
        raise EmptyFileError("input file is empty: %r" % (doc,))
    if data.startswith(_PDF_MAGIC):
        if b"/Encrypt" in data:
            if layout:
                raise TypeError("document is locked; open it with a password first")
            raise ValueError("document is locked; open it with a password first")
        return CONTINUE
    if data.startswith(_ZIP_MAGICS) or data.startswith(_PNG_MAGIC):
        return CONTINUE
    raise FileDataError("input bytes are not a supported document: %r" % (doc,))


def check_call(entry, doc, options, layout):
    """Run the core and lane rules for one entry call.

    Returns :data:`CONTINUE` when the entry should run, or a finished
    output value the entry must return as-is.
    """
    early = _check_core_document(doc, layout)
    if early is not CONTINUE:
        return early
    for rule in _DOCUMENT_RULES:
        early = rule(doc, layout=layout)
        if early is not CONTINUE:
            return early
    for rule in _OPTION_RULES.get(entry, ()):
        early = rule(options, layout=layout)
        if early is not CONTINUE:
            return early
    return CONTINUE
