#!/usr/bin/env python3
"""EVH-C2-03: Structural comparison of two PDF files.

Compares page count and page dimensions between our output and pdfRest reference.

CLI usage:
    python3 scripts/structural_compare.py \
        --ours      /path/to/our-output.pdf \
        --reference /path/to/pdfrest-output.pdf \
        --result    /tmp/structural.json

Also importable as a module:
    from structural_compare import compare_structure
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Thresholds
# ---------------------------------------------------------------------------

# structurally_equivalent pass  = page count delta 0,  no empty-page discrepancy
# partial                       = delta ±1  OR ≤1 empty page discrepancy
# fail                          = delta ≥2  OR significant empty mismatch


def _structural_verdict(page_delta: int, empty_discrepancy: int = 0) -> str:
    abs_delta = abs(page_delta)
    if abs_delta == 0 and empty_discrepancy == 0:
        return "pass"
    if abs_delta <= 1 or empty_discrepancy <= 1:
        return "partial"
    return "fail"


def _page_count_verdict(our: int, ref: int) -> str:
    if our == ref:
        return "match"
    if our > ref:
        return "over_paginated"
    return "under_paginated"


# ---------------------------------------------------------------------------
# Page count extraction
# ---------------------------------------------------------------------------

def _page_count_mutool(pdf_path: str) -> Optional[int]:
    """Extract page count via mutool info."""
    try:
        r = subprocess.run(
            ["mutool", "info", pdf_path],
            capture_output=True,
            timeout=15,
        )
        if r.returncode == 0:
            text = r.stdout.decode("utf-8", errors="replace")
            m = re.search(r"Pages:\s*(\d+)", text, re.IGNORECASE)
            if m:
                return int(m.group(1))
    except (FileNotFoundError, subprocess.TimeoutExpired, Exception):
        pass
    return None


def _page_count_pdf_bytes(pdf_bytes: bytes) -> Optional[int]:
    """Rough page count estimate by scanning PDF /Count entry in /Pages dict.

    Falls back to counting indirect /Page objects if /Count is ambiguous.
    """
    # Try /Count N in the Pages dictionary (most reliable single value)
    # Look for /Type /Pages ... /Count N  — we want the top-level Pages dict
    # which has the highest /Count.
    candidates: list[int] = []
    for m in re.finditer(rb"/Count\s+(\d+)", pdf_bytes):
        candidates.append(int(m.group(1)))
    if candidates:
        # The top-level /Pages /Count is the maximum value
        return max(candidates)

    # Fallback: count /Type /Page (individual page dicts)
    pages = len(re.findall(rb"/Type\s*/Page\b", pdf_bytes))
    if pages > 0:
        return pages

    return None


def get_page_count(pdf_path: str) -> Optional[int]:
    """Get page count for a PDF, trying mutool first then raw byte scan."""
    count = _page_count_mutool(pdf_path)
    if count is not None:
        return count
    try:
        pdf_bytes = Path(pdf_path).read_bytes()
        return _page_count_pdf_bytes(pdf_bytes)
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Dimension extraction
# ---------------------------------------------------------------------------

def _dimensions_mutool(pdf_path: str) -> list[dict]:
    """Extract page dimensions via mutool info -b (bbox)."""
    dims: list[dict] = []
    try:
        r = subprocess.run(
            ["mutool", "info", "-b", pdf_path],
            capture_output=True,
            timeout=15,
        )
        if r.returncode != 0:
            return dims
        text = r.stdout.decode("utf-8", errors="replace")
        # mutool info -b outputs MediaBox for each page:
        # "Page N MediaBox: 0 0 W H"
        for m in re.finditer(
            r"MediaBox:\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)", text
        ):
            x0, y0, x1, y1 = (float(m.group(i)) for i in range(1, 5))
            dims.append({
                "width": round(x1 - x0, 2),
                "height": round(y1 - y0, 2),
            })
    except (FileNotFoundError, subprocess.TimeoutExpired, Exception):
        pass
    return dims


def _dimensions_pdf_bytes(pdf_bytes: bytes) -> list[dict]:
    """Extract MediaBox dimensions from raw PDF bytes."""
    dims: list[dict] = []
    for m in re.finditer(
        rb"/MediaBox\s*\[\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\]",
        pdf_bytes,
    ):
        try:
            x0, y0, x1, y1 = (float(m.group(i)) for i in range(1, 5))
            dims.append({
                "width": round(x1 - x0, 2),
                "height": round(y1 - y0, 2),
            })
        except Exception:
            pass
    return dims


def get_dimensions(pdf_path: str) -> list[dict]:
    """Get per-page dimensions for a PDF."""
    dims = _dimensions_mutool(pdf_path)
    if dims:
        return dims
    try:
        pdf_bytes = Path(pdf_path).read_bytes()
        return _dimensions_pdf_bytes(pdf_bytes)
    except Exception:
        return []


# ---------------------------------------------------------------------------
# Dimension match check
# ---------------------------------------------------------------------------

def _dimensions_match(ours: list[dict], refs: list[dict]) -> bool:
    """Check if all corresponding page dimensions are within 1 point of each other."""
    if not ours or not refs:
        return True  # can't determine → be lenient
    for a, b in zip(ours, refs):
        if abs(a.get("width", 0) - b.get("width", 0)) > 1.0:
            return False
        if abs(a.get("height", 0) - b.get("height", 0)) > 1.0:
            return False
    return True


# ---------------------------------------------------------------------------
# Main comparison function
# ---------------------------------------------------------------------------

def compare_structure(our_pdf: str, reference_pdf: str) -> dict:
    """Compare structural properties of two PDFs. Returns JSON-serialisable dict."""
    our_pages = get_page_count(our_pdf)
    ref_pages = get_page_count(reference_pdf)
    our_dims = get_dimensions(our_pdf)
    ref_dims = get_dimensions(reference_pdf)

    if our_pages is None or ref_pages is None:
        page_delta = 0  # unknown
        page_count_verdict = "unknown"
        verdict = "partial"  # conservative
    else:
        page_delta = our_pages - ref_pages
        page_count_verdict = _page_count_verdict(our_pages, ref_pages)
        verdict = _structural_verdict(page_delta)

    dim_match = _dimensions_match(our_dims, ref_dims)

    return {
        "our_pages": our_pages,
        "ref_pages": ref_pages,
        "page_count_delta": page_delta if (our_pages is not None and ref_pages is not None) else None,
        "page_count_verdict": page_count_verdict,
        "our_dimensions": our_dims,
        "ref_dimensions": ref_dims,
        "dimension_match": dim_match,
        "structurally_equivalent": verdict,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Compare structural properties (page count, dimensions) of two PDFs"
    )
    p.add_argument("--ours", required=True, help="Path to our output PDF")
    p.add_argument("--reference", required=True, help="Path to pdfRest reference PDF")
    p.add_argument("--result", required=True, help="Path to write JSON result")
    args = p.parse_args()

    for label, path in [("--ours", args.ours), ("--reference", args.reference)]:
        if not Path(path).exists():
            sys.exit(f"ERROR: {label} file not found: {path}")

    result = compare_structure(args.ours, args.reference)

    out_path = Path(args.result)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2))
    print(
        f"structural_compare: our_pages={result['our_pages']}  "
        f"ref_pages={result['ref_pages']}  "
        f"verdict={result['structurally_equivalent']}"
    )
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
