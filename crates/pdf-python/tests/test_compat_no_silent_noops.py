# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No silent no-ops on the Python door (#643).

Mirrors crates/pdfluent/tests/compat_no_silent_noops.rs: every changed
member raises PdfluentError with the shared reason instead of returning
success, except the members the door already implements
(get_bboxlog, get_displaylist, resolve_link, set_layer_ui_config,
layer_ui_configs, the journal, get_highlight_selection,
util_ensure_widget_calc, Page.write_text, repair_mono_font,
UpdateFontInfo), which pin behaviour instead, and bake /
convert_to_pdf, which #648/#646 implement (see
test_compat_document_bake.py / test_compat_document_convert.py).

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_no_silent_noops.py -v
"""

from __future__ import annotations

import os
import pathlib
import re

import pytest

pdfluent_native = pytest.importorskip("pdfluent._native")

Document = pdfluent_native.Document
PdfluentError = pdfluent_native.PdfluentError

FIXTURES = os.path.join(os.path.dirname(__file__), "..", "..", "..", "fixtures")
SAMPLE = os.path.join(FIXTURES, "sample.pdf")

REASON = "not supported in this version: {}; see KNOWN_DIFFERENCES"


def _check(member: str, fn) -> None:
    with pytest.raises(PdfluentError) as exc:
        fn()
    assert str(exc.value) == REASON.format(member), member


def test_layout() -> None:
    doc = Document(SAMPLE)
    try:
        _check("Document.layout", lambda: doc.layout())
    finally:
        doc.close()


def test_get_bboxlog() -> None:
    # The L12 implementation wins over the no-op raise (#643 merge): the
    # door returns one fill-text entry per captured line. Content pins live
    # with the bboxlog cases; this only guards the raise stays gone.
    doc = Document(SAMPLE)
    try:
        entries = doc[0].get_bboxlog()
    finally:
        doc.close()
    assert isinstance(entries, list)
    assert entries, "sample page has text lines"
    kind, rect = entries[0]
    assert kind == "fill-text"
    assert rect[2] > rect[0] and rect[3] > rect[1]


def test_get_displaylist() -> None:
    # The L12 implementation wins over the no-op raise (#643 merge).
    doc = Document(SAMPLE)
    try:
        display = doc[0].get_displaylist()
    finally:
        doc.close()
    assert type(display).__name__ == "DisplayList"
    assert display.rect.x1 > display.rect.x0
    assert display.rect.y1 > display.rect.y0


def test_apply_css() -> None:
    doc = Document(SAMPLE)
    try:
        _check("Document.apply_css", lambda: doc.apply_css("", True))
    finally:
        doc.close()


def test_repair() -> None:
    doc = Document(SAMPLE)
    try:
        _check("Document.repair", lambda: doc.repair())
    finally:
        doc.close()


def test_resolve_link() -> None:
    # The L34 implementation wins over the no-op raise (#643 merge).
    doc = Document(SAMPLE)
    try:
        assert doc.resolve_link() == [-1, 0, 0]
        assert doc.resolve_link("#page=2") == [1, None, None]
    finally:
        doc.close()


def test_set_layer_ui_config() -> None:
    # The L34 implementation wins over the no-op raise (#643 merge): the
    # fixture has no layer interfaces, so index 0 reports a bad value.
    doc = Document(SAMPLE)
    try:
        with pytest.raises(Exception, match="no layer interface 0"):
            doc.set_layer_ui_config(0, 0)
    finally:
        doc.close()


# Allow-listed real no-ops on the door: (file stem, method, reason).
REAL_NOOPS = [
    ("document", "init_doc", "open already fully opens; finishing is idempotent"),
    ("document", "resolve_names", "destinations resolve on read"),
    ("document", "refresh", "reads come straight from the document"),
    ("document", "get_layout", "fixed-layout has no reflow layout; None is correct"),
    (
        "document",
        "get_oc_items",
        "recording pins empty for pages without layers",
    ),
    ("document", "is_wrapped", "fresh pages are unwrapped"),
    (
        "text",
        "check_font_info",
        "recording pins None for live and dead xrefs; the check answers nothing",
    ),
]


def _split_fns(text: str) -> list[tuple[str, str]]:
    lines = text.splitlines()
    out: list[tuple[str, str]] = []
    i = 0
    while i < len(lines):
        m = re.search(r"^\s*fn (\w+)\s*\(", lines[i])
        if not m:
            i += 1
            continue
        name = m.group(1)
        j = i
        while j < len(lines) and "{" not in lines[j]:
            j += 1
        if j >= len(lines):
            break
        depth = 0
        k = j
        closed = False
        while k < len(lines):
            depth += lines[k].count("{") - lines[k].count("}")
            if depth == 0:
                closed = True
                break
            k += 1
        if not closed:
            break
        out.append((name, "\n".join(lines[j : k + 1])))
        i = k + 1
    return out


def _is_noop(name: str, body: str) -> bool:
    if name.startswith("with_") or name in {"new", "default"}:
        return False
    code = [
        line.strip()
        for line in body.splitlines()
        if line.strip()
        and not line.strip().startswith("//")
        and line.strip() not in {"{", "}"}
    ]
    if len(code) > 6:
        return False
    joined = "\n".join(code)
    has_let = "let _ =" in joined
    ok_unit = "Ok(())" in joined
    ok_none = (
        "Ok(py.None())" in joined
        or "Ok(None" in joined
        or "Ok(Vec::new())" in joined
        or "Ok(String::new())" in joined
    )
    mentions = "nothing to " in body or "no-op" in body
    return (ok_unit and (has_let or mentions or len(code) == 1)) or (
        ok_none and has_let
    )


def test_no_silent_noop_without_allowance() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent / "src" / "compat"
    offenders: list[str] = []
    for stem in ["document", "edit", "text", "pixmap", "geometry"]:
        text = (root / f"{stem}.rs").read_text(encoding="utf-8")
        for name, body in _split_fns(text):
            if _is_noop(name, body) and not any(
                f == stem and m == name for f, m, _ in REAL_NOOPS
            ):
                offenders.append(f"{stem}.rs::{name}")
    assert not offenders, f"silent no-op doors without reason: {offenders}"
    for stem, method, reason in REAL_NOOPS:
        assert reason, f"{stem}::{method} needs a reason"
        text = (root / f"{stem}.rs").read_text(encoding="utf-8")
        assert f"fn {method}" in text, f"allow-list names missing {stem}::{method}"
