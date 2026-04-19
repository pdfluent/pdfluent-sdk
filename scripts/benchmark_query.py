#!/usr/bin/env python3
"""EVH-C2-06: Query tool for benchmark result files.

Supports filtering, sorting, limiting, and multiple output formats.

Usage:
    python3 scripts/benchmark_query.py \
        --results benchmarks/xfa-benchmark-2026-04-19.json \
        --filter status=major_deviation \
        --filter ssim<0.90 \
        --sort ssim \
        --limit 20 \
        --format table

    python3 scripts/benchmark_query.py \
        --results benchmarks/xfa-benchmark-2026-04-19.json \
        --summary
"""
import argparse
import csv
import io
import json
import operator
import re
import sys
from pathlib import Path
from typing import Any, Optional

# ---------------------------------------------------------------------------
# Filter parsing
# ---------------------------------------------------------------------------

_NUMERIC_OPS = {
    "<":  operator.lt,
    "<=": operator.le,
    ">":  operator.gt,
    ">=": operator.ge,
    "=":  operator.eq,
    "!=": operator.ne,
}

_FILTER_RE = re.compile(
    r"^(?P<field>[a-zA-Z_][a-zA-Z0-9_.]*)"
    r"(?P<op><=|>=|!=|<|>|=)"
    r"(?P<value>.+)$"
)


def _parse_filter(expr: str) -> tuple[str, str, Any]:
    """Parse a filter expression like 'ssim<0.90' or 'status=crash'.

    Returns (field, op, value) where value is a float/int if numeric, else str.
    """
    m = _FILTER_RE.match(expr)
    if not m:
        sys.exit(f"ERROR: invalid filter expression: {expr!r}  "
                 f"(expected: field=value, field<value, etc.)")
    field = m.group("field")
    op = m.group("op")
    raw_value = m.group("value")

    # Try to parse as number
    try:
        value: Any = float(raw_value)
        if value == int(value):
            value = int(value)
    except ValueError:
        value = raw_value  # string

    return field, op, value


def _get_field(row: dict, field: str) -> Any:
    """Get a (possibly nested) field from a result dict using dot notation."""
    parts = field.split(".", 1)
    val = row.get(parts[0])
    if len(parts) == 2 and isinstance(val, dict):
        return val.get(parts[1])
    return val


def _apply_filter(row: dict, field: str, op: str, value: Any) -> bool:
    """Return True if the row passes the filter condition."""
    actual = _get_field(row, field)
    if actual is None:
        return False
    op_fn = _NUMERIC_OPS.get(op)
    if op_fn is None:
        return False
    try:
        return bool(op_fn(actual, value))
    except TypeError:
        return str(actual) == str(value)


# ---------------------------------------------------------------------------
# KPI scorecard
# ---------------------------------------------------------------------------

VALID_STATUSES = [
    "fully_correct", "minor_deviation", "major_deviation",
    "partial_render", "render_fail", "flatten_issue",
    "crash", "encrypted", "degenerate", "xfa_error", "oracle_fault",
]

KPI_TARGETS = {
    "fully_correct_pct":   (">=", 75.0),
    "minor_deviation_pct": ("<=", 15.0),
    "major_deviation_pct": ("<=",  5.0),
    "partial_render_pct":  ("<=",  3.0),
    "crash_pct":           ("<=",  2.0),
}


def _compute_summary(results: list[dict]) -> dict:
    total = len(results)
    if total == 0:
        return {"total": 0}

    counts: dict[str, int] = {s: 0 for s in VALID_STATUSES}
    ssim_vals: list[float] = []
    completeness_vals: list[float] = []

    for r in results:
        status = r.get("status", "crash")
        if status in counts:
            counts[status] += 1
        ssim = r.get("ssim")
        if isinstance(ssim, (int, float)):
            ssim_vals.append(float(ssim))
        comp = r.get("completeness")
        if isinstance(comp, (int, float)):
            completeness_vals.append(float(comp))

    mean_ssim = round(sum(ssim_vals) / len(ssim_vals), 4) if ssim_vals else None
    mean_completeness = (
        round(sum(completeness_vals) / len(completeness_vals), 4)
        if completeness_vals else None
    )

    summary = {"total": total}
    for status in VALID_STATUSES:
        summary[status] = counts[status]
        summary[f"{status}_pct"] = round(100.0 * counts[status] / total, 2)

    summary["mean_ssim"] = mean_ssim
    summary["mean_completeness"] = mean_completeness
    return summary


def _print_scorecard(results: list[dict], config: dict) -> None:
    s = _compute_summary(results)
    total = s["total"]
    print(f"\n{'='*60}")
    print(f"BENCHMARK KPI SCORECARD   (total={total})")
    print(f"{'='*60}")
    print(f"{'KPI':<30} {'Target':>10} {'Actual':>10} {'Status':>8}")
    print(f"{'-'*60}")

    kpi_rows = [
        ("fully_correct_pct",   "≥ 75%"),
        ("minor_deviation_pct", "≤ 15%"),
        ("major_deviation_pct", "≤  5%"),
        ("partial_render_pct",  "≤  3%"),
        ("crash_pct",           "≤  2%"),
    ]

    for key, target_str in kpi_rows:
        actual = s.get(key, 0.0)
        op_str, threshold = KPI_TARGETS[key]
        if op_str == ">=":
            ok = actual >= threshold
        else:
            ok = actual <= threshold
        badge = "PASS" if ok else "FAIL"
        print(f"  {key:<28} {target_str:>10} {actual:>9.2f}% {badge:>8}")

    print(f"{'-'*60}")
    print(f"  {'mean_ssim':<28} {'≥ 0.970':>10} "
          f"{str(s.get('mean_ssim', 'n/a')):>10}")
    print(f"\nStatus distribution:")
    for status in VALID_STATUSES:
        count = s.get(status, 0)
        pct = s.get(f"{status}_pct", 0.0)
        bar = "#" * int(pct / 2)
        print(f"  {status:<22} {count:>5}  {pct:>5.1f}%  {bar}")
    print()


# ---------------------------------------------------------------------------
# Output formatters
# ---------------------------------------------------------------------------

_TABLE_COLS = ["file", "status", "ssim", "completeness", "our_pages", "ref_pages",
               "flatten_valid", "timing_ms"]


def _format_table(results: list[dict]) -> str:
    if not results:
        return "(no results)\n"
    col_widths = {c: len(c) for c in _TABLE_COLS}
    formatted_rows: list[dict[str, str]] = []
    for r in results:
        row_str: dict[str, str] = {}
        for col in _TABLE_COLS:
            val = _get_field(r, col)
            if isinstance(val, float):
                s = f"{val:.4f}"
            elif val is None:
                s = "-"
            else:
                s = str(val)
            row_str[col] = s
            col_widths[col] = max(col_widths[col], len(s))
        formatted_rows.append(row_str)

    sep = "  ".join("-" * col_widths[c] for c in _TABLE_COLS)
    header = "  ".join(c.ljust(col_widths[c]) for c in _TABLE_COLS)

    lines = [header, sep]
    for row_str in formatted_rows:
        lines.append("  ".join(row_str[c].ljust(col_widths[c]) for c in _TABLE_COLS))

    return "\n".join(lines) + "\n"


def _format_csv(results: list[dict]) -> str:
    if not results:
        return ""
    buf = io.StringIO()
    writer = csv.DictWriter(
        buf,
        fieldnames=_TABLE_COLS,
        extrasaction="ignore",
        lineterminator="\n",
    )
    writer.writeheader()
    for r in results:
        row = {}
        for col in _TABLE_COLS:
            val = _get_field(r, col)
            row[col] = "" if val is None else str(val)
        writer.writerow(row)
    return buf.getvalue()


# ---------------------------------------------------------------------------
# Load results file
# ---------------------------------------------------------------------------

def load_results_file(path: str) -> tuple[list[dict], dict, dict]:
    """Load results JSON. Returns (results, config, summary)."""
    try:
        data = json.loads(Path(path).read_text())
    except Exception as exc:
        sys.exit(f"ERROR: could not load results file {path}: {exc}")

    if isinstance(data, list):
        # Bare list of result objects
        return data, {}, {}

    results = data.get("results", [])
    config = data.get("config", {})
    summary = data.get("summary", {})
    return results, config, summary


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Query and filter XFA benchmark result files"
    )
    p.add_argument("--results", required=True,
                   help="Path to benchmark result JSON file")
    p.add_argument("--filter", action="append", dest="filters", default=[],
                   metavar="EXPR",
                   help="Filter expression: field=value, field<N, field>=N, ... (repeatable)")
    p.add_argument("--sort", default=None,
                   help="Sort by field (prefix with '-' for descending, e.g. -ssim)")
    p.add_argument("--limit", type=int, default=None,
                   help="Show at most N results")
    p.add_argument("--format", choices=["table", "json", "csv"], default="table",
                   help="Output format (default: table)")
    p.add_argument("--summary", action="store_true",
                   help="Print KPI scorecard instead of individual results")
    args = p.parse_args()

    if not Path(args.results).exists():
        sys.exit(f"ERROR: results file not found: {args.results}")

    results, config, stored_summary = load_results_file(args.results)

    # ----------------------------------------------------------------
    # Summary mode
    # ----------------------------------------------------------------
    if args.summary:
        _print_scorecard(results, config)
        return

    # ----------------------------------------------------------------
    # Parse and apply filters
    # ----------------------------------------------------------------
    parsed_filters = [_parse_filter(f) for f in args.filters]

    filtered = results
    for field, op, value in parsed_filters:
        filtered = [r for r in filtered if _apply_filter(r, field, op, value)]

    # ----------------------------------------------------------------
    # Sort
    # ----------------------------------------------------------------
    if args.sort:
        sort_field = args.sort
        reverse = False
        if sort_field.startswith("-"):
            sort_field = sort_field[1:]
            reverse = True

        def _sort_key(r: dict) -> tuple:
            val = _get_field(r, sort_field)
            # None values sort last
            if val is None:
                return (1, 0, "")
            if isinstance(val, (int, float)):
                return (0, float(val), "")
            return (0, 0, str(val))

        filtered.sort(key=_sort_key, reverse=reverse)

    # ----------------------------------------------------------------
    # Limit
    # ----------------------------------------------------------------
    if args.limit is not None:
        filtered = filtered[:args.limit]

    # ----------------------------------------------------------------
    # Output
    # ----------------------------------------------------------------
    print(f"# {len(filtered)} result(s) "
          f"(of {len(results)} total) — {args.results}")

    if args.format == "json":
        print(json.dumps(filtered, indent=2))
    elif args.format == "csv":
        print(_format_csv(filtered), end="")
    else:
        print(_format_table(filtered), end="")


if __name__ == "__main__":
    main()
