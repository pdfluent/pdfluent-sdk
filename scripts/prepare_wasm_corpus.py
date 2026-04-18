#!/usr/bin/env python3
"""Prepare the WASM CI corpus sample by randomly sampling PDFs from a source directory.

Copies --count PDFs (chosen with --seed for reproducibility) from --source into --out.
Designed to produce a stable, reproducible subset of a larger corpus for WASM gate runs.

Usage:
    python3 scripts/prepare_wasm_corpus.py \
        --source corpus/ci-sample-200 \
        --out corpus/wasm-sample-100 \
        --count 100 \
        --seed 42
"""
import argparse
import random
import shutil
import sys
from pathlib import Path


def main():
    p = argparse.ArgumentParser(
        description="Sample PDFs from a source corpus directory into an output directory."
    )
    p.add_argument("--source", required=True, help="Source corpus directory containing PDFs")
    p.add_argument("--out", required=True, help="Output directory for sampled PDFs")
    p.add_argument(
        "--count",
        type=int,
        default=50,
        help="Number of PDFs to sample (default: 50)",
    )
    p.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for reproducibility (default: 42)",
    )
    args = p.parse_args()

    source_dir = Path(args.source)
    out_dir = Path(args.out)

    if not source_dir.exists():
        print(f"ERROR: source directory not found: {source_dir}", file=sys.stderr)
        sys.exit(1)

    all_pdfs = sorted(source_dir.glob("*.pdf"))
    if not all_pdfs:
        print(f"ERROR: no PDFs found in source directory: {source_dir}", file=sys.stderr)
        sys.exit(1)

    if args.count > len(all_pdfs):
        print(
            f"WARN: requested {args.count} PDFs but only {len(all_pdfs)} available; "
            f"using all {len(all_pdfs)}",
            file=sys.stderr,
        )
        selected = all_pdfs
    else:
        rng = random.Random(args.seed)
        selected = rng.sample(all_pdfs, args.count)
        selected = sorted(selected)

    out_dir.mkdir(parents=True, exist_ok=True)

    copied = 0
    skipped = 0
    for pdf in selected:
        dest = out_dir / pdf.name
        if dest.exists():
            skipped += 1
            continue
        shutil.copy2(pdf, dest)
        copied += 1

    total = len(list(out_dir.glob("*.pdf")))
    print(
        f"WASM corpus ready: {total} PDFs in {out_dir} "
        f"(copied={copied}, cached={skipped}, seed={args.seed})"
    )


if __name__ == "__main__":
    main()
