#!/usr/bin/env python3
"""check_benchmark_sla.py — Compare a benchmark result JSON against SLA targets.

Usage:
    python3 scripts/check_benchmark_sla.py \\
        --suite-json benchmarks/results/<machine-class>-<date>.json \\
        --sla BENCHMARKS_SLA.md

Exit codes:
    0  every measured target passes
    1  a target failed
    2  the result file is missing or unreadable
    3  the result cannot be compared -- see below

WHY 3 EXISTS

The targets below were measured in April 2026 on a machine that was
decommissioned that August. This script used to print the result's `hardware`
field and then compare anyway, which made the label decoration: a regression
measured on faster hardware still cleared a threshold set on slower hardware
and reported a pass (#283).

So the machine now gates the comparison. A result is comparable only when it
names a machine class, that class is in benchmarks/BASELINE_HARDWARE.toml, the
class is calibrated, the calibration has not expired, and the cores the run saw
match what the class declares. Anything else exits 3 with
`SKIPPED (not a pass): <reason>` on stderr. There is no path through this
script that compares against a baseline from other hardware in silence.

SLA targets are hardcoded here (extracted from BENCHMARKS_SLA.md) to avoid
the complexity of parsing Markdown tables at runtime.
"""

import argparse
import datetime as dt
import json
import pathlib
import sys
import tomllib

REGISTRY = pathlib.Path(__file__).resolve().parent.parent / "benchmarks" / "BASELINE_HARDWARE.toml"

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


def uncomparable(suite: dict, today: dt.date) -> str | None:
    """Why this result cannot be judged, or None if it can.

    Every branch returns a sentence a reader can act on. None of them may be
    reachable by accident: the default is refusal, and comparison is the
    special case that has to earn its way through.
    """
    if not REGISTRY.is_file():
        return (f"{REGISTRY.name} is missing, so nothing can say which machine "
                f"these targets belong to")

    try:
        registry = tomllib.loads(REGISTRY.read_text())
    except (OSError, tomllib.TOMLDecodeError) as exc:
        return f"{REGISTRY.name} cannot be read: {exc}"

    klass = suite.get("machine_class")
    if not klass:
        return ("the result names no machine class. It was written before the "
                "machine became part of the result (#283), so what it measured "
                "is unknown and it cannot be compared to anything")

    entry = registry.get("classes", {}).get(klass)
    if entry is None:
        return (f"machine class {klass!r} has no entry in {REGISTRY.name}; "
                f"nobody has written down what it measures")

    if not entry.get("calibrated", False):
        return (f"machine class {klass!r} is not calibrated: {REGISTRY.name} "
                f"carries no measured baseline for it, and the SLA targets were "
                f"set on hardware that no longer exists")

    stamp = str(entry.get("calibrated_on", "")).strip()
    if not stamp:
        return (f"machine class {klass!r} claims a calibration with no date, so "
                f"there is no way to tell whether it still holds")
    try:
        when = dt.date.fromisoformat(stamp)
    except ValueError:
        return f"machine class {klass!r} has an unreadable calibrated_on ({stamp!r})"

    valid_days = int(registry.get("meta", {}).get("calibration_valid_days", 0))
    age = (today - when).days
    if valid_days and age > valid_days:
        return (f"the calibration for {klass!r} is {age} days old and expires "
                f"after {valid_days}; re-measure before comparing against it")

    declared = entry.get("cores")
    observed = suite.get("machine_cores_observed")
    if declared is not None and observed is not None and int(declared) != int(observed):
        return (f"machine class {klass!r} declares {declared} cores; the run saw "
                f"{observed}. The baseline does not describe the machine that "
                f"produced this result")

    return None


def check(suite_json_path: str, sla_path: str) -> int:
    """Run the SLA check. Returns 0 all pass, 1 any fail, 2 bad input, 3 no baseline."""

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

    # The machine gates the comparison. Refuse before printing anything that
    # looks like a verdict -- a table of PASS rows above a warning is read as a
    # pass.
    reason = uncomparable(suite, dt.date.today())
    if reason is not None:
        print(f"SKIPPED (not a pass): {reason}", file=sys.stderr)
        print(f"SKIPPED (not a pass): no SLA verdict was reached for "
              f"{suite_json_path}", file=sys.stderr)
        return 3

    sla_results: dict = suite.get("sla_results", {})
    categories: dict = suite.get("categories", {})
    date = suite.get("date", "unknown")
    machine = suite.get("machine_class", "unknown")

    print(f"\n{BOLD}SLA Check Results{RESET}")
    print("=================")
    print(f"  Date:     {date}")
    print(f"  Machine:  {machine}")
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
