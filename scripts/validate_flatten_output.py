#!/usr/bin/env python3
"""EVH-C2-05: Validate that a flattened PDF has no XFA artifacts.

Scans PDF bytes for XFA-related markers and basic structural validity.
No external dependencies — stdlib + raw byte scanning only.

CLI usage:
    python3 scripts/validate_flatten_output.py \
        --pdf /path/to/output.pdf \
        --result /tmp/flatten.json

Also importable as a module:
    from validate_flatten_output import validate_flatten
"""

# VALIDATOR FIX DESIGN (FSC-04/FSC-06)
#
# Current: _has_xfa() and _has_widget_annotations() use raw byte scan.
# This causes false positives when orphaned lopdf objects remain in the byte
# stream after catalog references are removed.
#
# Fix: replace raw scans with structural traversal using pikepdf:
#   _has_xfa(): pdf.Root -> /AcroForm -> /XFA (None if not reachable)
#   _has_widget_annotations(): all pages -> /Annots -> /Subtype == /Widget
#
# Fallback: if pikepdf is unavailable, use a context-aware scan like
# _has_acroform(). Do not fall back to bare `b"/XFA" in pdf_bytes`.
#
# Validation primacy (FLATTEN_OUTPUT_SPEC.md):
#   L1 (structural) is the locking gate. L2 (bytes) is informational only.
#   flatten_clean = "pass" when the catalog is clean, even if orphaned bytes
#   remain in the serialized output.

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Optional

# Minimum size for a valid PDF (rough guard)
MIN_PDF_SIZE = 1024  # 1 KB


# ---------------------------------------------------------------------------
# Raw-byte scan helpers
# ---------------------------------------------------------------------------

def _has_xfa(pdf_bytes: bytes) -> bool:
    """True if /XFA key is present in the PDF bytes."""
    return b"/XFA" in pdf_bytes


def _has_needs_rendering(pdf_bytes: bytes) -> bool:
    """True if /NeedsRendering key is present."""
    return b"/NeedsRendering" in pdf_bytes


def _has_widget_annotations(pdf_bytes: bytes) -> bool:
    """True if /Widget subtype annotations are present."""
    return b"/Widget" in pdf_bytes


def _has_acroform(pdf_bytes: bytes) -> bool:
    """True if /AcroForm is present in the catalog pointing at a real dictionary.

    A bare '/AcroForm null' is acceptable (AcroForm was nulled out).
    We flag it only if /AcroForm is followed by a reference (N G R) or '<<'.
    """
    # Find all occurrences of /AcroForm
    pos = 0
    pattern = b"/AcroForm"
    while True:
        idx = pdf_bytes.find(pattern, pos)
        if idx == -1:
            break
        # Look at the bytes immediately following /AcroForm (skip whitespace)
        after_start = idx + len(pattern)
        after = pdf_bytes[after_start:after_start + 40].lstrip()

        # Acceptable: "null" or end of dict ">>" — these mean no AcroForm
        if after.startswith(b"null") or after.startswith(b">>"):
            pos = idx + 1
            continue

        # Not acceptable: reference "N G R" or inline dict "<<"
        if after.startswith(b"<<"):
            return True
        # Indirect reference pattern: digits whitespace digits whitespace R
        if re.match(rb"\d+\s+\d+\s+R", after):
            return True

        pos = idx + 1

    return False


def _has_content(pdf_bytes: bytes) -> bool:
    """True if the PDF has at least some content (size > MIN_PDF_SIZE and
    at least one /Page object with a content stream).
    """
    if len(pdf_bytes) < MIN_PDF_SIZE:
        return False

    # Check there is at least one /Type /Page
    if not re.search(rb"/Type\s*/Page\b", pdf_bytes):
        return False

    # Check at least one stream body exists (content streams)
    if b"stream" not in pdf_bytes:
        return False

    return True


def _is_valid_pdf(pdf_bytes: bytes) -> bool:
    """Very basic PDF header check."""
    return pdf_bytes[:5] == b"%PDF-"


# ---------------------------------------------------------------------------
# Main validation function
# ---------------------------------------------------------------------------

def validate_flatten(pdf_path: str) -> dict:
    """Validate a flattened PDF for XFA artifacts. Returns JSON-serialisable dict."""
    issues: list[str] = []

    try:
        pdf_bytes = Path(pdf_path).read_bytes()
    except Exception as exc:
        return {
            "is_valid_pdf": False,
            "has_acroform": None,
            "has_xfa": None,
            "has_needs_rendering": None,
            "has_widget_annotations": None,
            "has_content": False,
            "flatten_clean": "fail",
            "issues": [f"could not read file: {exc}"],
        }

    valid_pdf = _is_valid_pdf(pdf_bytes)
    if not valid_pdf:
        issues.append("file does not start with PDF header (%PDF-)")

    has_xfa = _has_xfa(pdf_bytes)
    has_acroform = _has_acroform(pdf_bytes)
    has_needs_rendering = _has_needs_rendering(pdf_bytes)
    has_widget = _has_widget_annotations(pdf_bytes)
    has_content = _has_content(pdf_bytes)

    if has_xfa:
        issues.append("/XFA stream present — document not fully flattened")
    if has_needs_rendering:
        issues.append("/NeedsRendering flag present — document requires rendering")
    if has_widget:
        issues.append("/Widget annotations present — form fields not flattened")
    if not has_content:
        if len(pdf_bytes) < MIN_PDF_SIZE:
            issues.append(f"file too small ({len(pdf_bytes)} bytes) — likely degenerate")
        else:
            issues.append("no content pages detected")

    # flatten_clean: pass only if no XFA artifacts and has content
    # Note: has_acroform is informational — AcroForm without XFA is acceptable
    flatten_clean = (
        "pass"
        if (not has_xfa and not has_needs_rendering and not has_widget and has_content)
        else "fail"
    )

    return {
        "is_valid_pdf": valid_pdf,
        "has_acroform": has_acroform,
        "has_xfa": has_xfa,
        "has_needs_rendering": has_needs_rendering,
        "has_widget_annotations": has_widget,
        "has_content": has_content,
        "flatten_clean": flatten_clean,
        "issues": issues,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Validate flattened PDF: check for XFA artifacts and basic structural validity"
    )
    p.add_argument("--pdf", required=True, help="Path to the flattened output PDF")
    p.add_argument("--result", required=True, help="Path to write JSON result")
    args = p.parse_args()

    if not Path(args.pdf).exists():
        sys.exit(f"ERROR: --pdf file not found: {args.pdf}")

    result = validate_flatten(args.pdf)

    out_path = Path(args.result)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2))
    print(
        f"validate_flatten: clean={result['flatten_clean']}  "
        f"xfa={result['has_xfa']}  "
        f"needs_rendering={result['has_needs_rendering']}  "
        f"widget={result['has_widget_annotations']}"
    )
    if result["issues"]:
        for issue in result["issues"]:
            print(f"  ISSUE: {issue}")
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
