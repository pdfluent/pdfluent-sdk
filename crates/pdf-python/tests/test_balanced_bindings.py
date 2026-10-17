"""The balanced tier through the Python binding (#661).

The CLI is the reference: ``extract --quality balanced`` runs the learned
layout model, and ``--table-model-dir`` opts table regions into TATR
structure. The wheel ships both behind the ``ort-layout`` / ``ort-tables``
cargo features (always on in wheel builds), with weights installed by
``pdfluent models install layout tables``.

BINDING CONTRACT (defined here; L7b implements it). The keywords mirror the
CLI option names and values exactly, in snake_case:

- ``Document.extract_markdown(..., quality="balanced", model_dir=None,
  table_model_dir=None)``. ``quality`` is ``"fast"`` (default) or
  ``"balanced"``; anything else raises ``PdfluentValidationError`` naming
  the value, like the existing ``page_separators`` mapping. ``model_dir``
  and ``table_model_dir`` are optional directory paths (``str`` or
  ``None``); a ``model_dir`` of ``None`` resolves through
  ``PDFLUENT_MODEL_DIR`` to the per-user model dir, exactly as the shared
  runtime resolves it, while a ``table_model_dir`` of ``None`` leaves
  TATR off.
- ``Document.extract_blocks`` takes the same keywords (both methods take
  the same options struct), but ``quality="balanced"`` is markdown-only in
  the engine: it raises ``PdfluentValidationError`` with the engine's
  "markdown-only" message, never the fast tier under a balanced name.
- Errors. A balanced call without the ``ort-layout`` build (musl, or any
  platform without ONNX Runtime) raises the product's typed model error:
  ``PdfluentError`` whose message contains "is not available" and the
  install command ``pdfluent models install layout``. The same holds with
  the feature but without the weights, where the message additionally
  names the path looked at. A ``table_model_dir`` opt-in without the
  ``ort-tables`` build raises ``PdfluentValidationError`` naming
  ``ort-tables``; with the build but without the tables weights it raises
  the typed model error naming the ``tatr-v1.1-all`` path and the
  ``pdfluent models install tables`` command.

When the weights are not installed the parity tests skip with the
tier's own message as the reason -- never a silent pass -- while the
error-contract tests run on any build.

References live in ``tests/data/balanced/``: per fixture the CLI's exact
``--quality balanced --table-model-dir`` markdown plus the block kinds in
order, the headings, and the table cell grids parsed from it. The parity
tests assert both the exact markdown and the parsed kinds/order/grids.
Parity runs balanced with ``table_model_dir`` at the installed models
(``PDFLUENT_MODEL_DIR`` else the per-user default); ``model_dir`` stays
``None`` so the tier's own resolution is what parity exercises.

Run:
    cd crates/pdf-python
    maturin develop --features ort-layout,ort-tables
    pdfluent models install layout tables
    pytest tests/test_balanced_bindings.py
"""

import json
import os
import re
import sys

import pytest

pdfluent = pytest.importorskip("pdfluent")

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "..")
DATA = os.path.join(os.path.dirname(__file__), "data", "balanced")


def installed_model_root():
    """Where ``pdfluent models install`` put the weights.

    The ``PDFLUENT_MODEL_DIR`` override else the per-user default, the
    same root the shared runtime resolves to -- the parity ``table_model_dir``.
    """
    override = os.environ.get("PDFLUENT_MODEL_DIR")
    if override:
        return override
    if sys.platform == "win32":
        base = os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
    elif sys.platform == "darwin":
        base = os.path.expanduser("~/Library/Application Support")
    else:
        base = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    return os.path.join(base, "pdfluent", "models")

INSTALL_LAYOUT = "pdfluent models install layout"
INSTALL_TABLES = "pdfluent models install tables"
TATR_MODEL = "tatr-v1.1-all"
NOT_AVAILABLE = "is not available"

REFERENCE_FILES = [
    "structured.json",
    "uksi_20200121_p50.json",
    "nasdaq_wwd_2002_p7.json",
    "pmc2438475_p1_table_notes.json",
]


def load_reference(name):
    """One recorded CLI reference: the markdown plus its parsed shape."""
    with open(os.path.join(DATA, name), encoding="utf-8") as handle:
        return json.load(handle)


def strip_tags(text):
    """A table cell's inner text: tags out, surrounding space off.

    No entity decoding: the balanced tier writes raw ``&`` (nasdaq), and a
    decoder the other binding lacks would split the two suites' parsers.
    """
    return re.sub(r"<[^>]*>", "", text).strip()


def parse_table(block):
    """One ``<table>`` block as a grid: rows of cell texts."""
    rows = []
    for row in re.findall(r"<tr[^>]*>(.*?)</tr>", block, re.S):
        rows.append(
            [strip_tags(cell) for cell in re.findall(r"<t[hd][^>]*>(.*?)</t[hd]>", row, re.S)]
        )
    return rows


def parse_markdown(markdown):
    """Block kinds in order, headings, and table grids from markdown.

    Blocks split on blank lines: ``#``-led is a heading (level kept),
    ``- ``-led is a list item, ``<table>``-led is a table (grid kept),
    anything else a paragraph. The same rules the references were parsed
    with, so a drift in either direction fails the comparison.
    """
    kinds = []
    headings = []
    tables = []
    for block in [part.strip() for part in re.split(r"\n\s*\n", markdown.strip())]:
        if not block:
            continue
        match = re.match(r"(#{1,3})\s+(.*)", block, re.S)
        if match:
            kinds.append("heading")
            headings.append({"level": len(match.group(1)), "text": match.group(2).strip()})
        elif block.startswith("- "):
            kinds.append("list_item")
        elif block.startswith("<table>"):
            kinds.append("table")
            tables.append(parse_table(block))
        else:
            kinds.append("paragraph")
    return kinds, headings, tables


_probe_result = None


def balanced_skip_reason():
    """None when the balanced tier runs here, else why the parity tests skip.

    Probed once per session on the smallest fixture. A missing keyword
    is NOT a skip: the keywords landed, so a TypeError here is real
    breakage and the tests must fail on it. Only the product's own model
    error -- feature or weights absent -- skips, carrying the tier's
    message as the reason.
    """
    global _probe_result
    if _probe_result is not None:
        return _probe_result
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    try:
        with pdfluent.Document(fixture) as doc:
            doc.extract_markdown(quality="balanced", table_model_dir=installed_model_root())
    except TypeError:
        _probe_result = False
    except Exception as exc:
        # The binding's typed errors all derive from its base error; the
        # probe branches on the product's message, not the class.
        if NOT_AVAILABLE in str(exc):
            _probe_result = f"balanced tier not installed: {exc}"
        else:
            raise
    else:
        _probe_result = False
    return _probe_result


@pytest.fixture()
def require_balanced():
    """Skip the parity tests where the tier cannot run, naming the cause."""
    reason = balanced_skip_reason()
    if reason:
        pytest.skip(reason)


@pytest.mark.parametrize("reference_file", REFERENCE_FILES)
def test_balanced_markdown_matches_the_cli_reference(require_balanced, reference_file):
    reference = load_reference(reference_file)
    fixture = os.path.join(ROOT, reference["fixture"])
    with pdfluent.Document(fixture) as doc:
        markdown = doc.extract_markdown(quality="balanced", table_model_dir=installed_model_root())
    assert markdown == reference["markdown"]
    kinds, headings, tables = parse_markdown(markdown)
    assert kinds == reference["kinds"]
    assert headings == reference["headings"]
    assert tables == reference["tables"]


def test_quality_defaults_to_fast():
    """Omitting ``quality`` is the shipping engine, byte for byte."""
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    with pdfluent.Document(fixture) as doc:
        assert doc.extract_markdown() == doc.extract_markdown(quality="fast")


def test_balanced_blocks_is_markdown_only():
    """``extract_blocks`` refuses balanced with the engine's own message."""
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    with pdfluent.Document(fixture) as doc:
        with pytest.raises(pdfluent.PdfluentValidationError, match="markdown-only"):
            doc.extract_blocks(quality="balanced")


def test_an_unknown_quality_is_rejected():
    """An unknown ``quality`` reads as a mistake in the call, not I/O."""
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    with pdfluent.Document(fixture) as doc:
        with pytest.raises(pdfluent.PdfluentValidationError, match="quality"):
            doc.extract_markdown(quality="ultra")


def test_an_empty_model_dir_raises_the_typed_model_error(tmp_path):
    """``model_dir`` points the tier at scratch: nothing there is an error.

    On a build with the feature the message names the empty dir and the
    install; without the feature it names the feature and the install.
    Either way the typed model error: "is not available" plus the layout
    install command.
    """
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    with pdfluent.Document(fixture) as doc:
        with pytest.raises(pdfluent.PdfluentError) as caught:
            doc.extract_markdown(quality="balanced", model_dir=str(tmp_path))
    message = str(caught.value)
    assert NOT_AVAILABLE in message
    assert INSTALL_LAYOUT in message


def test_table_model_dir_without_weights_refuses(tmp_path):
    """The TATR opt-in never runs silent fast: it refuses naming the cause.

    With the ``ort-tables`` build but no tables weights the typed model
    error names the ``tatr-v1.1-all`` path looked at; without the build
    the options error names ``ort-tables``.
    """
    fixture = os.path.join(ROOT, "fixtures", "structured.pdf")
    with pdfluent.Document(fixture) as doc:
        with pytest.raises(pdfluent.PdfluentError) as caught:
            doc.extract_markdown(table_model_dir=str(tmp_path))
    message = str(caught.value)
    if "ort-tables" in message:
        return
    assert NOT_AVAILABLE in message
    assert TATR_MODEL in message
    assert INSTALL_TABLES in message
