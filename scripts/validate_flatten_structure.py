#!/usr/bin/env python3
"""Authoritative structural validator for flattened PDFs.

Level semantics:
- L1: structural checks (R-01 through R-04) + content presence (R-07/R-08)
- L2: adds byte-level orphan markers (R-05/R-06), informational only
- L3: reserved for deeper content/resource checks; currently a stub
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

try:
    import pikepdf

    HAS_PIKEPDF = True
except ImportError:
    pikepdf = None
    HAS_PIKEPDF = False

try:
    from pypdf import PdfReader, PdfWriter

    HAS_PYPDF = True
except ImportError:
    PdfReader = None
    PdfWriter = None
    HAS_PYPDF = False


def _is_effective_acroform(obj: Any) -> bool:
    if obj is None:
        return False
    try:
        if str(obj).strip() == "null":
            return False
    except Exception:
        pass
    try:
        return len(obj.keys()) > 0
    except Exception:
        return True


def _read_pikepdf_bytes(obj: Any) -> bytes:
    if obj is None:
        return b""
    if isinstance(obj, pikepdf.Array):
        return b"".join(_read_pikepdf_bytes(item) for item in obj)
    try:
        return obj.read_bytes()
    except Exception:
        return b""


def _read_pypdf_bytes(obj: Any) -> bytes:
    if obj is None:
        return b""
    try:
        obj = obj.get_object()
    except Exception:
        pass
    if isinstance(obj, list):
        return b"".join(_read_pypdf_bytes(item) for item in obj)
    get_data = getattr(obj, "get_data", None)
    if callable(get_data):
        try:
            return get_data()
        except Exception:
            return b""
    return b""


def _inspect_with_pikepdf(pdf_path: Path) -> dict[str, Any]:
    with pikepdf.open(str(pdf_path)) as pdf:
        root = pdf.Root
        acroform = root.get("/AcroForm")
        has_acroform = _is_effective_acroform(acroform)
        has_xfa = root.get("/XFA") is not None
        if has_acroform and not has_xfa:
            try:
                has_xfa = acroform.get("/XFA") is not None
            except Exception:
                has_xfa = False
        has_needs_rendering = root.get("/NeedsRendering") is not None

        has_widget = False
        page_count = len(pdf.pages)
        all_pages_have_content = page_count > 0
        for page in pdf.pages:
            annots = page.get("/Annots")
            if annots is not None:
                try:
                    for annot in annots:
                        try:
                            if str(annot.get("/Subtype", "")) == "/Widget":
                                has_widget = True
                                break
                        except Exception:
                            pass
                except Exception:
                    pass
            if has_widget:
                # Continue checking content so has_content stays accurate.
                pass

            contents = page.obj.get("/Contents")
            content_bytes = _read_pikepdf_bytes(contents)
            if not content_bytes.strip():
                all_pages_have_content = False

        return {
            "has_xfa": has_xfa,
            "has_acroform": has_acroform,
            "has_needs_rendering": has_needs_rendering,
            "has_widget": has_widget,
            "page_count": page_count,
            "has_content": all_pages_have_content,
            "backend": "pikepdf",
        }


def _inspect_with_pypdf(pdf_path: Path) -> dict[str, Any]:
    reader = PdfReader(str(pdf_path))
    root = reader.trailer["/Root"]
    acroform = root.get("/AcroForm")
    if hasattr(acroform, "get_object"):
        try:
            acroform = acroform.get_object()
        except Exception:
            pass
    has_acroform = _is_effective_acroform(acroform)
    has_xfa = root.get("/XFA") is not None
    if has_acroform and not has_xfa:
        try:
            has_xfa = acroform.get("/XFA") is not None
        except Exception:
            has_xfa = False
    has_needs_rendering = root.get("/NeedsRendering") is not None

    has_widget = False
    page_count = len(reader.pages)
    all_pages_have_content = page_count > 0
    for page in reader.pages:
        annots = page.get("/Annots")
        if annots is not None:
            for annot_ref in annots:
                try:
                    annot = annot_ref.get_object()
                    if str(annot.get("/Subtype", "")) == "/Widget":
                        has_widget = True
                        break
                except Exception:
                    pass
        contents = page.get("/Contents")
        content_bytes = _read_pypdf_bytes(contents)
        if not content_bytes.strip():
            all_pages_have_content = False

    return {
        "has_xfa": has_xfa,
        "has_acroform": has_acroform,
        "has_needs_rendering": has_needs_rendering,
        "has_widget": has_widget,
        "page_count": page_count,
        "has_content": all_pages_have_content,
        "backend": "pypdf",
    }


def _inspect_structure(pdf_path: Path) -> dict[str, Any]:
    if HAS_PIKEPDF:
        return _inspect_with_pikepdf(pdf_path)
    if HAS_PYPDF:
        return _inspect_with_pypdf(pdf_path)
    raise RuntimeError("no structural PDF parser available (install pikepdf or pypdf)")


def _bytes_have_xfa(pdf_bytes: bytes) -> bool:
    return b"/XFA" in pdf_bytes


def _bytes_have_widget(pdf_bytes: bytes) -> bool:
    return b"/Widget" in pdf_bytes or bool(re.search(rb"/Subtype\s*/Widget", pdf_bytes))


def validate_structure(pdf_path: str, level: int = 1) -> dict:
    if level not in (1, 2, 3):
        raise ValueError("level must be 1, 2, or 3")

    path = Path(pdf_path)
    issues: list[str] = []

    if not path.exists():
        return {
            "level": level,
            "pass": False,
            "catalog_clean": False,
            "annots_clean": False,
            "bytes_clean": False,
            "has_content": False,
            "issues": [f"file not found: {path}"],
            "page_count": 0,
        }

    try:
        pdf_bytes = path.read_bytes()
    except Exception as exc:
        return {
            "level": level,
            "pass": False,
            "catalog_clean": False,
            "annots_clean": False,
            "bytes_clean": False,
            "has_content": False,
            "issues": [f"could not read file: {exc}"],
            "page_count": 0,
        }

    try:
        structural = _inspect_structure(path)
    except Exception as exc:
        return {
            "level": level,
            "pass": False,
            "catalog_clean": False,
            "annots_clean": False,
            "bytes_clean": False,
            "has_content": False,
            "issues": [f"structural parse failed: {exc}"],
            "page_count": 0,
        }

    if structural["has_xfa"]:
        issues.append("R-01: /XFA is still reachable from the document root")
    if structural["has_acroform"]:
        issues.append("R-02: /AcroForm is still reachable from the document catalog")
    if structural["has_needs_rendering"]:
        issues.append("R-03: /NeedsRendering is still present in the catalog")
    if structural["has_widget"]:
        issues.append("R-04: reachable /Widget annotations remain on one or more pages")
    if not structural["has_content"]:
        issues.append("R-07/R-08: page tree is empty or one or more pages lack non-empty /Contents")

    has_xfa_bytes = _bytes_have_xfa(pdf_bytes)
    has_widget_bytes = _bytes_have_widget(pdf_bytes)
    bytes_clean = not (has_xfa_bytes or has_widget_bytes)
    if level >= 2:
        if has_xfa_bytes:
            issues.append("R-05 informational: raw /XFA bytes still present in serialized output")
        if has_widget_bytes:
            issues.append("R-06 informational: raw /Widget bytes still present in serialized output")

    if level >= 3:
        issues.append("L3 stub: R-09 font/resource validation is not implemented yet")

    catalog_clean = not (
        structural["has_xfa"]
        or structural["has_acroform"]
        or structural["has_needs_rendering"]
    )
    annots_clean = not structural["has_widget"]
    is_pass = catalog_clean and annots_clean and structural["has_content"]

    return {
        "level": level,
        "pass": is_pass,
        "catalog_clean": catalog_clean,
        "annots_clean": annots_clean,
        "bytes_clean": bytes_clean,
        "has_content": structural["has_content"],
        "issues": issues,
        "page_count": structural["page_count"],
    }


def _run_self_test() -> None:
    if not HAS_PYPDF:
        raise RuntimeError("self-test requires pypdf to create a sample PDF")

    with tempfile.NamedTemporaryFile(suffix=".pdf") as tmp:
        writer = PdfWriter()
        writer.add_blank_page(width=72, height=72)
        with open(tmp.name, "wb") as f:
            writer.write(f)

        result = validate_structure(tmp.name, level=1)
        assert result["page_count"] == 1
        assert result["catalog_clean"] is True
        assert result["annots_clean"] is True
        assert result["has_content"] is False
        assert result["pass"] is False


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate flattened PDF structure")
    parser.add_argument("--pdf", help="Path to the flattened PDF to validate")
    parser.add_argument("--level", type=int, default=1, help="Validation level (1, 2, or 3)")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run a small built-in smoke test instead of validating a file",
    )
    args = parser.parse_args()

    if args.self_test:
        _run_self_test()
        print("self-test: ok")
        return 0

    if not args.pdf:
        parser.error("--pdf is required unless --self-test is used")

    result = validate_structure(args.pdf, level=args.level)
    print(json.dumps(result, indent=2))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
