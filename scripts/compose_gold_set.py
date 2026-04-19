#!/usr/bin/env python3
"""EVH-GOLD-01: Compose a stratified gold review set from benchmark results.

Creates a JSON gold set of 25-50 documents selected using a stratified sampling
strategy across six strata. Supports --update mode to re-select documents from
new results while preserving entries that already have human labels.

Usage (with results):
    python3 scripts/compose_gold_set.py \
        --results benchmarks/enterprise-baseline-2026-04-19.json \
        --inventory benchmarks/corpus_inventory.json \
        --output benchmarks/gold_set/gold_set.json \
        --target 40 \
        --strategy stratified

Usage (placeholder, no results yet):
    python3 scripts/compose_gold_set.py \
        --output benchmarks/gold_set/gold_set.json

Update mode (re-select from new results, preserve labeled entries):
    python3 scripts/compose_gold_set.py \
        --results benchmarks/enterprise-baseline-2026-04-19.json \
        --inventory benchmarks/corpus_inventory.json \
        --output benchmarks/gold_set/gold_set.json \
        --update
"""

from __future__ import annotations

import argparse
import datetime
import json
import random
import sys
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Default strata configuration
# ---------------------------------------------------------------------------

DEFAULT_STRATA_CONFIG = {
    "high_ssim": {
        "description": "status=fully_correct AND ssim >= 0.97 (expected fully_correct)",
        "target": 10,
    },
    "mid_ssim": {
        "description": "status in (minor_deviation, major_deviation) AND 0.88 <= ssim <= 0.96 (borderline)",
        "target": 10,
    },
    "low_ssim": {
        "description": "status in (render_fail, partial_render) AND ssim < 0.88 (failures)",
        "target": 8,
    },
    "scripted": {
        "description": "inventory has_formcalc=True OR has_javascript=True (complexity)",
        "target": 5,
    },
    "oracle_suspect": {
        "description": "oracle_fault=True or oracle_fault='suspected' (oracle validation)",
        "target": 5,
    },
    "dynamic": {
        "description": "inventory xfa_type=dynamic (XFA type coverage)",
        "target": 2,
    },
}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def load_json(path: Path) -> dict | list:
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def save_json(data: dict, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2, ensure_ascii=False)
    print(f"Wrote: {path}")


def get_ssim(doc: dict) -> Optional[float]:
    """Extract SSIM from a benchmark result document entry."""
    # Try common field locations used by run_xfa_benchmark.py output
    for key in ("ssim", "visual_ssim", "ssim_score"):
        if key in doc and doc[key] is not None:
            try:
                return float(doc[key])
            except (TypeError, ValueError):
                pass
    # Nested under visual_result
    visual = doc.get("visual_result") or {}
    for key in ("ssim", "ssim_score", "mean_ssim"):
        if key in visual and visual[key] is not None:
            try:
                return float(visual[key])
            except (TypeError, ValueError):
                pass
    return None


def get_completeness(doc: dict) -> Optional[float]:
    """Extract completeness score from a benchmark result document entry."""
    for key in ("completeness", "data_completeness", "completeness_score"):
        if key in doc and doc[key] is not None:
            try:
                return float(doc[key])
            except (TypeError, ValueError):
                pass
    comp = doc.get("completeness_result") or {}
    for key in ("completeness", "score", "ratio"):
        if key in comp and comp[key] is not None:
            try:
                return float(comp[key])
            except (TypeError, ValueError):
                pass
    return None


def get_status(doc: dict) -> Optional[str]:
    """Extract automated classification status."""
    for key in ("status", "classification", "automated_status"):
        if key in doc and doc[key]:
            return str(doc[key])
    classification = doc.get("classification_result") or {}
    return classification.get("status") or classification.get("classification")


def get_filename(doc: dict) -> Optional[str]:
    """Extract the document filename."""
    for key in ("file", "filename", "path", "input_file"):
        if key in doc and doc[key]:
            p = Path(str(doc[key]))
            return p.name
    return None


def get_oracle_fault(doc: dict) -> bool:
    """Check if oracle_fault flag is set."""
    val = doc.get("oracle_fault")
    if val is None:
        return False
    if isinstance(val, bool):
        return val
    if isinstance(val, str):
        return val.lower() in ("true", "suspected", "yes", "1")
    return bool(val)


def get_page_counts(doc: dict) -> tuple[Optional[int], Optional[int]]:
    """Return (our_pages, ref_pages)."""
    our = doc.get("our_pages") or doc.get("output_pages") or doc.get("page_count")
    ref = doc.get("ref_pages") or doc.get("reference_pages") or doc.get("ref_page_count")
    structural = doc.get("structural_result") or {}
    if our is None:
        our = structural.get("our_pages") or structural.get("output_page_count")
    if ref is None:
        ref = structural.get("ref_pages") or structural.get("reference_page_count")
    try:
        our = int(our) if our is not None else None
    except (TypeError, ValueError):
        our = None
    try:
        ref = int(ref) if ref is not None else None
    except (TypeError, ValueError):
        ref = None
    return our, ref


# ---------------------------------------------------------------------------
# Stratum membership checks
# ---------------------------------------------------------------------------

def in_high_ssim(doc: dict, _inv: Optional[dict]) -> bool:
    status = get_status(doc)
    ssim = get_ssim(doc)
    return status == "fully_correct" and ssim is not None and ssim >= 0.97


def in_mid_ssim(doc: dict, _inv: Optional[dict]) -> bool:
    status = get_status(doc)
    ssim = get_ssim(doc)
    return (
        status in ("minor_deviation", "major_deviation")
        and ssim is not None
        and 0.88 <= ssim <= 0.96
    )


def in_low_ssim(doc: dict, _inv: Optional[dict]) -> bool:
    status = get_status(doc)
    ssim = get_ssim(doc)
    return (
        status in ("render_fail", "partial_render")
        and ssim is not None
        and ssim < 0.88
    )


def in_scripted(doc: dict, inv: Optional[dict]) -> bool:
    if inv is None:
        return False
    filename = get_filename(doc)
    if filename is None:
        return False
    entry = inv.get(filename) or inv.get(str(Path(filename).stem)) or {}
    return bool(entry.get("has_formcalc")) or bool(entry.get("has_javascript"))


def in_oracle_suspect(doc: dict, _inv: Optional[dict]) -> bool:
    return get_oracle_fault(doc)


def in_dynamic(doc: dict, inv: Optional[dict]) -> bool:
    if inv is None:
        return False
    filename = get_filename(doc)
    if filename is None:
        return False
    entry = inv.get(filename) or inv.get(str(Path(filename).stem)) or {}
    xfa_type = entry.get("xfa_type") or entry.get("type") or ""
    return str(xfa_type).lower() == "dynamic"


STRATUM_CHECKS = {
    "high_ssim": in_high_ssim,
    "low_ssim": in_low_ssim,
    "mid_ssim": in_mid_ssim,
    "scripted": in_scripted,
    "oracle_suspect": in_oracle_suspect,
    "dynamic": in_dynamic,
}


# ---------------------------------------------------------------------------
# Inventory loading
# ---------------------------------------------------------------------------

def load_inventory(path: Optional[Path]) -> Optional[dict]:
    """Load inventory JSON into a filename-keyed dict."""
    if path is None or not path.exists():
        return None
    raw = load_json(path)
    # Support list or dict forms
    if isinstance(raw, list):
        result = {}
        for entry in raw:
            fname = entry.get("file") or entry.get("filename") or entry.get("path")
            if fname:
                result[Path(fname).name] = entry
                result[Path(fname).stem] = entry
        return result
    if isinstance(raw, dict):
        # Could be {filename: entry} or {"documents": [...]}
        docs = raw.get("documents")
        if docs is not None:
            return load_inventory(None)  # fallback: re-parse as list
        # Assume keys are filenames
        result = {}
        for k, v in raw.items():
            result[Path(k).name] = v
            result[Path(k).stem] = v
        return result
    return None


# ---------------------------------------------------------------------------
# Build a gold set document entry from a benchmark result doc
# ---------------------------------------------------------------------------

def make_gold_entry(doc: dict, stratum: str) -> dict:
    filename = get_filename(doc) or doc.get("file") or "unknown"
    ssim = get_ssim(doc)
    completeness = get_completeness(doc)
    our_pages, ref_pages = get_page_counts(doc)
    return {
        "file": filename,
        "stratum": stratum,
        "automated_status": get_status(doc),
        "ssim": round(ssim, 4) if ssim is not None else None,
        "completeness": round(completeness, 4) if completeness is not None else None,
        "our_pages": our_pages,
        "ref_pages": ref_pages,
        "human_label": None,
        "human_label_date": None,
        "human_label_by": None,
        "notes": "",
    }


# ---------------------------------------------------------------------------
# Core sampling logic
# ---------------------------------------------------------------------------

def stratified_sample(
    results_docs: list[dict],
    inventory: Optional[dict],
    strata_config: dict,
    seed: int = 42,
) -> tuple[list[dict], dict]:
    """
    Sample documents using stratified strategy.

    Returns:
        (selected_entries, strata_actual_counts)
    """
    rng = random.Random(seed)

    # Assign each result doc to the FIRST matching stratum (priority order)
    priority_order = ["oracle_suspect", "dynamic", "scripted", "high_ssim", "mid_ssim", "low_ssim"]
    stratum_pools: dict[str, list[dict]] = {s: [] for s in strata_config}

    for doc in results_docs:
        for stratum in priority_order:
            if stratum not in STRATUM_CHECKS:
                continue
            if STRATUM_CHECKS[stratum](doc, inventory):
                stratum_pools[stratum].append(doc)
                break

    selected: list[dict] = []
    strata_actual: dict = {}

    for stratum, config in strata_config.items():
        target = config["target"]
        pool = stratum_pools.get(stratum, [])
        sample_size = min(target, len(pool))
        sampled = rng.sample(pool, sample_size)
        strata_actual[stratum] = {"target": target, "actual": sample_size}
        for doc in sampled:
            selected.append(make_gold_entry(doc, stratum))

    return selected, strata_actual


# ---------------------------------------------------------------------------
# Load results documents
# ---------------------------------------------------------------------------

def load_results_docs(path: Path) -> list[dict]:
    """Load benchmark results JSON and return a flat list of document entries."""
    raw = load_json(path)
    if isinstance(raw, list):
        return raw
    if isinstance(raw, dict):
        # Try common keys
        for key in ("documents", "results", "entries", "files"):
            if key in raw and isinstance(raw[key], list):
                return raw[key]
        # Flat dict of filename -> result
        docs = []
        for k, v in raw.items():
            if isinstance(v, dict):
                entry = dict(v)
                if "file" not in entry and "filename" not in entry:
                    entry["file"] = k
                docs.append(entry)
        return docs
    return []


# ---------------------------------------------------------------------------
# Placeholder gold set (no results)
# ---------------------------------------------------------------------------

def make_placeholder(target: int, strategy: str, strata_config: dict) -> dict:
    strata = {
        name: {"target": cfg["target"], "actual": 0}
        for name, cfg in strata_config.items()
    }
    return {
        "version": "1.0",
        "created": datetime.date.today().isoformat(),
        "target_size": target,
        "actual_size": 0,
        "strategy": strategy,
        "strata": strata,
        "documents": [],
        "note": (
            "Gold set not yet populated. "
            "Run with --results after first baseline run (EVH-BASELINE-02)."
        ),
    }


# ---------------------------------------------------------------------------
# Main logic
# ---------------------------------------------------------------------------

def compose(args: argparse.Namespace) -> None:
    output_path = Path(args.output)
    strata_config = DEFAULT_STRATA_CONFIG  # Could be made configurable in future

    # --- No results: write placeholder ---
    if not args.results:
        print("No --results provided. Writing placeholder gold set.")
        gold = make_placeholder(args.target, args.strategy, strata_config)
        save_json(gold, output_path)
        return

    results_path = Path(args.results)
    if not results_path.exists():
        print(f"ERROR: results file not found: {results_path}", file=sys.stderr)
        sys.exit(1)

    inventory_path = Path(args.inventory) if args.inventory else None
    inventory = load_inventory(inventory_path)
    if args.inventory and inventory is None:
        print(f"WARNING: inventory file not found or unreadable: {args.inventory}", file=sys.stderr)

    results_docs = load_results_docs(results_path)
    print(f"Loaded {len(results_docs)} documents from results.")
    if inventory:
        print(f"Loaded inventory with {len(inventory) // 2} entries.")

    # --- Update mode: preserve already-labeled entries ---
    preserved: list[dict] = []
    preserved_files: set[str] = set()

    if args.update and output_path.exists():
        existing = load_json(output_path)
        if isinstance(existing, dict):
            for entry in existing.get("documents", []):
                if entry.get("human_label") is not None:
                    preserved.append(entry)
                    preserved_files.add(entry.get("file", ""))
            print(f"Preserved {len(preserved)} labeled entries from existing gold set.")

        # Exclude preserved files from results pool
        results_docs = [
            d for d in results_docs if get_filename(d) not in preserved_files
        ]

    # Adjust targets to account for preserved entries
    adjusted_config = {}
    preserved_by_stratum: dict[str, int] = {}
    for entry in preserved:
        s = entry.get("stratum", "")
        preserved_by_stratum[s] = preserved_by_stratum.get(s, 0) + 1

    for stratum, cfg in strata_config.items():
        already = preserved_by_stratum.get(stratum, 0)
        adjusted_target = max(0, cfg["target"] - already)
        adjusted_config[stratum] = dict(cfg, target=adjusted_target)

    selected, strata_actual = stratified_sample(
        results_docs, inventory, adjusted_config, seed=args.seed
    )

    # Merge preserved back in
    all_docs = preserved + selected

    # Compute final strata counts (including preserved)
    final_strata: dict = {}
    for stratum, cfg in strata_config.items():
        target = cfg["target"]
        actual = sum(1 for d in all_docs if d.get("stratum") == stratum)
        final_strata[stratum] = {"target": target, "actual": actual}

    gold = {
        "version": "1.0",
        "created": datetime.date.today().isoformat(),
        "target_size": args.target,
        "actual_size": len(all_docs),
        "strategy": args.strategy,
        "strata": final_strata,
        "documents": all_docs,
    }

    save_json(gold, output_path)
    print(f"\nGold set summary:")
    print(f"  Total documents: {len(all_docs)} / {args.target} target")
    for stratum, counts in final_strata.items():
        print(f"  {stratum:20s}  {counts['actual']:3d} / {counts['target']:3d}")

    unlabeled = sum(1 for d in all_docs if d.get("human_label") is None)
    labeled = len(all_docs) - unlabeled
    print(f"\n  Labeled: {labeled}  |  Awaiting review: {unlabeled}")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compose a stratified gold review set from XFA benchmark results."
    )
    parser.add_argument(
        "--results",
        metavar="FILE",
        default=None,
        help="Benchmark results JSON (e.g. benchmarks/enterprise-baseline-2026-04-19.json). "
             "If omitted, writes a placeholder gold set.",
    )
    parser.add_argument(
        "--inventory",
        metavar="FILE",
        default=None,
        help="Corpus inventory JSON (e.g. benchmarks/corpus_inventory.json). "
             "Required for 'scripted' and 'dynamic' strata.",
    )
    parser.add_argument(
        "--output",
        metavar="FILE",
        default="benchmarks/gold_set/gold_set.json",
        help="Output path for gold_set.json (default: benchmarks/gold_set/gold_set.json).",
    )
    parser.add_argument(
        "--target",
        type=int,
        default=40,
        help="Target gold set size (default: 40).",
    )
    parser.add_argument(
        "--strategy",
        default="stratified",
        choices=["stratified"],
        help="Sampling strategy (default: stratified).",
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Update mode: re-select from new results, preserving labeled entries.",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for reproducible sampling (default: 42).",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    compose(args)


if __name__ == "__main__":
    main()
