#!/usr/bin/env python3
"""
xfa_visual_evidence.py — Zero-cost SSIM + heatmap comparator for XFA visual fidelity.

Compares a rendered PDF (ours) against an oracle PNG/PDF using SSIM, and produces
a 3-panel heatmap (ours / oracle / abs-diff) per page.

Zero paid API calls: only existing oracle artifacts are used. Scripts defaults to
--dry-run; no oracle generation API calls are ever made.

Usage:
    python3 scripts/xfa_visual_evidence.py \\
        --doc path/to/doc.pdf \\
        --oracle path/to/oracle.png \\
        [--out-dir benchmarks/runs/xfa_enterprise_plan/sprint2/heatmaps_demo/] \\
        [--dry-run]

Dependencies:
    pip install scikit-image pillow numpy
    Local tool: pdftoppm (poppler-utils) for PDF rendering.
    Check with: pdftoppm -v

Output JSON schema defined in scripts/xfa_visual_evidence_schema.json.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import numpy as np
    from PIL import Image
    from skimage.metrics import structural_similarity as ssim
except ImportError as e:
    print(f"ERROR: Missing Python dependency: {e}")
    print("Install with: pip install scikit-image pillow numpy")
    sys.exit(1)


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

PDFLUENT = os.environ.get(
    "PDFLUENT_BIN",
    str(Path(__file__).parent.parent / "target" / "release" / "pdfluent"),
)
PDFTOPPM = "pdftoppm"
DPI = 150
HEATMAP_COLORMAP = "hot"  # high-contrast red/yellow for diff


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def sha256_file(path: Path) -> str:
    """Return hex SHA-256 of a file."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def render_pdf_to_pngs(pdf_path: Path, out_dir: Path, dpi: int = DPI) -> list[Path]:
    """Render all pages of a PDF to PNG files. Returns list of PNG paths (sorted by page)."""
    out_dir.mkdir(parents=True, exist_ok=True)

    # Try pdfluent render first (preferred — uses our engine)
    pdfluent_bin = Path(PDFLUENT)
    if pdfluent_bin.exists():
        result = subprocess.run(
            [str(pdfluent_bin), "render", str(pdf_path), "-o", str(out_dir), "-d", str(dpi)],
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            pngs = sorted(out_dir.glob("page-*.png"))
            if pngs:
                return pngs
        # Fall through to pdftoppm on failure

    # Fall back to pdftoppm (poppler)
    prefix = out_dir / "pdftoppm_page"
    result = subprocess.run(
        [PDFTOPPM, "-r", str(dpi), "-png", str(pdf_path), str(prefix)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"PDF rendering failed for {pdf_path}.\n"
            f"pdftoppm stderr: {result.stderr}\n"
            f"Ensure pdftoppm (poppler-utils) is installed."
        )
    pngs = sorted(out_dir.glob("pdftoppm_page-*.png"))
    if not pngs:
        pngs = sorted(out_dir.glob("pdftoppm_page*.png"))
    return pngs


def load_oracle_pages(oracle_path: Path, tmp_dir: Path, dpi: int = DPI) -> list[Path]:
    """Load oracle as list of PNG paths. Handles both PNG and PDF oracle files."""
    ext = oracle_path.suffix.lower()
    if ext == ".png":
        return [oracle_path]
    elif ext == ".pdf":
        oracle_render_dir = tmp_dir / "oracle_render"
        oracle_render_dir.mkdir(parents=True, exist_ok=True)
        prefix = oracle_render_dir / "oracle_page"
        result = subprocess.run(
            [PDFTOPPM, "-r", str(dpi), "-png", str(oracle_path), str(prefix)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"Oracle PDF rendering failed: {oracle_path}\n"
                f"stderr: {result.stderr}"
            )
        pngs = sorted(oracle_render_dir.glob("oracle_page*.png"))
        return pngs
    else:
        raise ValueError(f"Unsupported oracle file type: {ext}. Expected .png or .pdf")


def resize_to_match(img_a: np.ndarray, img_b: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Resize img_b to match img_a dimensions if they differ."""
    if img_a.shape == img_b.shape:
        return img_a, img_b
    h, w = img_a.shape[:2]
    pil_b = Image.fromarray(img_b)
    pil_b = pil_b.resize((w, h), Image.LANCZOS)
    return img_a, np.array(pil_b)


def compute_ssim(ours: np.ndarray, oracle: np.ndarray) -> float:
    """Compute SSIM between two images. Converts to grayscale if needed."""
    # Convert to grayscale for SSIM
    if ours.ndim == 3:
        ours_gray = np.mean(ours, axis=2).astype(np.uint8)
    else:
        ours_gray = ours
    if oracle.ndim == 3:
        oracle_gray = np.mean(oracle, axis=2).astype(np.uint8)
    else:
        oracle_gray = oracle
    score, _ = ssim(ours_gray, oracle_gray, full=True, data_range=255)
    return float(score)


def build_heatmap(
    ours: np.ndarray,
    oracle: np.ndarray,
    out_path: Path,
) -> None:
    """Build and save 3-panel heatmap: ours | oracle | abs-diff."""
    # Ensure same dimensions
    if ours.shape != oracle.shape:
        h, w = ours.shape[:2]
        pil_oracle = Image.fromarray(oracle).resize((w, h), Image.LANCZOS)
        oracle = np.array(pil_oracle)

    # Compute abs diff on grayscale
    ours_gray = np.mean(ours, axis=2) if ours.ndim == 3 else ours
    oracle_gray = np.mean(oracle, axis=2) if oracle.ndim == 3 else oracle
    diff = np.abs(ours_gray.astype(np.float32) - oracle_gray.astype(np.float32))

    # Normalize diff to 0-255 and apply a simple hot colormap (red = high diff)
    diff_norm = (diff / 255.0 * 255).clip(0, 255).astype(np.uint8)
    # Apply hot colormap: 0=black, 128=red, 255=yellow/white
    r = np.clip(diff_norm * 2, 0, 255).astype(np.uint8)
    g = np.clip((diff_norm - 128) * 2, 0, 255).astype(np.uint8)
    b = np.zeros_like(diff_norm)
    diff_color = np.stack([r, g, b], axis=2)

    # Convert panels to RGB PIL images of same height
    panel_ours = Image.fromarray(ours if ours.ndim == 3 else np.stack([ours] * 3, axis=2))
    panel_oracle = Image.fromarray(oracle if oracle.ndim == 3 else np.stack([oracle] * 3, axis=2))
    panel_diff = Image.fromarray(diff_color)

    h = panel_ours.height
    w = panel_ours.width

    # Add label strip (20px)
    label_h = 20
    total_h = h + label_h
    combined = Image.new("RGB", (w * 3, total_h), color=(40, 40, 40))

    # Label background
    from PIL import ImageDraw
    draw = ImageDraw.Draw(combined)
    labels = ["ours", "oracle", "abs-diff"]
    for i, label in enumerate(labels):
        draw.text((i * w + 4, 2), label, fill=(255, 255, 255))

    # Paste panels below labels
    combined.paste(panel_ours, (0, label_h))
    combined.paste(panel_oracle, (w, label_h))
    combined.paste(panel_diff, (w * 2, label_h))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    combined.save(str(out_path), "PNG", optimize=True)


# ---------------------------------------------------------------------------
# Main pipeline
# ---------------------------------------------------------------------------


def run_comparison(
    doc_path: Path,
    oracle_path: Path,
    out_dir: Path,
    dry_run: bool = True,
) -> dict:
    """Run SSIM + heatmap comparison. Returns result dict matching JSON schema."""
    doc_sha = sha256_file(doc_path)
    doc_filename = doc_path.name

    print(f"Doc:    {doc_filename} (sha256: {doc_sha[:16]}...)")
    print(f"Oracle: {oracle_path}")
    print(f"Out:    {out_dir}")
    print(f"Mode:   {'DRY-RUN (no outputs written)' if dry_run else 'APPLY'}")

    if dry_run:
        # In dry-run mode, simulate the pipeline without writing files
        print("[dry-run] Would render doc pages, load oracle pages, compute SSIM, build heatmaps.")
        return {
            "doc_sha256": doc_sha,
            "doc_filename": doc_filename,
            "oracle_path": str(oracle_path),
            "dry_run": True,
            "per_page": [],
            "aggregate": {
                "mean_ssim": None,
                "min_ssim": None,
                "max_ssim": None,
                "page_count": 0,
            },
        }

    # Apply mode: render + compare
    with tempfile.TemporaryDirectory(prefix="xfa_vis_") as tmp:
        tmp_path = Path(tmp)

        # Render our version
        ours_render_dir = tmp_path / "ours"
        print("Rendering doc pages...")
        ours_pages = render_pdf_to_pngs(doc_path, ours_render_dir)
        print(f"  -> {len(ours_pages)} pages rendered")

        # Load oracle pages
        print("Loading oracle pages...")
        oracle_pages = load_oracle_pages(oracle_path, tmp_path)
        print(f"  -> {len(oracle_pages)} oracle pages loaded")

        page_count = min(len(ours_pages), len(oracle_pages))
        if page_count == 0:
            raise RuntimeError(
                f"No pages to compare (ours: {len(ours_pages)}, oracle: {len(oracle_pages)})"
            )
        if len(ours_pages) != len(oracle_pages):
            print(
                f"  WARNING: page count mismatch (ours={len(ours_pages)}, "
                f"oracle={len(oracle_pages)}). Comparing {page_count} pages."
            )

        per_page = []
        ssim_scores = []

        for page_idx in range(page_count):
            ours_img = np.array(Image.open(ours_pages[page_idx]).convert("RGB"))
            oracle_img = np.array(Image.open(oracle_pages[page_idx]).convert("RGB"))
            ours_img, oracle_img = resize_to_match(ours_img, oracle_img)

            score = compute_ssim(ours_img, oracle_img)
            ssim_scores.append(score)

            # Build heatmap output paths
            slug = doc_filename.replace(".pdf", "").replace(" ", "_")
            heatmap_path = out_dir / f"{slug}_page{page_idx + 1:02d}_heatmap.png"
            ours_out = out_dir / f"{slug}_page{page_idx + 1:02d}_ours.png"
            oracle_out = out_dir / f"{slug}_page{page_idx + 1:02d}_oracle.png"

            out_dir.mkdir(parents=True, exist_ok=True)

            # Save panel images
            Image.fromarray(ours_img).save(str(ours_out), "PNG", optimize=True)
            Image.fromarray(oracle_img).save(str(oracle_out), "PNG", optimize=True)

            # Build and save heatmap
            build_heatmap(ours_img, oracle_img, heatmap_path)

            print(
                f"  Page {page_idx + 1}: SSIM={score:.4f}  "
                f"heatmap={heatmap_path.name}"
            )

            per_page.append(
                {
                    "page_index": page_idx,
                    "ssim": round(score, 6),
                    "image_paths": {
                        "ours": str(ours_out),
                        "oracle": str(oracle_out),
                        "diff": str(heatmap_path),
                    },
                }
            )

    aggregate = {
        "mean_ssim": round(float(np.mean(ssim_scores)), 6),
        "min_ssim": round(float(np.min(ssim_scores)), 6),
        "max_ssim": round(float(np.max(ssim_scores)), 6),
        "page_count": page_count,
    }

    result = {
        "doc_sha256": doc_sha,
        "doc_filename": doc_filename,
        "oracle_path": str(oracle_path),
        "dry_run": False,
        "per_page": per_page,
        "aggregate": aggregate,
    }

    # Write JSON result
    json_path = out_dir / f"{doc_filename.replace('.pdf', '')}_ssim.json"
    with open(json_path, "w") as f:
        json.dump(result, f, indent=2)
    print(f"  Result JSON: {json_path}")
    print(
        f"  Aggregate: mean={aggregate['mean_ssim']:.4f}  "
        f"min={aggregate['min_ssim']:.4f}  "
        f"max={aggregate['max_ssim']:.4f}  "
        f"pages={aggregate['page_count']}"
    )

    return result


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description=(
            "XFA visual fidelity evidence pipeline. "
            "Computes SSIM and builds 3-panel heatmaps (ours/oracle/diff). "
            "Zero paid API calls — existing oracle artifacts only."
        )
    )
    parser.add_argument(
        "--doc",
        required=True,
        metavar="PDF_PATH",
        help="Path to the input PDF document to compare.",
    )
    parser.add_argument(
        "--oracle",
        required=True,
        metavar="PNG_OR_PDF_PATH",
        help="Path to oracle reference PNG or PDF.",
    )
    parser.add_argument(
        "--out-dir",
        default="benchmarks/runs/xfa_enterprise_plan/sprint2/heatmaps_demo",
        metavar="DIR",
        help="Output directory for heatmaps and SSIM JSON (default: heatmaps_demo/).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        default=True,
        help="Dry-run mode (default): print what would happen, no outputs written.",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        default=False,
        help=(
            "Apply mode: actually render pages, compute SSIM, write heatmaps. "
            "Does NOT make any API calls; uses only local tools + existing oracles."
        ),
    )
    parser.add_argument(
        "--json-out",
        metavar="JSON_PATH",
        help="Write result JSON to this path (in addition to out-dir).",
    )

    args = parser.parse_args()

    dry_run = not args.apply  # --apply overrides default dry-run

    doc_path = Path(args.doc)
    oracle_path = Path(args.oracle)
    out_dir = Path(args.out_dir)

    if not doc_path.exists():
        print(f"ERROR: Doc not found: {doc_path}")
        sys.exit(1)
    if not oracle_path.exists():
        print(f"ERROR: Oracle not found: {oracle_path}")
        sys.exit(1)

    result = run_comparison(doc_path, oracle_path, out_dir, dry_run=dry_run)

    if args.json_out:
        Path(args.json_out).parent.mkdir(parents=True, exist_ok=True)
        with open(args.json_out, "w") as f:
            json.dump(result, f, indent=2)
        print(f"JSON written: {args.json_out}")


if __name__ == "__main__":
    main()
