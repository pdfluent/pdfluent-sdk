#!/usr/bin/env python3
"""EVH-C2-01: Visual comparison of two rendered PNG files.

Computes SSIM, pHash distance, per-region SSIM (4x4 grid), and pixel diff ratio.

CLI usage:
    python3 scripts/visual_compare.py \
        --ours /tmp/our.png \
        --reference /tmp/ref.png \
        --output /tmp/result.json

Also importable as a module:
    from visual_compare import compare_images
"""
import argparse
import json
import sys
from pathlib import Path

# Optional heavy imports — degrade gracefully if unavailable
try:
    import numpy as np
    _NUMPY_OK = True
except ImportError:
    _NUMPY_OK = False

try:
    from PIL import Image
    _PIL_OK = True
except ImportError:
    _PIL_OK = False

try:
    from skimage.metrics import structural_similarity as _ssim_fn
    _SKIMAGE_OK = True
except ImportError:
    _SKIMAGE_OK = False

try:
    import imagehash
    _IMAGEHASH_OK = True
except ImportError:
    _IMAGEHASH_OK = False

# ---------------------------------------------------------------------------
# Thresholds
# ---------------------------------------------------------------------------

SSIM_PASS = 0.94
SSIM_PARTIAL = 0.85


def _visual_verdict(ssim_score: float | None) -> str:
    if ssim_score is None:
        return "fail"
    if ssim_score >= SSIM_PASS:
        return "pass"
    if ssim_score >= SSIM_PARTIAL:
        return "partial"
    return "fail"


# ---------------------------------------------------------------------------
# Core comparison
# ---------------------------------------------------------------------------

def compare_images(our_path: str, reference_path: str) -> dict:
    """Compare two PNG files and return a rich comparison dict.

    Returns a JSON-serialisable dict with keys:
        ssim, phash_distance, region_ssim, pixel_diff_ratio, visual_acceptable
    On error, returns a dict with error key and visual_acceptable="fail".
    """
    result: dict = {
        "ssim": None,
        "phash_distance": None,
        "region_ssim": {},
        "pixel_diff_ratio": None,
        "visual_acceptable": "fail",
    }

    if not _PIL_OK:
        result["error"] = "Pillow not available"
        return result

    if not _NUMPY_OK:
        result["error"] = "numpy not available"
        return result

    # ------------------------------------------------------------------
    # Load images
    # ------------------------------------------------------------------
    try:
        img_a = Image.open(our_path)
        img_b = Image.open(reference_path)
    except Exception as exc:
        result["error"] = f"failed to open image: {exc}"
        return result

    # Resize to common dimensions (match to reference size, LANCZOS)
    if img_a.size != img_b.size:
        img_a = img_a.resize(img_b.size, Image.LANCZOS)

    # Convert to grayscale for SSIM / pixel diff
    gray_a = img_a.convert("L")
    gray_b = img_b.convert("L")

    arr_a = np.array(gray_a, dtype=np.float32)
    arr_b = np.array(gray_b, dtype=np.float32)

    # ------------------------------------------------------------------
    # Overall SSIM
    # ------------------------------------------------------------------
    if _SKIMAGE_OK:
        try:
            score = float(_ssim_fn(arr_a, arr_b, data_range=255.0))
            result["ssim"] = round(score, 6)
        except Exception as exc:
            result["ssim"] = None
            result["ssim_error"] = str(exc)
    else:
        result["ssim_error"] = "scikit-image not available"

    # ------------------------------------------------------------------
    # pHash distance
    # ------------------------------------------------------------------
    if _IMAGEHASH_OK:
        try:
            hash_a = imagehash.phash(img_a)
            hash_b = imagehash.phash(img_b)
            result["phash_distance"] = int(hash_a - hash_b)
        except Exception as exc:
            result["phash_error"] = str(exc)
    # else: leave phash_distance as None — gracefully skipped

    # ------------------------------------------------------------------
    # 4x4 region SSIM
    # ------------------------------------------------------------------
    if _SKIMAGE_OK:
        try:
            h, w = arr_a.shape
            rows, cols = 4, 4
            region_ssim: dict[str, float] = {}
            for r in range(rows):
                for c in range(cols):
                    r0 = r * h // rows
                    r1 = (r + 1) * h // rows
                    c0 = c * w // cols
                    c1 = (c + 1) * w // cols
                    tile_a = arr_a[r0:r1, c0:c1]
                    tile_b = arr_b[r0:r1, c0:c1]
                    if tile_a.size == 0 or tile_b.size == 0:
                        continue
                    # win_size must be odd and <= min(tile dimension)
                    min_dim = min(tile_a.shape)
                    win = min(7, min_dim if min_dim % 2 == 1 else min_dim - 1)
                    if win < 3:
                        continue
                    tile_score = float(
                        _ssim_fn(tile_a, tile_b, data_range=255.0, win_size=win)
                    )
                    region_ssim[f"{r}_{c}"] = round(tile_score, 6)
            result["region_ssim"] = region_ssim
        except Exception as exc:
            result["region_ssim_error"] = str(exc)

    # ------------------------------------------------------------------
    # Pixel diff ratio
    # ------------------------------------------------------------------
    try:
        diff = np.abs(arr_a - arr_b)
        diff_count = int(np.sum(diff > 10))
        total_pixels = int(arr_a.size)
        result["pixel_diff_ratio"] = round(diff_count / total_pixels, 6) if total_pixels > 0 else 0.0
    except Exception as exc:
        result["pixel_diff_ratio_error"] = str(exc)

    # ------------------------------------------------------------------
    # Verdict
    # ------------------------------------------------------------------
    result["visual_acceptable"] = _visual_verdict(result.get("ssim"))

    return result


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Compare two rendered PNG files (SSIM + pHash + region grid + pixel diff)"
    )
    p.add_argument("--ours", required=True, help="Path to our rendered PNG")
    p.add_argument("--reference", required=True, help="Path to reference PNG")
    p.add_argument("--output", required=True, help="Path to write JSON result")
    args = p.parse_args()

    our_path = Path(args.ours)
    ref_path = Path(args.reference)

    if not our_path.exists():
        sys.exit(f"ERROR: --ours file not found: {our_path}")
    if not ref_path.exists():
        sys.exit(f"ERROR: --reference file not found: {ref_path}")

    result = compare_images(str(our_path), str(ref_path))

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2))
    print(f"visual_compare: ssim={result.get('ssim')}  "
          f"phash_dist={result.get('phash_distance')}  "
          f"verdict={result['visual_acceptable']}")
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
