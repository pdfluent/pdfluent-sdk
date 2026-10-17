#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The coverage matcher counts what a binding ships, and what a test calls (#510).

This guard is a ratchet, so a wrong answer here does not merely mislead: it
either lowers a floor on an invented gap, or lets a real one through. Three
readings were wrong on 10-09-2026, all silently, because the only job that ran
the guard was hosted and hosted Actions had been stopped for five days:

  * a doc comment saying `Kept out of #[pymethods]` opened an impl block, so
    the two helpers the comment describes were counted as Python methods;
  * a private `office_export` body inside a `#[wasm_bindgen]` impl was counted
    as an exported method, in two bindings, while no caller can reach it;
  * `getattr(doc, method)()` fed by a parametrize list read as nothing, so the
    three Office exports read untested while their test exported, unzipped and
    asserted on all three.

Each case below drives one rule on a fragment written here, so the test says
what the rule is rather than what one tree happens to contain. The last block
runs the real tree.
"""
from __future__ import annotations

import io
import contextlib
import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("api_coverage", HERE / "api_coverage.py")
guard = importlib.util.module_from_spec(_spec)
sys.modules["api_coverage"] = guard
_spec.loader.exec_module(guard)

MINIMUM_CASES = 11  # FLOOR

COMMENT_NAMES_THE_ATTRIBUTE = '''
#[pymethods]
impl PyDocument {
    fn save(&self) {}
}

/// Plumbing for the exporters. Kept out of `#[pymethods]` so PyO3 does not
/// expose it to Python.
impl PyDocument {
    fn current_bytes(&self) {}
}
'''

A_PRIVATE_HELPER_IN_A_PUBLIC_IMPL = '''
#[wasm_bindgen]
impl PdfDocument {
    #[wasm_bindgen(js_name = "toDocx")]
    pub fn to_docx(&self) {}

    /// Shared body for the three Office exports.
    fn office_export(&self) {}
}
'''

DISPATCHED_BY_NAME = '''
def test_one(doc):
    assert getattr(doc, "to_numpy")()
'''

DISPATCHED_BY_PARAMETRIZE = '''
import pytest

@pytest.mark.parametrize(
    ("method", "entry"),
    [
        ("to_docx", "word/document.xml"),
        ("to_xlsx", "xl/workbook.xml"),
    ],
)
def test_export(doc, method, entry):
    package = getattr(doc, method)()
    assert entry in package
'''

A_STRING_THAT_IS_NOT_A_CALL = '''
def test_metadata(doc):
    assert doc.metadata()["title"] == "Report"
    assert doc.metadata()["to_pptx"] is None
'''

# Asking whether a method exists is not running it. A test that keeps the
# introspection and loses the invocation has stopped exercising the binding,
# and a ratchet that counts it stays green over exactly that. Reported by
# codex on #1865.
LOOKED_UP_BUT_NEVER_CALLED = '''
import pytest

def test_the_method_is_there(doc):
    assert callable(getattr(doc, "to_numpy"))

@pytest.mark.parametrize("method", ["to_docx", "to_xlsx"])
def test_the_method_is_documented(doc, method):
    assert getattr(doc, method).__doc__
'''


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    # -- what a binding ships -------------------------------------------------
    shipped = guard.exports_in_impl_blocks(COMMENT_NAMES_THE_ATTRIBUTE, ["#[pymethods]"])
    case("a real #[pymethods] block ships its methods", "save" in shipped, str(shipped))
    case("a doc comment that names #[pymethods] ships nothing",
         "current_bytes" not in shipped, str(shipped))

    shipped = guard.exports_in_impl_blocks(A_PRIVATE_HELPER_IN_A_PUBLIC_IMPL, ["#[wasm_bindgen]"])
    case("a #[wasm_bindgen] impl ships its public methods", "to_docx" in shipped, str(shipped))
    case("and its exported name, not the Rust one",
         shipped.get("to_docx") == "toDocx", str(shipped))
    case("a private helper in that impl is not something a caller can reach",
         "office_export" not in shipped, str(shipped))

    shipped = guard.exports_in_impl_blocks(A_PRIVATE_HELPER_IN_A_PUBLIC_IMPL, ["#[napi]"])
    case("an attribute that opens no impl here finds nothing", shipped == {}, str(shipped))

    # -- what a test calls ----------------------------------------------------
    called = guard.dispatched_names([DISPATCHED_BY_NAME])
    case("getattr(doc, \"to_numpy\")() is a call to to_numpy",
         called == {"to_numpy"}, str(called))

    called = guard.dispatched_names([DISPATCHED_BY_PARAMETRIZE])
    case("a parametrised getattr calls every case it is given",
         called == {"to_docx", "to_xlsx"}, str(called))

    called = guard.dispatched_names([A_STRING_THAT_IS_NOT_A_CALL])
    case("a bare string that happens to be a method name is not a call",
         called == set(), str(called))

    called = guard.dispatched_names([LOOKED_UP_BUT_NEVER_CALLED])
    case("an attribute that is looked up and never invoked is not a call",
         called == set(), str(called))

    # -- the real tree --------------------------------------------------------
    buf = io.StringIO()
    code = 0
    try:
        with contextlib.redirect_stdout(buf):
            guard.main()
    except SystemExit as stop:
        code = stop.code or 0
    measured = sum(len(b.covered) for b in guard.BINDINGS)
    case(f"the tree is at or above its floor of {guard.FLOOR}: {measured}",
         code == 0 and measured >= guard.FLOOR, buf.getvalue()[-400:])
    case("and the floor is the measured number, not a number above it",
         guard.FLOOR <= measured, f"FLOOR={guard.FLOOR} measured={measured}")

    if cases < MINIMUM_CASES:
        print(f"\nFAIL: {cases} cases ran, at least {MINIMUM_CASES} were written")
        return 1
    if failures:
        print(f"\nFAIL: {len(failures)} of {cases} cases")
        return 1
    print(f"\nOK: {cases} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
