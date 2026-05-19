#!/usr/bin/env python3
"""QF3-C — ObjectStream cache real-world impact measurement.

Measures pdfluent flatten wall time and peak RSS on ObjStm-heavy PDFs
using two binaries: one built at the pre-QF2-B baseline (a266e77e4) and
one built at the post-QF2-B baseline (288fcd9b8 = QF2-B landed). Both
binaries share the same input + same methodology, so the only difference
is the QF2-B ObjectStream offsets cache in pdf-syntax.

Methodology:
  * 3 warm-up + 5 measure iterations per (doc, binary) pair
  * wall_ms = wall-clock around the binary invocation (subprocess timing)
  * Optional /usr/bin/time -v for RSS on Linux; on macOS RSS is not
    captured (BSD time format differs; left null)
  * Per-doc rows ordered by /ObjStm count descending
  * Aggregates: median %improvement, p25/p75, #docs with regression

Output:
  * <rundir>/QF3_C_OBJSTREAM_IMPACT.json — raw per-iteration data
  * <rundir>/QF3_C_OBJSTREAM_IMPACT.md   — human-readable summary

Usage:
  python3 scripts/qf3c_objstream_impact_measure.py <rundir> \\
      --pre-binary  /path/to/pre-qf2b/pdfluent \\
      --post-binary /path/to/post-qf2b/pdfluent \\
      --doc <pdf-path> [--doc <pdf-path> ...]

NO engine src changes; this is measurement-only.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

WARMUP = 2
MEASURE = 5
TIMEOUT_S = 300

# QF4-A additive: allow overriding via CLI flags --warmup / --measure / --timeout-s
# without changing the QF3-C default behaviour. Original constants above remain
# the defaults; argparse overrides them via globals at run-time. These flags are
# additive (no removal / rename), so existing QF3-C reproduction commands keep
# working unchanged.


def parse_rss_kb(text: str):
    """Parse peak RSS from either GNU time -v ("Maximum resident set size
    (kbytes): N") or macOS BSD time -l ("peak memory footprint" in bytes).
    Returns RSS in kilobytes, or None."""
    m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", text)
    if m:
        return int(m.group(1))
    m = re.search(r"(\d+)\s+peak memory footprint", text)
    if m:
        return int(m.group(1)) // 1024
    return None


def run_once(binary: Path, pdf: Path, outpath: Path, gtime_mode: str):
    """Run one flatten iteration, return (wall_ms, rss_kb_or_none).

    gtime_mode: 'gnu' (-v), 'bsd' (-l on macOS), or 'off'.
    """
    cmd: list[str] = []
    if gtime_mode == "gnu":
        cmd += ["/usr/bin/time", "-v"]
    elif gtime_mode == "bsd":
        cmd += ["/usr/bin/time", "-l"]
    cmd += [str(binary), "flatten", "--output", str(outpath), str(pdf)]
    t0 = time.perf_counter()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return None, None
    wall_ms = (time.perf_counter() - t0) * 1000.0
    if r.returncode != 0:
        # Some flatten paths exit 2 (encrypted); treat as failure here
        return None, None
    rss_kb = parse_rss_kb(r.stderr or "") if gtime_mode != "off" else None
    return wall_ms, rss_kb


def measure(binary: Path, pdf: Path, tmpdir: Path, gtime_mode: str):
    name = pdf.name
    outdir = tmpdir / f"out-{name}"
    outdir.mkdir(parents=True, exist_ok=True)
    out_pdf = outdir / "flat.pdf"
    # warm-up
    for _ in range(WARMUP):
        if out_pdf.exists():
            try:
                out_pdf.unlink()
            except OSError:
                pass
        run_once(binary, pdf, out_pdf, gtime_mode)
    # measure
    walls = []
    rsses = []
    for _ in range(MEASURE):
        if out_pdf.exists():
            try:
                out_pdf.unlink()
            except OSError:
                pass
        w, r = run_once(binary, pdf, out_pdf, gtime_mode)
        if w is not None:
            walls.append(w)
            if r is not None:
                rsses.append(r)
    if not walls:
        return {"error": "no successful iteration"}
    return {
        "wall_ms_samples": walls,
        "wall_ms_p50": float(statistics.median(walls)),
        "wall_ms_p95": float(sorted(walls)[max(0, int(len(walls) * 0.95) - 1)])
        if len(walls) > 1
        else walls[0],
        "wall_ms_min": float(min(walls)),
        "wall_ms_max": float(max(walls)),
        "rss_kb_samples": rsses,
        "rss_kb_max": max(rsses) if rsses else None,
        "iterations": len(walls),
    }


def count_objstm(pdf: Path):
    try:
        return pdf.read_bytes().count(b"/ObjStm")
    except OSError:
        return None


def main():
    # QF4-A additive: hoisted to the top of main() so the assignments at the
    # end of arg parsing are legal. Default values (WARMUP/MEASURE/TIMEOUT_S
    # constants) remain unchanged.
    global WARMUP, MEASURE, TIMEOUT_S
    ap = argparse.ArgumentParser(description="QF3-C ObjectStream cache real-world impact")
    ap.add_argument("rundir", help="Output dir for JSON/MD")
    ap.add_argument("--pre-binary", required=True, help="Path to pre-QF2-B pdfluent")
    ap.add_argument("--post-binary", required=True, help="Path to post-QF2-B pdfluent")
    ap.add_argument(
        "--doc",
        action="append",
        required=True,
        help="PDF path (repeatable; provide top ObjStm-heavy docs)",
    )
    ap.add_argument(
        "--no-gtime",
        action="store_true",
        help="Disable /usr/bin/time -v even if available (e.g. on macOS BSD)",
    )
    # QF4-A additive overrides — keep QF3-C defaults if omitted.
    ap.add_argument(
        "--warmup",
        type=int,
        default=WARMUP,
        help=f"Warm-up iterations per (doc, binary). Default={WARMUP}.",
    )
    ap.add_argument(
        "--measure",
        type=int,
        default=MEASURE,
        help=f"Measured iterations per (doc, binary). Default={MEASURE}.",
    )
    ap.add_argument(
        "--timeout-s",
        type=int,
        default=TIMEOUT_S,
        help=f"Per-iteration subprocess timeout in seconds. Default={TIMEOUT_S}.",
    )
    args = ap.parse_args()
    # Apply CLI overrides to module globals so `measure()` / `run_once()` use them.
    WARMUP = args.warmup
    MEASURE = args.measure
    TIMEOUT_S = args.timeout_s

    rundir = Path(args.rundir)
    rundir.mkdir(parents=True, exist_ok=True)
    tmpdir = rundir / "tmp"
    tmpdir.mkdir(exist_ok=True)

    pre = Path(args.pre_binary).resolve()
    post = Path(args.post_binary).resolve()
    if not pre.exists() or not post.exists():
        print(f"ERROR: missing binary (pre={pre.exists()} post={post.exists()})", file=sys.stderr)
        sys.exit(1)

    # Auto-detect /usr/bin/time flavour. GNU has -v; macOS BSD has -l.
    gtime_mode = "off"
    if not args.no_gtime and Path("/usr/bin/time").exists():
        if platform.system() == "Linux":
            gtime_mode = "gnu"
        elif platform.system() == "Darwin":
            gtime_mode = "bsd"

    docs_in = [Path(d).resolve() for d in args.doc]
    # Rank by /ObjStm count desc
    docs_ranked = sorted(
        ((count_objstm(p), p.stat().st_size, p) for p in docs_in if p.exists()),
        key=lambda t: -(t[0] or 0),
    )

    print(f"[qf3c] pre  binary: {pre}")
    print(f"[qf3c] post binary: {post}")
    print(f"[qf3c] docs       : {len(docs_ranked)}")
    print(f"[qf3c] gtime_mode  : {gtime_mode}")

    rows = []
    for objstm, size_b, pdf in docs_ranked:
        print(f"\n[qf3c] === {pdf.name} (/ObjStm={objstm}, size={size_b}) ===")
        print(f"[qf3c]   pre  …", flush=True)
        pre_m = measure(pre, pdf, tmpdir, gtime_mode)
        print(f"[qf3c]   post …", flush=True)
        post_m = measure(post, pdf, tmpdir, gtime_mode)
        row = {
            "doc": pdf.name,
            "path": str(pdf),
            "size_bytes": size_b,
            "objstm_markers": objstm,
            "pre_qf2b": pre_m,
            "post_qf2b": post_m,
        }
        # Compute % improvement on p50 and p95
        if "error" not in pre_m and "error" not in post_m:
            pre_p50 = pre_m["wall_ms_p50"]
            post_p50 = post_m["wall_ms_p50"]
            pre_p95 = pre_m["wall_ms_p95"]
            post_p95 = post_m["wall_ms_p95"]
            row["delta_p50_pct"] = 100.0 * (pre_p50 - post_p50) / pre_p50 if pre_p50 > 0 else 0.0
            row["delta_p95_pct"] = 100.0 * (pre_p95 - post_p95) / pre_p95 if pre_p95 > 0 else 0.0
            row["delta_p50_ms"] = pre_p50 - post_p50
            row["delta_p95_ms"] = pre_p95 - post_p95
            print(
                f"[qf3c]   p50: pre={pre_p50:7.1f}ms  post={post_p50:7.1f}ms  Δ={row['delta_p50_pct']:+6.2f}%"
            )
            print(
                f"[qf3c]   p95: pre={pre_p95:7.1f}ms  post={post_p95:7.1f}ms  Δ={row['delta_p95_pct']:+6.2f}%"
            )
        rows.append(row)

    # Aggregate
    p50_deltas = [r["delta_p50_pct"] for r in rows if "delta_p50_pct" in r]
    p95_deltas = [r["delta_p95_pct"] for r in rows if "delta_p95_pct" in r]
    agg = {
        "n_docs": len(rows),
        "n_successful": len(p50_deltas),
        "delta_p50_median_pct": statistics.median(p50_deltas) if p50_deltas else None,
        "delta_p50_p25_pct": statistics.quantiles(p50_deltas, n=4)[0] if len(p50_deltas) >= 4 else None,
        "delta_p50_p75_pct": statistics.quantiles(p50_deltas, n=4)[2] if len(p50_deltas) >= 4 else None,
        "delta_p95_median_pct": statistics.median(p95_deltas) if p95_deltas else None,
        "delta_p95_p25_pct": statistics.quantiles(p95_deltas, n=4)[0] if len(p95_deltas) >= 4 else None,
        "delta_p95_p75_pct": statistics.quantiles(p95_deltas, n=4)[2] if len(p95_deltas) >= 4 else None,
        "docs_with_gain_gt_10pct_p50": sum(1 for d in p50_deltas if d > 10),
        "docs_with_gain_gt_10pct_p95": sum(1 for d in p95_deltas if d > 10),
        "docs_with_regression_p50": sum(1 for d in p50_deltas if d < -2),
        "docs_with_regression_p95": sum(1 for d in p95_deltas if d < -2),
    }

    payload = {
        "schema_version": "qf3c-objstream-impact-1",
        "baseline_pre_qf2b": "a266e77e4",
        "baseline_post_qf2b": "288fcd9b8 (xfa/qf3-plan tip)",
        "baseline_date": time.strftime("%Y-%m-%d"),
        "env": {
            "platform": platform.platform(),
            "cpu_count_logical": os.cpu_count(),
            "gtime_mode": gtime_mode,
            "warmup": WARMUP,
            "measure": MEASURE,
        },
        "pre_binary": str(pre),
        "post_binary": str(post),
        "rows": rows,
        "aggregate": agg,
    }
    out_json = rundir / "QF3_C_OBJSTREAM_IMPACT.json"
    out_json.write_text(json.dumps(payload, indent=2))
    print(f"\n[qf3c] wrote {out_json}")
    # Markdown report
    md = _render_md(payload)
    out_md = rundir / "QF3_C_OBJSTREAM_IMPACT.md"
    out_md.write_text(md)
    print(f"[qf3c] wrote {out_md}")


def _render_md(payload: dict) -> str:
    lines = []
    lines.append("# QF3-C — ObjectStream cache real-world impact")
    lines.append("")
    lines.append(f"**Date:** {payload['baseline_date']}")
    lines.append(f"**Pre-QF2-B baseline:** `{payload['baseline_pre_qf2b']}`")
    lines.append(f"**Post-QF2-B baseline:** `{payload['baseline_post_qf2b']}`")
    lines.append("")
    lines.append("## Per-doc results")
    lines.append("")
    lines.append("| Doc | /ObjStm | size (KB) | pre p50 (ms) | post p50 (ms) | Δp50 % | pre p95 (ms) | post p95 (ms) | Δp95 % |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for r in payload["rows"]:
        if "delta_p50_pct" not in r:
            lines.append(f"| {r['doc']} | {r['objstm_markers']} | {r['size_bytes']//1024} | — | — | err | — | — | err |")
            continue
        pre = r["pre_qf2b"]
        post = r["post_qf2b"]
        lines.append(
            f"| {r['doc']} | {r['objstm_markers']} | {r['size_bytes']//1024} | "
            f"{pre['wall_ms_p50']:.1f} | {post['wall_ms_p50']:.1f} | {r['delta_p50_pct']:+.2f} | "
            f"{pre['wall_ms_p95']:.1f} | {post['wall_ms_p95']:.1f} | {r['delta_p95_pct']:+.2f} |"
        )
    agg = payload["aggregate"]
    lines.append("")
    lines.append("## Aggregate")
    lines.append("")
    lines.append(f"- **n_docs:** {agg['n_docs']} (successful: {agg['n_successful']})")
    if agg["delta_p50_median_pct"] is not None:
        lines.append(f"- **Δp50 median:** {agg['delta_p50_median_pct']:+.2f} %")
    if agg["delta_p50_p25_pct"] is not None:
        lines.append(f"- **Δp50 p25 / p75:** {agg['delta_p50_p25_pct']:+.2f} % / {agg['delta_p50_p75_pct']:+.2f} %")
    if agg["delta_p95_median_pct"] is not None:
        lines.append(f"- **Δp95 median:** {agg['delta_p95_median_pct']:+.2f} %")
    if agg["delta_p95_p25_pct"] is not None:
        lines.append(f"- **Δp95 p25 / p75:** {agg['delta_p95_p25_pct']:+.2f} % / {agg['delta_p95_p75_pct']:+.2f} %")
    lines.append(f"- **docs with >10 % p50 gain:** {agg['docs_with_gain_gt_10pct_p50']}")
    lines.append(f"- **docs with >10 % p95 gain:** {agg['docs_with_gain_gt_10pct_p95']}")
    lines.append(f"- **docs with regression (>2 % p50):** {agg['docs_with_regression_p50']}")
    lines.append(f"- **docs with regression (>2 % p95):** {agg['docs_with_regression_p95']}")
    lines.append("")
    lines.append("## Methodology")
    lines.append("")
    lines.append(f"- {payload['env']['warmup']} warm-up + {payload['env']['measure']} measure iterations per (doc, binary)")
    lines.append("- Wall time = Python subprocess wall-clock around the binary")
    lines.append(f"- RSS via /usr/bin/time mode: {payload['env']['gtime_mode']}")
    lines.append(f"- Platform: {payload['env']['platform']}")
    lines.append("- Binary comparison is the **only** changed variable. Both binaries share")
    lines.append("  the same Cargo workspace + the same upstream dependencies; only the")
    lines.append("  `pdf-syntax` QF2-B patches (`crates/pdf-syntax/src/{data,xref}.rs`)")
    lines.append("  differ between the two commits.")
    lines.append("")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    main()
