#!/usr/bin/env python3
"""EVH-C2-02: Text completeness comparison.

Extracts field values from the XFA datasets XML embedded in the original PDF,
then checks what fraction of those values are present in the rendered output PDF
and the pdfRest reference PDF.

CLI usage:
    python3 scripts/text_completeness_compare.py \
        --original /path/to/original.pdf \
        --output   /path/to/our-output.pdf \
        --reference /path/to/pdfrest-output.pdf \
        --result /tmp/completeness.json

Also importable as a module:
    from text_completeness_compare import check_completeness
"""
import argparse
import json
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Thresholds
# ---------------------------------------------------------------------------

COMPLETENESS_PASS = 0.90
COMPLETENESS_PARTIAL = 0.50


def _completeness_verdict(score: float | None) -> str:
    if score is None:
        return "fail"
    if score >= COMPLETENESS_PASS:
        return "pass"
    if score >= COMPLETENESS_PARTIAL:
        return "partial"
    return "fail"


# ---------------------------------------------------------------------------
# XFA datasets extraction
# ---------------------------------------------------------------------------

def _find_xfa_streams(pdf_bytes: bytes) -> list[bytes]:
    """Return a list of raw XFA stream payloads found in the PDF bytes."""
    streams: list[bytes] = []
    # XFA is stored as a XFA array in the catalog.
    # Each chunk is an inline stream object. We look for all stream...endstream blocks
    # that contain XML (start with <?xml or <xdp: or <datasets or similar).
    # Simple heuristic: find all stream bodies, filter by XML-looking content.
    pos = 0
    while True:
        start = pdf_bytes.find(b"stream", pos)
        if start == -1:
            break
        # skip past "stream" and the mandatory CRLF or LF
        stream_start = start + len(b"stream")
        if stream_start < len(pdf_bytes) and pdf_bytes[stream_start:stream_start + 1] == b"\r":
            stream_start += 1
        if stream_start < len(pdf_bytes) and pdf_bytes[stream_start:stream_start + 1] == b"\n":
            stream_start += 1
        end = pdf_bytes.find(b"endstream", stream_start)
        if end == -1:
            break
        chunk = pdf_bytes[stream_start:end]
        # Only keep chunks that look like XML
        stripped = chunk.lstrip()
        if stripped.startswith(b"<?xml") or stripped.startswith(b"<xdp:") or b"<datasets" in chunk[:200]:
            streams.append(chunk)
        pos = end + len(b"endstream")
    return streams


def extract_datasets_values(pdf_path: str) -> list[str]:
    """Extract leaf text values from XFA datasets XML in *pdf_path*.

    Returns a deduplicated list of non-trivial field values.
    """
    try:
        pdf_bytes = Path(pdf_path).read_bytes()
    except Exception:
        return []

    streams = _find_xfa_streams(pdf_bytes)
    values: list[str] = []

    for stream in streams:
        # Try to find the <datasets ...> section
        idx = stream.find(b"<datasets")
        if idx == -1:
            # Maybe this is the whole datasets packet
            try:
                root_text = stream.decode("utf-8", errors="replace")
                tree = ET.fromstring(root_text)
                _walk_tree(tree, values)
            except Exception:
                pass
            continue

        # Extract from <datasets ...> ... </datasets>
        end_tag = stream.find(b"</datasets>", idx)
        if end_tag != -1:
            fragment = stream[idx:end_tag + len(b"</datasets>")]
        else:
            fragment = stream[idx:]

        try:
            fragment_text = fragment.decode("utf-8", errors="replace")
            # Strip namespace declarations that may confuse ElementTree
            fragment_text = re.sub(r'\s+xmlns[^=]*="[^"]*"', "", fragment_text)
            tree = ET.fromstring(fragment_text)
            _walk_tree(tree, values)
        except Exception:
            pass

    # Deduplicate while preserving order
    seen: set[str] = set()
    result: list[str] = []
    for v in values:
        if v not in seen:
            seen.add(v)
            result.append(v)
    return result


def _walk_tree(elem: ET.Element, out: list[str]) -> None:
    """Walk all XML elements, collecting leaf text values."""
    if elem.text is not None and len(elem.text.strip()) > 2:
        val = elem.text.strip()
        if _is_meaningful_value(val):
            out.append(val)
    for child in elem:
        _walk_tree(child, out)
        if child.tail is not None and len(child.tail.strip()) > 2:
            tail = child.tail.strip()
            if _is_meaningful_value(tail):
                out.append(tail)


def _is_meaningful_value(val: str) -> bool:
    """Filter out trivial values: pure whitespace, too short, or pure short numbers."""
    if len(val) < 3:
        return False
    # Pure numeric and short (less than 4 digits) → skip
    if re.fullmatch(r"\d{1,3}", val):
        return False
    # Pure whitespace
    if not val.strip():
        return False
    return True


# ---------------------------------------------------------------------------
# PDF text extraction
# ---------------------------------------------------------------------------

def extract_pdf_text(pdf_path: str) -> Optional[str]:
    """Extract plain text from a PDF using mutool or pdftotext.

    Returns None if neither tool is available or extraction fails.
    """
    # Try mutool first
    try:
        r = subprocess.run(
            ["mutool", "draw", "-F", "text", pdf_path],
            capture_output=True,
            timeout=30,
        )
        if r.returncode == 0 and r.stdout:
            return r.stdout.decode("utf-8", errors="replace")
    except (FileNotFoundError, subprocess.TimeoutExpired, Exception):
        pass

    # Try pdftotext
    try:
        r = subprocess.run(
            ["pdftotext", pdf_path, "-"],
            capture_output=True,
            timeout=30,
        )
        if r.returncode == 0 and r.stdout:
            return r.stdout.decode("utf-8", errors="replace")
    except (FileNotFoundError, subprocess.TimeoutExpired, Exception):
        pass

    # Fallback: raw byte scan for ASCII printable runs (very rough)
    try:
        raw = Path(pdf_path).read_bytes()
        # Extract text-like runs from BT...ET PDF content stream blocks
        text_runs: list[str] = []
        for m in re.finditer(rb"\(([^\)]{2,200})\)", raw):
            try:
                text_runs.append(m.group(1).decode("latin-1", errors="replace"))
            except Exception:
                pass
        return " ".join(text_runs) if text_runs else None
    except Exception:
        return None


def _count_matches(values: list[str], text: str) -> tuple[int, list[str]]:
    """Return (match_count, missing_values) for case-insensitive substring check."""
    if not text:
        return 0, list(values)
    text_lower = text.lower()
    missing: list[str] = []
    count = 0
    for val in values:
        if val.strip().lower() in text_lower:
            count += 1
        else:
            missing.append(val)
    return count, missing


# ---------------------------------------------------------------------------
# Main comparison function
# ---------------------------------------------------------------------------

def check_completeness(
    original_pdf: str,
    our_output_pdf: str,
    reference_pdf: str,
) -> dict:
    """Check text completeness. Returns a JSON-serialisable dict."""
    values = extract_datasets_values(original_pdf)
    total = len(values)

    result: dict = {
        "datasets_values": values,
        "our_completeness": None,
        "ref_completeness": None,
        "completeness_delta": None,
        "missing_values": [],
        "data_complete": "fail",
    }

    if total == 0:
        # No XFA datasets found — treat as pass (non-XFA doc or no data)
        result["our_completeness"] = 1.0
        result["ref_completeness"] = 1.0
        result["completeness_delta"] = 0.0
        result["data_complete"] = "pass"
        result["note"] = "no_datasets_values_found"
        return result

    our_text = extract_pdf_text(our_output_pdf)
    ref_text = extract_pdf_text(reference_pdf)

    our_matches, missing = _count_matches(values, our_text or "")
    ref_matches, _ = _count_matches(values, ref_text or "")

    our_completeness = round(our_matches / total, 6)
    ref_completeness = round(ref_matches / total, 6)

    result["our_completeness"] = our_completeness
    result["ref_completeness"] = ref_completeness
    result["completeness_delta"] = round(our_completeness - ref_completeness, 6)
    result["missing_values"] = missing
    result["data_complete"] = _completeness_verdict(our_completeness)

    return result


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Check text completeness: XFA datasets values vs rendered PDF text"
    )
    p.add_argument("--original", required=True, help="Original XFA PDF (source of truth)")
    p.add_argument("--output", required=True, help="Our flattened output PDF")
    p.add_argument("--reference", required=True, help="pdfRest reference PDF")
    p.add_argument("--result", required=True, help="Path to write JSON result")
    args = p.parse_args()

    for label, path in [
        ("--original", args.original),
        ("--output", args.output),
        ("--reference", args.reference),
    ]:
        if not Path(path).exists():
            sys.exit(f"ERROR: {label} file not found: {path}")

    result = check_completeness(args.original, args.output, args.reference)

    out_path = Path(args.result)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2))
    print(
        f"text_completeness: our={result['our_completeness']}  "
        f"ref={result['ref_completeness']}  "
        f"verdict={result['data_complete']}"
    )
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
