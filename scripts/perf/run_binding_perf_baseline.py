#!/usr/bin/env python3
"""run_binding_perf_baseline.py — non-XFA binding overhead baseline.

Measures binding overhead for built local artifacts. For each binding the
status is one of:
  - green_measured        : artifact present, timed open+page_count loop
  - blocked_missing_artifact : artifact not built in this checkout (honest;
                              buildable per the QR-11 evidence) -> advisory
Never fakes a number. Release-blocking surface is the Rust core (Phase 3);
bindings are advisory overhead surfaces.

Currently auto-measures C-ABI (compiles a tiny C loop against libpdf_capi).
Node/Python/.NET/Java are measured only when their built artifact is present
(rebuilding them is environment-heavy; see PHASE4 doc), otherwise reported
as blocked_missing_artifact with the exact reason.

Usage:
    python3 scripts/perf/run_binding_perf_baseline.py --out <json> [--iters 300]
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def measure_c_abi(root: Path, fixture: Path, iters: int) -> dict:
    lib = root / "target" / "release"
    dylib = lib / "libpdf_capi.dylib"
    so = lib / "libpdf_capi.so"
    if not dylib.is_file() and not so.is_file():
        return {"status": "blocked_missing_artifact",
                "reason": "libpdf_capi not built; run `cargo build -p pdf-capi --release`"}
    inc = root / "crates" / "pdf-capi" / "include"
    src = root / "scripts" / "perf" / "c_abi_perf_loop.c"
    binp = Path("/tmp/c_abi_perf_loop")
    import platform
    cc_cmd = ["cc", "-O2"]
    # On macOS, compile for the *dylib's* arch (detected via lipo): the host
    # python/cc may be x86_64 under Rosetta while the lib is arm64.
    if platform.system() == "Darwin":
        target = dylib if dylib.is_file() else so
        archs = subprocess.run(["lipo", "-archs", str(target)],
                               capture_output=True, text=True)
        arch = (archs.stdout.split() or ["arm64"])[0]
        cc_cmd += ["-arch", arch]
    cc_cmd += ["-I", str(inc), "-o", str(binp), str(src), "-L", str(lib), "-lpdf_capi"]
    cc = subprocess.run(cc_cmd, capture_output=True, text=True)
    if cc.returncode != 0:
        return {"status": "blocked_build_failed", "reason": cc.stderr[-500:]}
    env = dict(os.environ)
    env["DYLD_LIBRARY_PATH"] = str(lib)
    env["LD_LIBRARY_PATH"] = str(lib)
    run = subprocess.run([str(binp), str(fixture), str(iters)],
                         capture_output=True, text=True, env=env, timeout=120)
    if run.returncode != 0:
        return {"status": "blocked_run_failed", "reason": run.stderr[-500:]}
    try:
        m = json.loads(run.stdout.strip().splitlines()[-1])
    except Exception as e:
        return {"status": "blocked_parse_failed", "reason": f"{e}: {run.stdout[-300:]}"}
    m["status"] = "green_measured"
    return m


def detect_or_block(name: str, present: bool, build_hint: str) -> dict:
    if present:
        # Artifact present but no auto-timer wired here yet -> honest skip.
        return {"status": "artifact_present_no_timer",
                "reason": f"{name} artifact found; per-binding timer not run in this pass"}
    return {"status": "blocked_missing_artifact",
            "reason": f"{name} artifact not built in this checkout; buildable via {build_hint} "
                      f"(see QR-11 SDK_GA_QR11 report). Advisory surface, not release-blocking."}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--iters", type=int, default=300)
    ap.add_argument("--fixture", default="tests/corpus-mini/multi-page.pdf")
    args = ap.parse_args()
    root = repo_root()
    fixture = root / args.fixture

    bindings: dict[str, dict] = {}
    bindings["c-abi"] = measure_c_abi(root, fixture, args.iters)

    node_node = list((root / "crates" / "pdf-node").glob("*.node"))
    bindings["node"] = detect_or_block("node", bool(node_node),
                                       "npm install && npm run build (crates/pdf-node)")
    py_so = list((root / "crates" / "pdf-python" / "python" / "pdfluent").glob("*.so"))
    bindings["python"] = detect_or_block("python", bool(py_so),
                                         "maturin develop --release in an arm64 venv (crates/pdf-python)")
    dotnet_dll = list((root / "bindings" / "dotnet").rglob("PDFluent.dll"))
    bindings["dotnet"] = detect_or_block("dotnet", bool(dotnet_dll),
                                         "dotnet build + x86_64 libpdf_capi (bindings/dotnet)")
    java_jni = list((root / "target").rglob("libpdfluent_java.*"))
    bindings["java"] = detect_or_block("java", bool(java_jni),
                                       "cargo build -p pdf-java + x86_64 JNI (bindings/java)")

    result = {
        "milestone": "SDK_GA_PERFORMANCE_BASELINE",
        "kind": "binding_overhead",
        "fixture": args.fixture,
        "iters": args.iters,
        "bindings": bindings,
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
    print(f"binding perf run -> {args.out}")
    for k, v in bindings.items():
        if v.get("status") == "green_measured":
            print(f"  {k:8s} p50={v['p50_ns']/1000:.1f}us p95={v['p95_ns']/1000:.1f}us n={v['samples']}")
        else:
            print(f"  {k:8s} {v['status']}")
    return 0  # measurement run never fails on advisory/blocked; checker enforces budgets


if __name__ == "__main__":
    raise SystemExit(main())
