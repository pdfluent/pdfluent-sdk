#!/usr/bin/env python3
"""run_core_sdk_perf_baseline.py — non-XFA Rust core SDK performance baseline.

Drives the existing criterion facade benchmark
(`crates/pdfluent/benches/facade.rs`, 8 common `pdfluent::PdfDocument`
operations on the shipped fixtures), then derives real per-iteration
percentiles (p50/p95/p99/min/max) from criterion's raw sample data and
writes a single JSON baseline/run file with explicit environment metadata.

Design goals (GA):
- deterministic, committed fixtures only (no network, no private paths);
- warmup + measured iterations (criterion-managed);
- median/p95/p99 + min/max per operation;
- explicit machine/environment metadata;
- per-benchmark timeout;
- exit nonzero ONLY on a real benchmark failure (build/run/parse), never
  on an advisory budget miss (budgets are enforced by
  check_performance_budgets.py, not here).

Usage:
    python3 scripts/perf/run_core_sdk_perf_baseline.py \
        --out benchmarks/runs/ga_readiness_3d/sdk_ga_performance_baseline/core_perf_run.json \
        [--sample-size 30] [--measurement-time 2.0] [--warmup-time 1.0] [--quick]
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import statistics
import subprocess
import sys
from pathlib import Path

# The 8 facade operations measured by crates/pdfluent/benches/facade.rs.
CORE_OPS = [
    "open_from_bytes",
    "page_count",
    "text",
    "form_fields",
    "to_bytes",
    "compress_strict",
    "subset_fonts",
    "extract_pages_all",
]


def repo_root() -> Path:
    # scripts/perf/<this file> -> repo root is two parents up.
    return Path(__file__).resolve().parents[2]


def env_metadata() -> dict:
    def _cmd(args: list[str]) -> str:
        try:
            return subprocess.run(
                args, capture_output=True, text=True, timeout=20
            ).stdout.strip()
        except Exception:
            return "unknown"

    return {
        "date_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "os": platform.system(),
        "os_release": platform.release(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "rustc": _cmd(["rustc", "--version"]),
        "cargo": _cmd(["cargo", "--version"]),
        "cpu_count": os.cpu_count(),
        "note": "Wall-time is hardware-dependent; budgets are expressed as "
        "relative regression thresholds, not absolute claims.",
    }


def run_bench(
    root: Path, sample_size: int, meas_t: float, warm_t: float, timeout_s: int
) -> None:
    """Run the criterion facade bench; raises on real failure."""
    cmd = [
        "cargo", "bench", "-p", "pdfluent", "--bench", "facade", "--",
        "--sample-size", str(sample_size),
        "--measurement-time", str(meas_t),
        "--warm-up-time", str(warm_t),
    ]
    proc = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=timeout_s)
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout[-2000:] + "\n" + proc.stderr[-2000:] + "\n")
        raise RuntimeError(f"criterion bench failed (exit {proc.returncode})")


def per_iter_ns_samples(criterion_dir: Path, op: str) -> list[float]:
    """Per-iteration nanoseconds from criterion raw samples (times/iters)."""
    sample_json = criterion_dir / op / "new" / "sample.json"
    if not sample_json.is_file():
        return []
    data = json.loads(sample_json.read_text())
    times = data.get("times") or []
    iters = data.get("iters") or []
    out = []
    for t, n in zip(times, iters):
        if n:
            out.append(float(t) / float(n))
    return out


def pct(samples: list[float], q: float) -> float:
    if not samples:
        return float("nan")
    s = sorted(samples)
    if len(s) == 1:
        return s[0]
    idx = q * (len(s) - 1)
    lo = int(idx)
    frac = idx - lo
    hi = min(lo + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * frac


def summarize(samples_ns: list[float]) -> dict:
    if not samples_ns:
        return {"status": "no_samples"}
    return {
        "samples": len(samples_ns),
        "min_ns": min(samples_ns),
        "p50_ns": pct(samples_ns, 0.50),
        "p95_ns": pct(samples_ns, 0.95),
        "p99_ns": pct(samples_ns, 0.99),
        "max_ns": max(samples_ns),
        "mean_ns": statistics.fmean(samples_ns),
        "stdev_ns": statistics.pstdev(samples_ns) if len(samples_ns) > 1 else 0.0,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--sample-size", type=int, default=30)
    ap.add_argument("--measurement-time", type=float, default=2.0)
    ap.add_argument("--warmup-time", type=float, default=1.0)
    ap.add_argument("--timeout", type=int, default=900)
    ap.add_argument("--quick", action="store_true",
                    help="reduced sampling for fast/CI smoke (not a stable baseline)")
    args = ap.parse_args()

    root = repo_root()
    ss = 10 if args.quick else args.sample_size
    mt = 1.0 if args.quick else args.measurement_time
    wt = 0.5 if args.quick else args.warmup_time

    try:
        run_bench(root, ss, mt, wt, args.timeout)
    except Exception as e:  # real benchmark failure -> nonzero
        sys.stderr.write(f"BENCH FAILURE: {e}\n")
        return 2

    criterion_dir = root / "target" / "criterion"
    ops: dict[str, dict] = {}
    missing = []
    for op in CORE_OPS:
        samples = per_iter_ns_samples(criterion_dir, op)
        if not samples:
            missing.append(op)
        ops[op] = summarize(samples)

    result = {
        "milestone": "SDK_GA_PERFORMANCE_BASELINE",
        "kind": "rust_core_facade",
        "fixture": "crates/pdfluent/tests/fixtures/sample.pdf (+ form.pdf for form_fields)",
        "criterion": {"sample_size": ss, "measurement_time_s": mt, "warmup_time_s": wt,
                      "quick": args.quick},
        "environment": env_metadata(),
        "operations": ops,
        "ops_missing_samples": missing,
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
    print(f"core perf run -> {args.out}")
    for op in CORE_OPS:
        m = ops[op]
        if m.get("status") == "no_samples":
            print(f"  {op:20s} NO SAMPLES")
        else:
            print(f"  {op:20s} p50={m['p50_ns']:>12.1f}ns  p95={m['p95_ns']:>12.1f}ns  n={m['samples']}")
    if missing:
        sys.stderr.write(f"WARNING: ops without samples: {missing}\n")
    # Parsing/run succeeded -> exit 0 even if some ops had no samples
    # (those become 'no_samples' and are handled by the budget checker).
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
