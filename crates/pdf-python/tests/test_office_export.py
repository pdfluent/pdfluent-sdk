"""Office export: Word, Excel and PowerPoint from a PDF.

Until 23-08-2026 no binding could do this while the feature page sold it.
The Rust crates existed and stopped at the language boundary.

The assertions look at the bytes rather than at the absence of an exception.
An OOXML package is a ZIP with a known entry, so "the call returned something"
and "Word opens this" are separable claims, and only the second one matters.
"""

from __future__ import annotations

import io
import json
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest

import pdfluent

FIXTURE = Path(__file__).resolve().parents[3] / "fixtures" / "sample.pdf"


def _sample() -> bytes:
    if not FIXTURE.exists():
        print(f"SKIPPED (not a pass): fixture missing at {FIXTURE}", file=sys.stderr)
        pytest.skip(f"fixture missing: {FIXTURE}")
    return FIXTURE.read_bytes()


@pytest.fixture(scope="module")
def licensed_doc() -> pdfluent.Document:
    # "enterprise", not "business". Two tier vocabularies exist: signed and
    # JSON payloads speak xfa_license's (trial/basic/professional/enterprise/
    # archival), while the dev shortcut `tier:X` speaks pdfluent's own
    # (trial/developer/team/business/enterprise). map_xfa_tier() bridges them,
    # and "business" is simply not a word on this side of the bridge -- passing
    # it raises "unknown license tier" and leaves the process on Trial.
    #
    # Only the first activation in a process takes effect (OnceLock in the
    # core), so another module may already have set a tier. A second activation
    # raises rather than silently downgrading -- hence the swallow.
    try:
        pdfluent.activate_license(
            json.dumps(
                {
                    "licensee": "Test Corp",
                    "company": "Test Corp Ltd",
                    "tier": "enterprise",
                    "seats": 5,
                }
            )
        )
    except pdfluent.PdfluentLicenseError:
        pass
    return pdfluent.Document(_sample())


@pytest.mark.parametrize(
    ("method", "entry"),
    [
        ("to_docx", "word/document.xml"),
        ("to_xlsx", "xl/workbook.xml"),
        ("to_pptx", "ppt/presentation.xml"),
    ],
)
def test_export_returns_a_package_office_opens(licensed_doc, method, entry):
    package = getattr(licensed_doc, method)()
    assert isinstance(package, bytes)
    with zipfile.ZipFile(io.BytesIO(package)) as zf:
        assert entry in zf.namelist(), f"{method}: {entry} missing from {zf.namelist()}"


# The licence is process-wide state. A refusal test that ran in this process
# would prove nothing, because the fixture above has already activated a
# Business key and whichever ran first would decide the answer for both. The
# C ABI suite made exactly that mistake: deleting the capability check left
# every test green. So the refusal runs in a virgin interpreter.
REFUSAL_PROBE = """
import sys, pdfluent
doc = pdfluent.Document(open(sys.argv[1], "rb").read())
for method in ("to_docx", "to_xlsx", "to_pptx"):
    try:
        getattr(doc, method)()
    except pdfluent.PdfluentLicenseError as e:
        assert "pdfluent.com" in str(e), f"{method}: no route to a key in {e!r}"
    else:
        raise AssertionError(f"{method} succeeded without a licence")
print("refused")
"""


def test_export_refuses_without_a_licence():
    result = subprocess.run(
        [sys.executable, "-c", REFUSAL_PROBE, str(FIXTURE)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    assert "refused" in result.stdout
