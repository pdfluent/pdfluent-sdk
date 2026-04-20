#!/usr/bin/env python3
"""GL-CA-01: pixel energy grid analysis for M#56."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from PIL import Image

from m56_common import (
    BENCHMARKS_DIR,
    TMP_FLAT_DIR,
    TMP_PAGE_DIR,
    detect_binary,
    flatten_pdf,
    minor_deviation_docs,
    parse_args,
    render_page_one,
    resolve_oracle_png,
    resolve_source_pdf,
    write_csv,
)


OUTPUT_PATH = BENCHMARKS_DIR / "m56_pixel_energy_raw.csv"


def compute_grid(our_path: Path, oracle_path: Path, grid_size: int = 10) -> np.ndarray:
    our = np.array(Image.open(our_path).convert("RGB"), dtype=float)
    ref = np.array(Image.open(oracle_path).convert("RGB"), dtype=float)
    if our.shape != ref.shape:
        resized = Image.fromarray(our.astype(np.uint8)).resize((ref.shape[1], ref.shape[0]))
        our = np.array(resized, dtype=float)
    sq = np.sum((our - ref) ** 2, axis=2)
    h, w = sq.shape
    grid = np.zeros((grid_size, grid_size))
    for r in range(grid_size):
        for c in range(grid_size):
            r0, r1 = r * h // grid_size, (r + 1) * h // grid_size
            c0, c1 = c * w // grid_size, (c + 1) * w // grid_size
            grid[r, c] = sq[r0:r1, c0:c1].sum()
    total = grid.sum()
    return (grid / total * 100.0) if total > 0 else grid


def top_tiles(grid: np.ndarray) -> list[tuple[int, int, float]]:
    tiles: list[tuple[int, int, float]] = []
    for r in range(grid.shape[0]):
        for c in range(grid.shape[1]):
            tiles.append((r, c, float(grid[r, c])))
    tiles.sort(key=lambda item: (-item[2], item[0], item[1]))
    return tiles[:3]


def main() -> None:
    parse_args("Compute M56 pixel energy grid output.")
    binary = detect_binary()
    rows = []

    for item in minor_deviation_docs():
        doc_name = item["doc_name"]
        ssim = item["ssim"]
        doc_stem = Path(doc_name).stem

        source_pdf = resolve_source_pdf(doc_name)
        flat_pdf = flatten_pdf(binary, source_pdf, TMP_FLAT_DIR / f"{doc_stem}.pdf")
        our_png = render_page_one(flat_pdf, TMP_PAGE_DIR / doc_stem)
        oracle_png = resolve_oracle_png(doc_name, source_pdf)
        grid = compute_grid(our_png, oracle_png)

        for rank, (tile_r, tile_c, energy_pct) in enumerate(top_tiles(grid), start=1):
            rows.append(
                {
                    "doc_name": doc_name,
                    "ssim": f"{ssim:.6f}",
                    "tile_r": tile_r,
                    "tile_c": tile_c,
                    "energy_pct": f"{energy_pct:.4f}",
                    "rank": rank,
                }
            )

    if len(rows) != 192:
        raise RuntimeError(f"Expected 192 output rows, found {len(rows)}")

    unique_docs = {row["doc_name"] for row in rows}
    if len(unique_docs) != 64:
        raise RuntimeError(f"Expected 64 unique docs, found {len(unique_docs)}")

    write_csv(
        OUTPUT_PATH,
        ["doc_name", "ssim", "tile_r", "tile_c", "energy_pct", "rank"],
        rows,
    )

    print(json.dumps({"output": str(OUTPUT_PATH), "rows": len(rows), "unique_docs": len(unique_docs)}))


if __name__ == "__main__":
    main()
