#!/usr/bin/env python3
"""check_performance_budgets.py — non-XFA SDK performance regression gate.

Flattens a performance run directory (core / binding / wasm / stability JSON)
into a flat metric namespace, then compares it against a committed baseline
under a budget policy. Distinguishes HARD release-blocking budgets from
advisory and informational ones.

Exit codes:
  0  no release-blocking regression (advisory/informational may warn)
  1  at least one release-blocking budget violated, OR schema/baseline error

Modes:
  --emit-baseline   write the flattened current run as the baseline JSON
  (default)         compare current run dir against baseline under budgets

Usage:
  python3 scripts/perf/check_performance_budgets.py \
      --run-dir benchmarks/runs/ga_readiness_3d/sdk_ga_performance_baseline \
      --baseline .../performance_baseline.json \
      --budgets  .../performance_budgets.json
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def _load(p: Path) -> dict | None:
    return json.loads(p.read_text()) if p.is_file() else None


def flatten(run_dir: Path) -> dict:
    """Map the per-phase run JSONs to a flat {metric: value} namespace."""
    m: dict[str, object] = {}
    samples: dict[str, int] = {}

    core = _load(run_dir / "core_perf_run.json")
    if core:
        for op, v in core.get("operations", {}).items():
            if isinstance(v, dict) and "p50_ns" in v:
                m[f"rust.{op}.p50_ns"] = v["p50_ns"]
                m[f"rust.{op}.p95_ns"] = v["p95_ns"]
                samples[f"rust.{op}.p50_ns"] = v.get("samples", 0)
                samples[f"rust.{op}.p95_ns"] = v.get("samples", 0)

    bind = _load(run_dir / "binding_perf_run.json")
    if bind:
        c = bind.get("bindings", {}).get("c-abi", {})
        if c.get("status") == "green_measured":
            m["cabi.open_pagecount_free.p50_ns"] = c["p50_ns"]
            m["cabi.open_pagecount_free.p95_ns"] = c["p95_ns"]
            samples["cabi.open_pagecount_free.p50_ns"] = c.get("samples", 0)
            samples["cabi.open_pagecount_free.p95_ns"] = c.get("samples", 0)

    # Language-binding overhead (Node / Python), measured by the per-binding
    # timing harnesses (scripts/perf/bindings/*). Optional files; included
    # when present so the of-record run can budget binding overhead.
    for bname, fname in (("node", "binding_node_run.json"), ("python", "binding_python_run.json"),
                          ("java", "binding_java_run.json"), ("dotnet", "binding_dotnet_run.json")):
        bj = _load(run_dir / fname)
        if bj and bj.get("status") == "green_measured":
            v = bj.get("valid", {})
            if "p50_ns" in v:
                m[f"{bname}.open_pagecount.p50_ns"] = v["p50_ns"]
                m[f"{bname}.open_pagecount.p95_ns"] = v["p95_ns"]
                samples[f"{bname}.open_pagecount.p50_ns"] = v.get("samples", 0)
                samples[f"{bname}.open_pagecount.p95_ns"] = v.get("samples", 0)
            mf = bj.get("malformed", {})
            if "p50_ns" in mf:
                m[f"{bname}.error_path.p50_ns"] = mf["p50_ns"]
                samples[f"{bname}.error_path.p50_ns"] = mf.get("samples", 0)

    wasm = _load(run_dir / "wasm_perf_run.json")
    if wasm and wasm.get("ok"):
        if "init_ms" in wasm:
            m["wasm.init_ms"] = wasm["init_ms"]
        v = wasm.get("valid", {})
        if "p50_ns" in v:
            m["wasm.open_pagecount.p50_ns"] = v["p50_ns"]

    stab = _load(run_dir / "stability_run.json")
    if stab:
        c = stab.get("targets", {}).get("c-abi", {})
        if c.get("status") == "green_measured":
            m["stability.cabi.crashed"] = c.get("crashed")
            m["stability.cabi.peak_rss_kb"] = c.get("peak_rss_kb")
            m["stability.cabi.exit_run1"] = c.get("run1", {}).get("exit")
            m["stability.cabi.exit_run2"] = c.get("run2", {}).get("exit")

    return {"metrics": m, "samples": samples}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run-dir", required=True)
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--budgets", required=True)
    ap.add_argument("--emit-baseline", action="store_true")
    args = ap.parse_args()

    run_dir = Path(args.run_dir)
    cur = flatten(run_dir)

    if args.emit_baseline:
        Path(args.baseline).write_text(json.dumps(
            {"milestone": "SDK_GA_PERFORMANCE_BASELINE", "metrics": cur["metrics"],
             "samples": cur["samples"]}, indent=2))
        print(f"baseline emitted -> {args.baseline} ({len(cur['metrics'])} metrics)")
        return 0

    baseline = _load(Path(args.baseline))
    budgets = _load(Path(args.budgets))
    if baseline is None or budgets is None:
        sys.stderr.write("ERROR: missing baseline or budgets file\n")
        return 1
    base_m = baseline.get("metrics", {})
    bud = budgets.get("metrics", {})

    hard_fail, advisory_warn, info_note, skipped = [], [], [], []

    for metric, policy in bud.items():
        cls = policy.get("class", "advisory")
        cur_v = cur["metrics"].get(metric)
        base_v = base_m.get(metric)

        if cls == "blocked_missing_artifact":
            skipped.append((metric, "policy: blocked_missing_artifact"))
            continue
        if cur_v is None:
            msg = f"{metric}: no current value (artifact missing?)"
            (hard_fail if cls == "release_blocking" and policy.get("required") else
             (advisory_warn if cls != "informational" else info_note)).append(msg)
            continue

        # boolean expectation (e.g. crashed == false, exit == 0)
        if "expect" in policy:
            if cur_v != policy["expect"]:
                line = f"{metric}: got {cur_v}, expected {policy['expect']}"
                (hard_fail if cls == "release_blocking" else advisory_warn).append(line)
            continue

        if base_v is None or not isinstance(base_v, (int, float)) or base_v <= 0:
            skipped.append((metric, "no usable baseline value"))
            continue

        # min sample guard
        need = policy.get("min_samples", 0)
        have = cur["samples"].get(metric, 1_000_000)
        if have < need:
            (advisory_warn if cls != "informational" else info_note).append(
                f"{metric}: only {have} samples (< {need}) -> not gated this run")
            continue

        regression_pct = (cur_v - base_v) / base_v * 100.0
        allowed = policy.get("max_regression_pct", 50) + policy.get("noise_pct", 0)
        line = f"{metric}: {regression_pct:+.1f}% vs baseline (allowed +{allowed:.0f}%) [base={base_v:.1f}, cur={cur_v:.1f}]"
        if regression_pct > allowed:
            if cls == "release_blocking":
                hard_fail.append(line)
            elif cls == "advisory":
                advisory_warn.append(line)
            else:
                info_note.append(line)

    print("=== PERFORMANCE BUDGET CHECK ===")
    print(f"metrics in baseline: {len(base_m)}; budgets: {len(bud)}; "
          f"current metrics: {len(cur['metrics'])}")
    if hard_fail:
        print("\nRELEASE-BLOCKING REGRESSIONS:")
        for x in sorted(hard_fail):
            print("  FAIL  " + x)
    if advisory_warn:
        print("\nADVISORY (warn only):")
        for x in sorted(advisory_warn):
            print("  WARN  " + x)
    if info_note:
        print("\nINFORMATIONAL (not gated):")
        for x in sorted(info_note):
            print("  INFO  " + x)
    if skipped:
        print("\nSKIPPED:")
        for mm, why in skipped:
            print(f"  SKIP  {mm}: {why}")

    if hard_fail:
        print(f"\nPERFORMANCE_BUDGETS: FAIL — {len(hard_fail)} release-blocking regression(s).")
        return 1
    print(f"\nPERFORMANCE_BUDGETS: PASS — 0 release-blocking regressions, "
          f"{len(advisory_warn)} advisory warning(s), {len(info_note)} informational.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
