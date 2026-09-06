"""Office export: Word, Excel and PowerPoint from a PDF.

Until 23-08-2026 no binding could do this while the feature page sold it.
The Rust crates existed and stopped at the language boundary.

The assertions look at the bytes rather than at the absence of an exception.
An OOXML package is a ZIP with a known entry, so "the call returned something"
and "Word opens this" are separable claims, and only the second one matters.
"""

from __future__ import annotations

import io
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
def doc() -> pdfluent.Document:
    return pdfluent.Document(_sample())


@pytest.mark.parametrize(
    ("method", "entry"),
    [
        ("to_docx", "word/document.xml"),
        ("to_xlsx", "xl/workbook.xml"),
        ("to_pptx", "ppt/presentation.xml"),
    ],
)
def test_export_returns_a_package_office_opens(doc, method, entry):
    package = getattr(doc, method)()
    assert isinstance(package, bytes)
    with zipfile.ZipFile(io.BytesIO(package)) as zf:
        assert entry in zf.namelist(), f"{method}: {entry} missing from {zf.namelist()}"


# There is no licence key any more (#199, #226), so what is worth proving is
# that the environment cannot change the answer. A key in PDFLUENT_LICENSE_KEY
# used to decide the tier for the whole process, so the check runs in a virgin
# interpreter with that variable set: if anything still reads it, the export
# behaves differently there and nowhere else.
UNLICENSED_PROBE = """
import os, sys, pdfluent
os.environ["PDFLUENT_LICENSE_KEY"] = "tier:trial"
doc = pdfluent.Document(open(sys.argv[1], "rb").read())
for method in ("to_docx", "to_xlsx", "to_pptx"):
    package = getattr(doc, method)()
    assert package[:2] == b"PK", f"{method}: not an OOXML package"
print("exported")
"""


def test_export_needs_no_licence_and_ignores_the_environment():
    result = subprocess.run(
        [sys.executable, "-c", UNLICENSED_PROBE, str(FIXTURE)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    assert "exported" in result.stdout
