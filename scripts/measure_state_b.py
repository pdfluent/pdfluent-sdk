#!/usr/bin/env python3
"""Measure State B: structural validator on current (unfixed) binary output.

Run this after flattening the 140 baseline corpus documents to /tmp/state_b_outputs/.
The script inspects structure only: reachable /AcroForm -> /XFA and reachable page
/Annots -> /Widget. Raw orphaned bytes are not treated as structural failures here.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    import pikepdf

    HAS_PIKEPDF = True
except ImportError:
    pikepdf = None
    HAS_PIKEPDF = False

try:
    from pypdf import PdfReader

    HAS_PYPDF = True
except ImportError:
    PdfReader = None
    HAS_PYPDF = False


DEFAULT_OUTPUT_DIR = Path("/tmp/state_b_outputs")
DEFAULT_RESULTS_PATH = Path("benchmarks/state_b_results.json")
STATE_A_FLATTEN_ISSUE_PCT = 96.4


def has_xfa_structural(pdf_path: Path) -> bool | None:
    """True if /XFA is reachable from the document catalog."""
    if HAS_PIKEPDF:
        try:
            with pikepdf.open(str(pdf_path)) as pdf:
                acroform = pdf.Root.get("/AcroForm")
                if acroform is None:
                    return False
                return acroform.get("/XFA") is not None
        except Exception:
            return None

    if HAS_PYPDF:
        try:
            reader = PdfReader(str(pdf_path))
            root = reader.trailer["/Root"]
            acroform = root.get("/AcroForm")
            if acroform is None:
                return False
            if hasattr(acroform, "get_object"):
                acroform = acroform.get_object()
            return acroform.get("/XFA") is not None
        except Exception:
            return None

    return None


def has_widget_structural(pdf_path: Path) -> bool | None:
    """True if any /Widget annotation is reachable from any page."""
    if HAS_PIKEPDF:
        try:
            with pikepdf.open(str(pdf_path)) as pdf:
                for page in pdf.pages:
                    annots = page.get("/Annots")
                    if annots is None:
                        continue
                    try:
                        for annot in annots:
                            try:
                                if str(annot.get("/Subtype", "")) == "/Widget":
                                    return True
                            except Exception:
                                pass
                    except Exception:
                        pass
                return False
        except Exception:
            return None

    if HAS_PYPDF:
        try:
            reader = PdfReader(str(pdf_path))
            for page in reader.pages:
                annots = page.get("/Annots")
                if annots is None:
                    continue
                for annot_ref in annots:
                    try:
                        annot = annot_ref.get_object()
                        if str(annot.get("/Subtype", "")) == "/Widget":
                            return True
                    except Exception:
                        pass
            return False
        except Exception:
            return None

    return None


def measure_state_b(output_dir: Path, results_path: Path) -> dict:
    if not output_dir.exists():
        raise FileNotFoundError(f"output directory not found: {output_dir}")

    results = []
    for pdf_path in sorted(output_dir.glob("*.pdf")):
        xfa = has_xfa_structural(pdf_path)
        widget = has_widget_structural(pdf_path)
        structural_fail = None if xfa is None or widget is None else bool(xfa or widget)
        results.append(
            {
                "file": pdf_path.name,
                "xfa_structural": xfa,
                "widget_structural": widget,
                "structural_fail": structural_fail,
            }
        )

    total = len(results)
    parsed = sum(1 for r in results if r["structural_fail"] is not None)
    structural_fails = sum(1 for r in results if r["structural_fail"] is True)
    parse_errors = total - parsed

    results_path.parent.mkdir(parents=True, exist_ok=True)
    results_path.write_text(json.dumps(results, indent=2) + "\n")

    fail_pct = (100.0 * structural_fails / total) if total else 0.0
    delta_pp = STATE_A_FLATTEN_ISSUE_PCT - fail_pct

    return {
        "total": total,
        "parsed": parsed,
        "parse_errors": parse_errors,
        "structural_fails": structural_fails,
        "fail_pct": fail_pct,
        "delta_pp": delta_pp,
        "results_path": results_path,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Measure State B: structural flatten failure rate on current binary output"
    )
    parser.add_argument(
        "--output-dir",
        default=str(DEFAULT_OUTPUT_DIR),
        help=f"Directory containing flattened PDFs (default: {DEFAULT_OUTPUT_DIR})",
    )
    parser.add_argument(
        "--results-json",
        default=str(DEFAULT_RESULTS_PATH),
        help=f"Where to write per-file State B results (default: {DEFAULT_RESULTS_PATH})",
    )
    args = parser.parse_args()

    if not HAS_PIKEPDF:
        if HAS_PYPDF:
            print("WARNING: pikepdf not available, falling back to pypdf structural parsing")
        else:
            print("WARNING: neither pikepdf nor pypdf available; structural checks will return null")

    summary = measure_state_b(Path(args.output_dir), Path(args.results_json))

    print(
        f"State B: {summary['structural_fails']}/{summary['total']} = "
        f"{summary['fail_pct']:.1f}% structural fail"
    )
    print(f"A→B delta: {summary['delta_pp']:.1f} pp improvement from validator fix alone")
    if summary["parse_errors"]:
        print(f"Parse errors: {summary['parse_errors']} (files recorded with null structural status)")
    print(f"Wrote {summary['results_path']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
