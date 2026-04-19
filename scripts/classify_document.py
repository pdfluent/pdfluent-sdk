#!/usr/bin/env python3
"""EVH-C2-04: Composite document quality classifier.

Implements the 4-dimension composite correctness model and maps to one of
10 defined status values.

Importable as a module:
    from classify_document import classify_document, COMPOSITE_THRESHOLDS

CLI usage:
    python3 scripts/classify_document.py \
        --visual-result      /tmp/visual.json \
        --completeness-result /tmp/completeness.json \
        --structural-result  /tmp/structural.json \
        --flatten-result     /tmp/flatten.json \
        --exit-code 0 \
        --output /tmp/classification.json
"""
import argparse
import json
import sys
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Thresholds (exposed so the benchmark runner can print them)
# ---------------------------------------------------------------------------

COMPOSITE_THRESHOLDS: dict = {
    "visual_acceptable": {
        "pass":    "ssim >= 0.94",
        "partial": "0.85 <= ssim < 0.94",
        "fail":    "ssim < 0.85",
    },
    "data_complete": {
        "pass":    "completeness >= 0.90",
        "partial": "0.50 <= completeness < 0.90",
        "fail":    "completeness < 0.50",
    },
    "structurally_equivalent": {
        "pass":    "page_count_delta == 0, no empty-page discrepancy",
        "partial": "abs(page_count_delta) <= 1  OR  <= 1 empty page discrepancy",
        "fail":    "abs(page_count_delta) >= 2  OR  significant empty mismatch",
    },
    "flatten_clean": {
        "pass": "no /XFA, /NeedsRendering, /Widget annotations",
        "fail": "any XFA artifact present",
    },
}

# ---------------------------------------------------------------------------
# Per-dimension scorers
# ---------------------------------------------------------------------------

SSIM_PASS = 0.94
SSIM_PARTIAL = 0.85
COMPLETENESS_PASS = 0.90
COMPLETENESS_PARTIAL = 0.50


def _score_visual(ssim: Optional[float]) -> str:
    if ssim is None:
        return "fail"
    if ssim >= SSIM_PASS:
        return "pass"
    if ssim >= SSIM_PARTIAL:
        return "partial"
    return "fail"


def _score_data(completeness: Optional[float]) -> str:
    if completeness is None:
        return "fail"
    if completeness >= COMPLETENESS_PASS:
        return "pass"
    if completeness >= COMPLETENESS_PARTIAL:
        return "partial"
    return "fail"


def _score_structural(our_pages: Optional[int], ref_pages: Optional[int]) -> str:
    if our_pages is None or ref_pages is None:
        return "partial"  # unknown → conservative
    delta = abs(our_pages - ref_pages)
    if delta == 0:
        return "pass"
    if delta <= 1:
        return "partial"
    return "fail"


def _score_flatten(flatten_clean: Optional[bool]) -> str:
    if flatten_clean is None:
        return "fail"
    return "pass" if flatten_clean else "fail"


# ---------------------------------------------------------------------------
# Composite status logic
# ---------------------------------------------------------------------------

def _composite_status(
    exit_code: int,
    visual: str,
    data: str,
    structural: str,
    flatten: str,
    oracle_failed: bool,
) -> tuple[str, str]:
    """Return (status, primary_reason)."""

    # Special exit codes take priority
    if exit_code == 1:
        return "crash", "exit_code=1"
    if exit_code == 2:
        return "encrypted", "exit_code=2"
    if exit_code == 3:
        return "degenerate", "exit_code=3"
    if exit_code == 4:
        return "xfa_error", "exit_code=4"

    # Oracle failed
    if oracle_failed:
        return "oracle_fault", "oracle_fault=True"

    # flatten_issue overrides everything else (regardless of other dims)
    if flatten == "fail":
        return "flatten_issue", "flatten_clean=fail"

    # render_fail — visual completely broken
    if visual == "fail":
        return "render_fail", f"visual_acceptable=fail"

    # partial_render — critical data or structural failure
    if data == "fail" or structural == "fail":
        reason = []
        if data == "fail":
            reason.append("data_complete=fail")
        if structural == "fail":
            reason.append("structurally_equivalent=fail")
        return "partial_render", "; ".join(reason)

    # major_deviation — partial data or structural
    if data == "partial" or structural == "partial":
        reason = []
        if data == "partial":
            reason.append("data_complete=partial")
        if structural == "partial":
            reason.append("structurally_equivalent=partial")
        return "major_deviation", "; ".join(reason)

    # minor_deviation — only visual is partial, everything else pass
    if visual == "partial":
        return "minor_deviation", "visual_acceptable=partial"

    # All pass
    return "fully_correct", "all dimensions pass"


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def classify_document(
    exit_code: int,
    ssim: Optional[float],
    completeness: Optional[float],
    our_pages: Optional[int],
    ref_pages: Optional[int],
    flatten_clean: Optional[bool],
    oracle_failed: bool = False,
) -> dict:
    """Compute composite classification from raw dimension values.

    Returns a JSON-serialisable dict with keys:
        status, exit_code, dimensions, primary_reason,
        ssim, completeness, our_pages, ref_pages
    """
    visual = _score_visual(ssim)
    data = _score_data(completeness)
    structural = _score_structural(our_pages, ref_pages)
    flatten = _score_flatten(flatten_clean)

    status, primary_reason = _composite_status(
        exit_code, visual, data, structural, flatten, oracle_failed
    )

    return {
        "status": status,
        "exit_code": exit_code,
        "dimensions": {
            "visual_acceptable": visual,
            "data_complete": data,
            "structurally_equivalent": structural,
            "flatten_clean": flatten,
        },
        "primary_reason": primary_reason,
        "ssim": ssim,
        "completeness": completeness,
        "our_pages": our_pages,
        "ref_pages": ref_pages,
    }


# ---------------------------------------------------------------------------
# CLI helper: load result files
# ---------------------------------------------------------------------------

def _load_json(path: str) -> dict:
    try:
        return json.loads(Path(path).read_text())
    except Exception as exc:
        print(f"WARNING: could not load {path}: {exc}", file=sys.stderr)
        return {}


def _extract_from_files(
    visual_file: Optional[str],
    completeness_file: Optional[str],
    structural_file: Optional[str],
    flatten_file: Optional[str],
    exit_code: int,
) -> dict:
    """Extract dimension values from result JSON files."""
    visual_data = _load_json(visual_file) if visual_file else {}
    comp_data = _load_json(completeness_file) if completeness_file else {}
    struct_data = _load_json(structural_file) if structural_file else {}
    flatten_data = _load_json(flatten_file) if flatten_file else {}

    ssim = visual_data.get("ssim")
    completeness = comp_data.get("our_completeness")
    our_pages = struct_data.get("our_pages")
    ref_pages = struct_data.get("ref_pages")

    # flatten_clean: True if verdict is "pass", False if "fail", None if missing
    flatten_verdict = flatten_data.get("flatten_clean")
    if flatten_verdict == "pass":
        flatten_clean: Optional[bool] = True
    elif flatten_verdict == "fail":
        flatten_clean = False
    else:
        flatten_clean = None

    oracle_failed = (
        visual_data.get("oracle_fault", False)
        or comp_data.get("oracle_fault", False)
    )

    return classify_document(
        exit_code=exit_code,
        ssim=ssim,
        completeness=completeness,
        our_pages=our_pages,
        ref_pages=ref_pages,
        flatten_clean=flatten_clean,
        oracle_failed=oracle_failed,
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Composite 4-dimension document quality classifier"
    )
    p.add_argument("--visual-result", help="Path to visual_compare JSON result")
    p.add_argument("--completeness-result", help="Path to text_completeness JSON result")
    p.add_argument("--structural-result", help="Path to structural_compare JSON result")
    p.add_argument("--flatten-result", help="Path to validate_flatten_output JSON result")
    p.add_argument("--exit-code", type=int, default=0,
                   help="Exit code from the render binary (0=success, 1=crash, ...)")
    p.add_argument("--output", required=True, help="Path to write JSON classification")
    p.add_argument("--print-thresholds", action="store_true",
                   help="Print COMPOSITE_THRESHOLDS and exit")
    args = p.parse_args()

    if args.print_thresholds:
        print(json.dumps(COMPOSITE_THRESHOLDS, indent=2))
        sys.exit(0)

    result = _extract_from_files(
        visual_file=args.visual_result,
        completeness_file=args.completeness_result,
        structural_file=args.structural_result,
        flatten_file=args.flatten_result,
        exit_code=args.exit_code,
    )

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2))
    print(
        f"classify_document: status={result['status']}  "
        f"reason={result['primary_reason']}"
    )
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
