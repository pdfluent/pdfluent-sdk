#!/usr/bin/env python3
"""Display GATE history trend from benchmarks/gate-history/.

Usage:
    python3 scripts/gate_trend.py
    python3 scripts/gate_trend.py --last 10
    python3 scripts/gate_trend.py --ascii-chart
"""
import argparse
import json
from pathlib import Path


def load_history() -> list[dict]:
    history_dir = Path(__file__).parent.parent / "benchmarks" / "gate-history"
    if not history_dir.exists():
        return []
    entries = []
    for f in sorted(history_dir.glob("gate-*.json")):
        try:
            entries.append(json.loads(f.read_text()))
        except Exception:
            pass
    # Sort by gate_id numerically where possible
    def sort_key(e):
        gid = e.get("gate_id", "0")
        try:
            return float(gid)
        except ValueError:
            return float("inf")
    return sorted(entries, key=sort_key)


def ascii_chart(history: list[dict], width: int = 60) -> str:
    rates = [e.get("pass_rate", 0) or 0 for e in history]
    if not rates:
        return "(no data)"
    min_r = min(rates)
    max_r = max(rates)
    span = max_r - min_r or 0.01
    height = 10
    lines = []
    for row in range(height, -1, -1):
        threshold = min_r + span * row / height
        line = ""
        for r in rates:
            line += "█" if r >= threshold else " "
        label = f"{threshold*100:.1f}% |" if row % 2 == 0 else "        |"
        lines.append(f"{label} {line}")
    lines.append("         " + "-" * len(rates))
    gates = [e.get("gate_id", "?") for e in history]
    # Compact gate labels at bottom
    tick_line = "         "
    for i, g in enumerate(gates):
        tick_line += g[0] if len(g) > 0 else "."
    lines.append(tick_line)
    return "\n".join(lines)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--last", type=int, default=0, help="Show last N gates (0 = all)")
    p.add_argument("--ascii-chart", action="store_true")
    args = p.parse_args()

    history = load_history()
    if not history:
        print("No gate history found. Run scripts/archive_gate_result.py after each GATE.")
        return

    if args.last > 0:
        history = history[-args.last:]

    # Table
    header = f"{'Gate':<8} {'Date':<12} {'Commit':<10} {'Pass':<6} {'Fail':<6} {'Crash':<6} {'Rate':<8} {'SSIM':>6}"
    print(header)
    print("-" * len(header))
    for e in history:
        rate = f"{e.get('pass_rate', 0)*100:.1f}%" if e.get('pass_rate') else "?"
        ssim = f"{e.get('mean_ssim', ''):.4f}" if e.get('mean_ssim') else "?"
        print(
            f"{e.get('gate_id','?'):<8} "
            f"{e.get('date','?'):<12} "
            f"{e.get('commit','?'):<10} "
            f"{e.get('pass',0):<6} "
            f"{e.get('fail',0):<6} "
            f"{e.get('crash',0):<6} "
            f"{rate:<8} "
            f"{ssim:>6}"
        )

    if args.ascii_chart:
        print("\nPass rate trend:")
        print(ascii_chart(history))


if __name__ == "__main__":
    main()
