"""PDFluent Python SDK — golden-path example.

Pinned to ``pdfluent==1.0.0b8`` (see ``requirements.txt``).

Demonstrates the canonical lifecycle:

1. Open a PDF (file path or bytes).
2. Read page count + metadata.
3. Extract text from the first page.
4. Typed error handling — uses the typed exception hierarchy exposed by the
   binding (``PdfluentError`` and its subclasses). Falls back to ``ValueError``
   and ``RuntimeError`` for older binding builds where the typed hierarchy
   is not yet exported.

There is no licence step. The binding has no key, no activation call and no
tier: every feature works in every build (#226). The licence you accept is
AGPL-3.0 or the commercial one, and neither is checked at runtime.

Run:
    python main.py path/to/file.pdf
    python main.py ../../tests/corpus-mini/multi-page.pdf

When called without an argument the script falls back to the in-repo
fixture so the example runs out of the box during development.

Install (during beta development against the in-tree binding)::

    pip install -e ../../crates/pdf-python

Or, when the binding is published to PyPI::

    pip install -r requirements.txt
"""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Tuple, Type

import pdfluent

# Default fixture used when the script is run without arguments.
DEFAULT_FIXTURE: Path = (
    Path(__file__).resolve().parent.parent.parent
    / "tests"
    / "corpus-mini"
    / "multi-page.pdf"
)


def _resolve_error_types() -> Tuple[Type[BaseException], ...]:
    """Return the typed pdfluent exception classes, falling back to builtins.

    The canonical in-tree binding exports a typed hierarchy
    (``PdfluentError`` and subclasses). Older or stripped builds raise
    plain ``ValueError`` / ``RuntimeError`` from the native layer — we
    accept both so the example remains useful across binding versions.
    """
    typed: list[Type[BaseException]] = []
    for name in (
        "PdfluentParseError",
        "PdfluentIoError",
        "PdfluentEncryptedError",
        "PdfluentPageRangeError",
        "PdfluentValidationError",
        "PdfluentRenderError",
        "PdfluentError",
    ):
        cls = getattr(pdfluent, name, None)
        if isinstance(cls, type) and issubclass(cls, BaseException):
            typed.append(cls)
    # Always include builtins as a safety net for older binding builds.
    typed.extend([ValueError, RuntimeError, OSError])
    return tuple(typed)


def run(path: Path) -> int:
    print(f"Opening : {path}")

    # ── Open the PDF. ────────────────────────────────────────────────────
    # ``Document`` is a context manager so the underlying handle is closed
    # deterministically even if an exception propagates.
    with pdfluent.Document(str(path)) as doc:
        # ── Page count. ──────────────────────────────────────────────────
        page_count: int = doc.page_count
        print(f"Pages   : {page_count}")

        # ── Metadata (read-only property). ───────────────────────────────
        meta = doc.metadata
        title = getattr(meta, "title", None) or "(none)"
        author = getattr(meta, "author", None) or "(none)"
        producer = getattr(meta, "producer", None) or "(none)"
        print(f"Title   : {title}")
        print(f"Author  : {author}")
        print(f"Producer: {producer}")

        # ── Text extraction from the first page. ─────────────────────────
        text: str = doc.extract_text(0)
        preview = text[:200].rstrip()
        suffix = "…" if len(text) > 200 else ""
        print(f"Text[0] : {preview}{suffix}")

    return 0


def main(argv: list[str]) -> int:
    # ── Resolve the input path. ──────────────────────────────────────────
    path = Path(argv[1]) if len(argv) > 1 else DEFAULT_FIXTURE
    if not path.exists():
        print(f"fixture not found: {path}", file=sys.stderr)
        return 10

    # ── Run the golden path with typed error handling. ───────────────────
    try:
        return run(path)
    except _resolve_error_types() as err:
        # Typed error handling — we inspect the exception class, not the
        # message string. ``code`` is exposed by typed errors that wrap
        # the C8 error catalogue.
        code = getattr(err, "code", err.__class__.__name__)
        print(f"[{code}] {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
