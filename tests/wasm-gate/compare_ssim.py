#!/usr/bin/env python3
"""
Compare native vs WASM renders using SSIM and write a structured JSON result.

Matches PNG files by basename (stem): a native `foo.png` is paired with a
WASM `foo.png`.  Files present in only one directory are ignored.

Exit codes:
  0  all pairs are at or above --threshold (or threshold is 0.0)
  1  one or more pairs are below --threshold, no pairs found, or fatal error

Usage:
  python3 compare_ssim.py \\
    --native    /tmp/native-renders \\
    --wasm      /tmp/wasm-renders \\
    --threshold 0.97 \\
    --out       /tmp/wasm_gate_result.json
"""

import argparse
import json
import os
import sys
import warnings

import numpy as np
from PIL import Image

# ── SSIM backend ──────────────────────────────────────────────────────────
try:
    from skimage.metrics import structural_similarity as _sk_ssim

    def _ssim(a: np.ndarray, b: np.ndarray) -> float:
        """scikit-image SSIM (preferred, handles multi-channel correctly)."""
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            return float(_sk_ssim(a, b, data_range=1.0, channel_axis=2))

except ImportError:
    # Fallback: mean-squared-error converted to a similarity score.
    # Less accurate than SSIM but avoids a hard dependency in environments
    # where scikit-image is not installed.
    def _ssim(a: np.ndarray, b: np.ndarray) -> float:  # type: ignore[misc]
        mse = float(np.mean((a - b) ** 2))
        return max(0.0, 1.0 - mse)


# ── Image utilities ───────────────────────────────────────────────────────

def load_rgb(path: str) -> np.ndarray:
    """Load image as float32 (H, W, 3) RGB array normalised to [0, 1]."""
    return np.asarray(Image.open(path).convert('RGB'), dtype=np.float32) / 255.0


def resize_to(arr: np.ndarray, h: int, w: int) -> np.ndarray:
    """Resize arr to (h, w, c) using Lanczos resampling."""
    img = Image.fromarray((arr * 255).astype(np.uint8))
    return np.asarray(img.resize((w, h), Image.LANCZOS), dtype=np.float32) / 255.0


def ssim_pair(native_path: str, wasm_path: str):
    """Return (score: float, error: str | None)."""
    try:
        native = load_rgb(native_path)
        wasm   = load_rgb(wasm_path)
        if native.shape != wasm.shape:
            wasm = resize_to(wasm, native.shape[0], native.shape[1])
        return _ssim(native, wasm), None
    except Exception as exc:
        return None, str(exc)


# ── PNG discovery ─────────────────────────────────────────────────────────

def collect_pngs(directory: str) -> dict:
    """Return {stem: abspath} for every .png in *directory* (case-insensitive)."""
    result = {}
    try:
        for fname in os.listdir(directory):
            if fname.lower().endswith('.png'):
                stem = os.path.splitext(fname)[0]
                result[stem] = os.path.join(directory, fname)
    except FileNotFoundError:
        pass
    return result


# ── Main ──────────────────────────────────────────────────────────────────

def main() -> None:
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument('--native',    required=True, metavar='DIR',
                    help='Directory with native-rendered PNGs')
    ap.add_argument('--wasm',      required=True, metavar='DIR',
                    help='Directory with WASM-rendered PNGs')
    ap.add_argument('--threshold', type=float, default=0.97, metavar='FLOAT',
                    help='Minimum acceptable SSIM score (default: 0.97)')
    ap.add_argument('--out',       required=True, metavar='JSON',
                    help='Path for the JSON result file')
    args = ap.parse_args()

    native_pngs = collect_pngs(args.native)
    wasm_pngs   = collect_pngs(args.wasm)
    common      = sorted(set(native_pngs) & set(wasm_pngs))

    print(f'Native PNGs   : {len(native_pngs)}')
    print(f'WASM PNGs     : {len(wasm_pngs)}')
    print(f'Matched pairs : {len(common)}')
    print(f'Threshold     : {args.threshold}')

    # ── No pairs ─────────────────────────────────────────────────────────
    if not common:
        msg = (f'No matching PNG pairs found '
               f'(native={len(native_pngs)}, wasm={len(wasm_pngs)})')
        result = {
            'status':       'no-pairs',
            'reason':       msg,
            'threshold':    args.threshold,
            'native_count': len(native_pngs),
            'wasm_count':   len(wasm_pngs),
        }
        _write_json(args.out, result)
        print(f'ERROR: {msg}', file=sys.stderr)
        sys.exit(1)

    # ── Compute SSIM for every pair ───────────────────────────────────────
    scores   = []   # list of {'name': str, 'ssim': float}
    failures = []   # pairs below threshold or with errors

    for stem in common:
        score, err = ssim_pair(native_pngs[stem], wasm_pngs[stem])
        if err is not None:
            entry = {'name': stem, 'ssim': None, 'error': err}
            failures.append(entry)
            print(f'  {stem}: ERROR — {err}', file=sys.stderr)
        else:
            entry = {'name': stem, 'ssim': round(score, 6)}
            scores.append(entry)
            label = '  ← BELOW THRESHOLD' if score < args.threshold else ''
            print(f'  {stem}: {score:.4f}{label}')
            if score < args.threshold:
                failures.append(entry)

    # ── Summary ──────────────────────────────────────────────────────────
    valid = [e['ssim'] for e in scores]
    mean_ssim = float(np.mean(valid))  if valid else 0.0
    min_ssim  = float(np.min(valid))   if valid else 0.0
    passed    = len(failures) == 0 and len(scores) > 0

    print()
    print(f'Pairs scored  : {len(scores)} / {len(common)}')
    print(f'Mean SSIM     : {mean_ssim:.4f}')
    print(f'Min SSIM      : {min_ssim:.4f}')
    print(f'Below threshold: {len(failures)}')
    print(f'Result        : {"PASS" if passed else "FAIL"}')

    result = {
        'status':    'pass' if passed else 'fail',
        'threshold': args.threshold,
        'pairs':     len(common),
        'scored':    len(scores),
        'mean_ssim': round(mean_ssim, 6),
        'min_ssim':  round(min_ssim, 6),
        'failures':  failures,
        'scores':    scores,
    }
    _write_json(args.out, result)

    if not passed:
        print(
            f'\nFAILED: {len(failures)} pair(s) below threshold {args.threshold}',
            file=sys.stderr,
        )
        for entry in failures[:10]:
            val = f'{entry["ssim"]:.4f}' if entry.get('ssim') is not None else 'error'
            print(f'  {entry["name"]}: {val}', file=sys.stderr)
        sys.exit(1)

    print(f'\nPASSED: all {len(scores)} pair(s) >= {args.threshold}')


def _write_json(path: str, data: dict) -> None:
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as fh:
        json.dump(data, fh, indent=2)
    print(f'Result written → {path}')


if __name__ == '__main__':
    main()
