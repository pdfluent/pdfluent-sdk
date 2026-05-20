#!/usr/bin/env python3
"""run_repeated_run_stability.py — long-running stability smoke (non-XFA).

Not a replacement for LSAN/ASAN (QR-9). A GA smoke that a long-running
process stays healthy: no crashes, bounded peak RSS, and low run-to-run
duration drift over many open/page_count/free iterations.

Targets:
  - C-ABI repeated open+page_count+free loop (real peak RSS via /usr/bin/time -l
    on macOS or getrusage on Linux; run twice -> duration drift).
  - WASM repeated-open signal is taken from the Phase 5 wasm_perf_run.json
    (in-browser repeated opens + usedJSHeapSize), referenced if present.

Exit nonzero only on a real failure (crash / build failure). Drift and RSS
thresholds are advisory and enforced by check_performance_budgets.py.

Usage: python3 scripts/perf/run_repeated_run_stability.py --out <json> [--iters 4000]
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
import time
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def build_c_loop(root: Path) -> tuple[Path | None, str]:
    lib = root / "target" / "release"
    dylib = lib / "libpdf_capi.dylib"
    so = lib / "libpdf_capi.so"
    target = dylib if dylib.is_file() else (so if so.is_file() else None)
    if target is None:
        return None, "libpdf_capi not built (cargo build -p pdf-capi --release)"
    inc = root / "crates" / "pdf-capi" / "include"
    src = root / "scripts" / "perf" / "c_abi_perf_loop.c"
    binp = Path("/tmp/c_abi_perf_loop_stab")
    cmd = ["cc", "-O2"]
    if platform.system() == "Darwin":
        archs = subprocess.run(["lipo", "-archs", str(target)], capture_output=True, text=True)
        cmd += ["-arch", (archs.stdout.split() or ["arm64"])[0]]
    cmd += ["-I", str(inc), "-o", str(binp), str(src), "-L", str(lib), "-lpdf_capi"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        return None, r.stderr[-400:]
    return binp, "ok"


def run_with_rss(binp: Path, fixture: Path, iters: int, lib: Path) -> dict:
    """Run the loop, capturing wall time, exit code, and peak RSS (KB)."""
    env = dict(os.environ)
    env["DYLD_LIBRARY_PATH"] = str(lib)
    env["LD_LIBRARY_PATH"] = str(lib)
    is_mac = platform.system() == "Darwin"
    cmd = (["/usr/bin/time", "-l"] if is_mac else ["/usr/bin/time", "-v"]) + \
          [str(binp), str(fixture), str(iters)]
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600)
    wall = time.time() - t0
    rss_kb = None
    m = re.search(r"(\d+)\s+maximum resident set size", p.stderr)  # macOS: bytes
    if m:
        rss_kb = int(m.group(1)) // 1024
    else:
        m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", p.stderr)  # GNU time
        if m:
            rss_kb = int(m.group(1))
    return {"exit": p.returncode, "wall_s": round(wall, 4), "peak_rss_kb": rss_kb,
            "crashed": p.returncode < 0}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--iters", type=int, default=4000)
    ap.add_argument("--fixture", default="tests/corpus-mini/multi-page.pdf")
    args = ap.parse_args()
    root = repo_root()
    lib = root / "target" / "release"
    fixture = root / args.fixture

    result = {"milestone": "SDK_GA_PERFORMANCE_BASELINE", "kind": "repeated_run_stability",
              "iters": args.iters, "fixture": args.fixture, "targets": {}}
    rc = 0

    binp, msg = build_c_loop(root)
    if binp is None:
        result["targets"]["c-abi"] = {"status": "blocked_missing_artifact", "reason": msg}
    else:
        r1 = run_with_rss(binp, fixture, args.iters, lib)
        r2 = run_with_rss(binp, fixture, args.iters, lib)
        drift = None
        if r1["wall_s"] and r2["wall_s"]:
            drift = abs(r2["wall_s"] - r1["wall_s"]) / max(r1["wall_s"], 1e-9)
        crashed = r1["crashed"] or r2["crashed"] or r1["exit"] != 0 or r2["exit"] != 0
        result["targets"]["c-abi"] = {
            "status": "green_measured" if not crashed else "FAILED_CRASH",
            "run1": r1, "run2": r2,
            "wall_drift_ratio": round(drift, 4) if drift is not None else None,
            "peak_rss_kb": max([x for x in (r1["peak_rss_kb"], r2["peak_rss_kb"]) if x] or [0]),
            "crashed": crashed,
        }
        if crashed:
            rc = 1

    # WASM repeated-open signal (from Phase 5), if present.
    wasm_json = root / "benchmarks" / "runs" / "ga_readiness_3d" / \
        "sdk_ga_performance_baseline" / "wasm_perf_run.json"
    if wasm_json.is_file():
        w = json.loads(wasm_json.read_text())
        result["targets"]["wasm"] = {
            "status": "referenced_from_phase5",
            "valid_oks": (w.get("valid") or {}).get("oks"),
            "hostile_errs": (w.get("hostile") or {}).get("errs"),
            "usedJSHeapSize_valid": (w.get("valid") or {}).get("usedJSHeapSize"),
            "runtime_healthy_after_hostile": w.get("runtime_healthy_after_hostile"),
        }
    else:
        result["targets"]["wasm"] = {"status": "blocked_missing_artifact",
                                     "reason": "wasm_perf_run.json not present (Phase 5 not run here)"}

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
    print(f"stability -> {args.out}")
    c = result["targets"].get("c-abi", {})
    if c.get("status") == "green_measured":
        print(f"  c-abi: {args.iters} iters x2  peak_rss={c['peak_rss_kb']}KB  "
              f"wall_drift={c['wall_drift_ratio']}  crashed={c['crashed']}")
    else:
        print(f"  c-abi: {c.get('status')}")
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
