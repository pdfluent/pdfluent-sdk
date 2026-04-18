#!/usr/bin/env python3
"""check_benchmark_sla.py — Compare a benchmark result JSON against SLA targets.

Usage:
    python3 scripts/check_benchmark_sla.py \\
        --suite-json benchmarks/results/hetzner-e2176g-2026-04-18.json \\
        --sla BENCHMARKS_SLA.md

Exits 0 if all targets pass, 1 if any fail.

SLA targets are hardcoded here (extracted from BENCHMARKS_SLA.md) to avoid
the complexity of parsing Markdown tables at runtime.
"""

import argparse
import json
import sys

# ---------------------------------------------------------------------------
# SLA targets — extracted from BENCHMARKS_SLA.md
# Keyed by the metric name used in the benchmark JSON "sla_results" block.
# ---------------------------------------------------------------------------
SLA_TARGETS = {
    "render_text_p95": {
        "label": "Render A4 text-only (p95)",
        "target_ms": 50,
        "notes": "150 DPI, RGBA",
    },
    "render_mixed_p95": {
        "label": "Render A4 mixed text+images (p95)",
        "target_ms": 150,
        "notes": "150 DPI, RGBA",
    },
    "render_image_heavy_p95": {
        "label": "Render A4 image-heavy (p95)",
        "target_ms": 300,
        "notes": "150 DPI, JPEG image content",
    },
    "text_extract_p95": {
        "label": "Text extraction 10-page doc (p95)",
        "target_ms": 20,
        "notes": "Structured text with positions",
    },
    "text_extract_100page_p95": {
        "label": "Text extraction 100-page doc (p95)",
        "target_ms": 200,
        "notes": "Structured text",
    },
    "xfa_flatten_simple_p95": {
        "label": "XFA flatten simple 1-page (p95)",
        "target_ms": 100,
        "notes": "Basic form, no scripts",
    },
    "xfa_flatten_complex_p95": {
        "label": "XFA flatten complex 10-page (p95)",
        "target_ms": 500,
        "notes": "Multi-page with data binding",
    },
}

# ANSI colour codes (auto-disabled if not a TTY)
_USE_COLOUR = sys.stdout.isatty()
GREEN = "\033[92m" if _USE_COLOUR else ""
RED = "\033[91m" if _USE_COLOUR else ""
YELLOW = "\033[93m" if _USE_COLOUR else ""
RESET = "\033[0m" if _USE_COLOUR else ""
BOLD = "\033[1m" if _USE_COLOUR else ""


def pct_over(actual: int, target: int) -> str:
    """Return a formatted '+N%' string for the overshoot, or empty string."""
    if target == 0:
        return ""
    over = ((actual - target) / target) * 100
    return f" +{over:.0f}%" if over > 0 else ""


def check(suite_json_path: str, sla_path: str) -> int:
    """Run the SLA check. Returns exit code (0=all pass, 1=any fail)."""

    # Load result JSON
    try:
        with open(suite_json_path) as fh:
            suite = json.load(fh)
    except FileNotFoundError:
        print(f"Error: benchmark JSON not found: {suite_json_path}", file=sys.stderr)
        return 2
    except json.JSONDecodeError as exc:
        print(f"Error: invalid JSON in {suite_json_path}: {exc}", file=sys.stderr)
        return 2

    sla_results: dict = suite.get("sla_results", {})
    categories: dict = suite.get("categories", {})
    date = suite.get("date", "unknown")
    hardware = suite.get("hardware", "unknown")

    print(f"\n{BOLD}SLA Check Results{RESET}")
    print("=================")
    print(f"  Date:     {date}")
    print(f"  Hardware: {hardware}")
    print(f"  File:     {suite_json_path}")
    if sla_path:
        print(f"  SLA doc:  {sla_path}")
    print()

    any_fail = False
    any_checked = False

    # Determine column widths for alignment
    label_width = max(len(v["label"]) for v in SLA_TARGETS.values()) + 2

    for key, meta in SLA_TARGETS.items():
        label = meta["label"]
        target_ms = meta["target_ms"]

        # Pull actual value: first from sla_results, then from categories heuristic
        if key in sla_results:
            entry = sla_results[key]
            actual_ms = entry.get("actual_ms", None)
            reported_pass = entry.get("pass", None)
        else:
            # Try to find in categories by key mapping
            cat_key = _categories_key(key)
            actual_ms = categories.get(cat_key, None)
            reported_pass = None

        if actual_ms is None:
            # Not measured — skip with a note
            icon = f"{YELLOW}--{RESET}"
            status = "NOT MEASURED"
            line = f"  {icon}  {label:<{label_width}}  {status}"
            print(line)
            continue

        any_checked = True
        passed = actual_ms <= target_ms

        if not passed:
            any_fail = True
            icon = f"{RED}FAIL{RESET}"
            overshoot = pct_over(actual_ms, target_ms)
            status = f"{RED}{actual_ms}ms < {target_ms}ms (FAIL{overshoot}){RESET}"
        else:
            icon = f"{GREEN}PASS{RESET}"
            status = f"{GREEN}{actual_ms}ms < {target_ms}ms (PASS){RESET}"

        line = f"  [{icon}]  {label:<{label_width}}  {status}"
        print(line)

    print()

    # Summary line
    if not any_checked:
        print(f"{YELLOW}No SLA results found in the benchmark JSON.{RESET}")
        print("Ensure run_benchmarks.sh wrote a 'sla_results' block.")
        return 1

    if any_fail:
        print(f"{RED}{BOLD}Result: FAIL — one or more SLA targets exceeded.{RESET}")
        return 1
    else:
        print(f"{GREEN}{BOLD}Result: ALL PASS{RESET}")
        return 0


def _categories_key(sla_key: str) -> str:
    """Map an SLA key to a categories dict key."""
    mapping = {
        "render_text_p95": "render_text_p95_ms",
        "render_mixed_p95": "render_mixed_p95_ms",
        "render_image_heavy_p95": "render_image_heavy_p95_ms",
        "text_extract_p95": "text_extract_p95_ms",
        "text_extract_100page_p95": "text_extract_100page_p95_ms",
        "xfa_flatten_simple_p95": "xfa_flatten_p95_ms",
        "xfa_flatten_complex_p95": "xfa_flatten_complex_p95_ms",
    }
    return mapping.get(sla_key, sla_key + "_ms")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compare benchmark JSON results against SLA targets from BENCHMARKS_SLA.md."
    )
    parser.add_argument(
        "--suite-json",
        required=True,
        metavar="PATH",
        help="Path to the benchmark result JSON file.",
    )
    parser.add_argument(
        "--sla",
        default="BENCHMARKS_SLA.md",
        metavar="PATH",
        help="Path to BENCHMARKS_SLA.md (used for display; targets are hardcoded).",
    )
    args = parser.parse_args()

    exit_code = check(args.suite_json, args.sla)
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
