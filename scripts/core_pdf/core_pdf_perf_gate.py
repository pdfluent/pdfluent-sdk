#!/usr/bin/env python3
"""Core (non-XFA) PDF SDK performance baseline gate.

Measures wall-time of core operations (load + page-count + text extraction)
via the pre-built `golden_path` example binary against the safe committed
fixture set, and compares against per-operation budgets.

Design constraints (per CORE_PDF_SDK_ENTERPRISE_QUALITY_100):
  * dry-run by DEFAULT — no heavy execution unless --run is passed.
  * --output-dir with storage-safe validation (must live under the repo
    benchmarks/ tree or an explicit /tmp path; never a private $HOME path
    committed into reports).
  * JSON output.
  * budget file (JSON) drives pass/fail.
  * controlled timeout — a hung operation fails the gate, never hangs CI.

Usage:
  python3 scripts/core_pdf/core_pdf_perf_gate.py            # dry-run plan
  python3 scripts/core_pdf/core_pdf_perf_gate.py --run \
      --output-dir benchmarks/runs/ga_100_closure_v3/core_pdf_sdk_quality
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BIN = REPO / "pdfluent-examples/rust/target/release/golden_path"
FIXTURES_DIR = REPO / "tests/corpus-mini"
DEFAULT_BUDGET = REPO / "scripts/core_pdf/core_pdf_perf_budget.json"
PER_OP_TIMEOUT_S = 30  # controlled timeout: hung op fails the gate

# Operations measured = one golden_path invocation per fixture
# (load + page_count + version + metadata + first-page text extraction).
FIXTURES = [
    "simple.pdf",
    "multi-page.pdf",
    "acroform.pdf",
    "pdfa-2b.pdf",
    "scanned.pdf",
]


def storage_safe(output_dir: Path) -> bool:
    """Output dir must be inside the repo benchmarks tree or an explicit
    /tmp path — never a leaked private $HOME location in committed reports."""
    rp = output_dir.resolve()
    if str(rp).startswith("/tmp") or str(rp).startswith("/private/tmp"):
        return True
    try:
        rp.relative_to(REPO / "benchmarks")
        return True
    except ValueError:
        return False


def measure(fixture: Path) -> dict:
    start = time.perf_counter()
    try:
        proc = subprocess.run(
            [str(BIN), str(fixture)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            timeout=PER_OP_TIMEOUT_S,
        )
        elapsed = time.perf_counter() - start
        return {
            "fixture": fixture.name,
            "wall_s": round(elapsed, 4),
            "exit": proc.returncode,
            "hung": False,
        }
    except subprocess.TimeoutExpired:
        return {
            "fixture": fixture.name,
            "wall_s": None,
            "exit": None,
            "hung": True,
        }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="actually measure (default: dry-run plan)")
    ap.add_argument("--output-dir", default=None)
    ap.add_argument("--budget", default=str(DEFAULT_BUDGET))
    args = ap.parse_args()

    budget = json.loads(Path(args.budget).read_text()) if Path(args.budget).exists() else {}
    op_budget = float(budget.get("per_op_wall_s_max", 5.0))

    plan = {
        "binary": str(BIN.relative_to(REPO)),
        "fixtures": FIXTURES,
        "per_op_wall_s_max": op_budget,
        "per_op_timeout_s": PER_OP_TIMEOUT_S,
    }

    if not args.run:
        print("core_pdf_perf_gate: DRY-RUN (pass --run to measure)")
        print(json.dumps(plan, indent=2))
        # Dry-run still validates the binary + fixtures exist.
        missing = [f for f in FIXTURES if not (FIXTURES_DIR / f).exists()]
        if missing:
            print(f"WARN: missing fixtures: {missing}", file=sys.stderr)
        if not BIN.exists():
            print(
                "NOTE: golden_path binary not built; build with "
                "`cargo build --release` in pdfluent-examples/rust before --run",
                file=sys.stderr,
            )
        return 0

    if not BIN.exists():
        print(f"ERROR: binary not found: {BIN}", file=sys.stderr)
        return 2

    results = [measure(FIXTURES_DIR / f) for f in FIXTURES if (FIXTURES_DIR / f).exists()]
    failures = []
    for r in results:
        if r["hung"]:
            failures.append(f"{r['fixture']}: HUNG (> {PER_OP_TIMEOUT_S}s)")
        elif r["exit"] != 0:
            failures.append(f"{r['fixture']}: exit={r['exit']}")
        elif r["wall_s"] is not None and r["wall_s"] > op_budget:
            failures.append(f"{r['fixture']}: {r['wall_s']}s > budget {op_budget}s")

    report = {
        "plan": plan,
        "results": results,
        "failures": failures,
        "verdict": "PASS" if not failures else "FAIL",
    }

    out = json.dumps(report, indent=2)
    if args.output_dir:
        od = Path(args.output_dir)
        if not storage_safe(od):
            print(f"ERROR: --output-dir {od} is not storage-safe", file=sys.stderr)
            return 2
        od.mkdir(parents=True, exist_ok=True)
        (od / "CORE_PDF_PERFORMANCE_BASELINE.json").write_text(out + "\n")
    print(out)
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
