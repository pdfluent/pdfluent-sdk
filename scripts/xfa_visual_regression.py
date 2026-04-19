#!/usr/bin/env python3
"""
XFA Visual Regression Test Suite.

For each XFA PDF in a corpus directory, renders it with our engine and
computes SSIM against a reference (pre-stored PNG or mutool oracle).
Reports pass/fail per file with configurable SSIM threshold.

Usage:
    python3 scripts/xfa_visual_regression.py \
        --binary /path/to/pdfluent \
        --corpus /path/to/xfa-corpus \
        --oracle \
        --threshold 0.90 \
        --output results.json

    # With pre-stored reference PNGs:
    python3 scripts/xfa_visual_regression.py \
        --binary /path/to/pdfluent \
        --corpus /path/to/xfa-corpus \
        --reference /path/to/reference-pngs \
        --threshold 0.90 \
        --output results.json

Exit codes:
    0  — all files pass (or no failures)
    1  — one or more files failed the SSIM threshold
"""
import argparse
import concurrent.futures
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image
from skimage.metrics import structural_similarity as ssim_fn

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

DEFAULT_THRESHOLD = 0.90
DEFAULT_DPI = "150"
RENDER_TIMEOUT = 60  # seconds per PDF render


# ---------------------------------------------------------------------------
# SSIM helpers (reused from run_gate_ssim.py pattern)
# ---------------------------------------------------------------------------

def compute_ssim(a_path: str, b_path: str) -> float:
    """Compute grayscale SSIM between two image files."""
    a = Image.open(a_path).convert("L")
    b = Image.open(b_path).convert("L")
    w, h = min(a.width, b.width), min(a.height, b.height)
    if a.size != (w, h):
        a = a.resize((w, h), Image.LANCZOS)
    if b.size != (w, h):
        b = b.resize((w, h), Image.LANCZOS)
    return float(ssim_fn(np.array(a), np.array(b)))


# ---------------------------------------------------------------------------
# Oracle: mutool
# ---------------------------------------------------------------------------

def render_oracle(pdf: Path, out_png: Path, dpi: str) -> str | None:
    """Render page 1 via mutool. Returns None on success, error string on failure."""
    r = subprocess.run(
        ["mutool", "draw", "-q", "-r", dpi, "-o", str(out_png), str(pdf), "1"],
        capture_output=True,
        timeout=RENDER_TIMEOUT,
    )
    stdout = r.stdout.decode("utf-8", errors="replace") if r.stdout else ""
    stderr = r.stderr.decode("utf-8", errors="replace") if r.stderr else ""
    if r.returncode != 0 or not out_png.exists() or out_png.stat().st_size < 100:
        return (stderr or stdout or "mutool failed")[-300:]
    return None


# ---------------------------------------------------------------------------
# Our engine: pdfluent
# ---------------------------------------------------------------------------

def render_engine(pdf: Path, binary: str, out_dir: Path, dpi: str) -> tuple[Path | None, str | None]:
    """Render page 1 via our binary. Returns (png_path, error_string)."""
    expected = out_dir / "page-1.png"
    cmd = [binary, "render", "-o", str(out_dir), "-d", dpi, "-p", "1", str(pdf)]
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=RENDER_TIMEOUT)
        stdout = r.stdout.decode("utf-8", errors="replace") if r.stdout else ""
        stderr = r.stderr.decode("utf-8", errors="replace") if r.stderr else ""
    except subprocess.TimeoutExpired:
        return None, f"timeout: >{RENDER_TIMEOUT}s"

    if r.returncode == 2:
        return None, "encrypted"
    if r.returncode == 3:
        return None, "degenerate"
    if r.returncode == 4:
        return None, "xfa_error"
    if r.returncode != 0 or not expected.exists() or expected.stat().st_size < 100:
        return None, (stderr or stdout or "render failed")[-300:]

    # Guard against degenerate tiny PNGs.
    try:
        with Image.open(str(expected)) as img:
            if img.width < 8 or img.height < 8:
                return None, f"degenerate_png_{img.width}x{img.height}"
    except Exception:
        pass

    return expected, None


# ---------------------------------------------------------------------------
# Per-file processing
# ---------------------------------------------------------------------------

def process_file(
    fname: str,
    corpus: Path,
    binary: str,
    dpi: str,
    threshold: float,
    use_oracle: bool,
    reference_dir: Path | None,
    index: int,
    total: int,
) -> dict:
    pdf = corpus / fname
    if not pdf.exists():
        print(f"[{index}/{total}] SKIP {fname} (not found)", flush=True)
        return {"file": fname, "status": "not_found", "ssim": None, "pass": False}

    print(f"[{index}/{total}] {fname}", flush=True)

    tmpdir = Path(tempfile.mkdtemp(prefix="xfa_regression_"))
    try:
        # ----------------------------------------------------------------
        # Render with our engine
        # ----------------------------------------------------------------
        engine_out = tmpdir / "engine_out"
        engine_out.mkdir()
        engine_png, engine_err = render_engine(pdf, binary, engine_out, dpi)

        if engine_err is not None:
            print(f"  engine error: {engine_err}", flush=True)
            return {"file": fname, "status": engine_err, "ssim": None, "pass": False}

        # ----------------------------------------------------------------
        # Get reference image (oracle or pre-stored)
        # ----------------------------------------------------------------
        if use_oracle:
            ref_png = tmpdir / "oracle.png"
            oracle_err = render_oracle(pdf, ref_png, dpi)
            if oracle_err:
                print(f"  oracle error: {oracle_err}", flush=True)
                return {"file": fname, "status": "oracle_fault", "ssim": None, "pass": False}
        else:
            # Look for a pre-stored PNG named after the PDF (e.g. form.pdf -> form.png)
            stem = Path(fname).stem
            ref_png = reference_dir / f"{stem}.png"
            if not ref_png.exists():
                # Also try same name with .png extension
                ref_png = reference_dir / (fname + ".png")
            if not ref_png.exists():
                print(f"  no reference PNG for {fname}", flush=True)
                return {"file": fname, "status": "no_reference", "ssim": None, "pass": False}

        # ----------------------------------------------------------------
        # Compute SSIM
        # ----------------------------------------------------------------
        try:
            score = round(compute_ssim(str(engine_png), str(ref_png)), 4)
        except Exception as e:
            return {"file": fname, "status": "ssim_error", "ssim": None, "pass": False,
                    "error": str(e)[:200]}

        passed = score >= threshold
        status = "pass" if passed else "fail"
        print(f"  ssim={score:.4f}  {status}", flush=True)
        return {"file": fname, "status": status, "ssim": score, "pass": passed}

    except Exception as e:
        return {"file": fname, "status": "crash", "ssim": None, "pass": False,
                "error": str(e)[:300]}
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    p = argparse.ArgumentParser(
        description="XFA visual regression — SSIM comparison against oracle or reference PNGs."
    )
    p.add_argument("--binary", required=True, help="Path to pdfluent binary")
    p.add_argument("--corpus", required=True, help="Directory containing XFA PDF files")
    p.add_argument(
        "--oracle",
        action="store_true",
        help="Use mutool as reference (oracle mode). Mutually exclusive with --reference.",
    )
    p.add_argument(
        "--reference",
        default=None,
        help="Directory with pre-stored reference PNGs. Mutually exclusive with --oracle.",
    )
    p.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD,
                   help=f"SSIM pass threshold (default: {DEFAULT_THRESHOLD})")
    p.add_argument("--dpi", default=DEFAULT_DPI,
                   help=f"Render DPI (default: {DEFAULT_DPI})")
    p.add_argument("--output", required=True, help="Output JSON path")
    p.add_argument("--workers", type=int, default=1,
                   help="Parallel workers (default: 1)")
    p.add_argument("--limit", type=int, default=None,
                   help="Max number of files to process")
    args = p.parse_args()

    # Validate mutually exclusive options.
    if args.oracle and args.reference:
        sys.exit("ERROR: --oracle and --reference are mutually exclusive")
    if not args.oracle and not args.reference:
        sys.exit("ERROR: one of --oracle or --reference is required")

    corpus = Path(args.corpus)
    if not corpus.exists():
        sys.exit(f"ERROR: corpus directory not found: {corpus}")

    binary = args.binary
    if not Path(binary).exists():
        sys.exit(f"ERROR: binary not found: {binary}")

    reference_dir: Path | None = None
    if args.reference:
        reference_dir = Path(args.reference)
        if not reference_dir.exists():
            sys.exit(f"ERROR: reference directory not found: {reference_dir}")

    files = sorted(f.name for f in corpus.glob("*.pdf"))
    if not files:
        sys.exit(f"ERROR: no PDF files found in {corpus}")

    if args.limit:
        files = files[: args.limit]

    total = len(files)
    mode = "oracle (mutool)" if args.oracle else f"reference ({reference_dir})"
    print(
        f"XFA Visual Regression: {total} files, threshold={args.threshold}, "
        f"dpi={args.dpi}, mode={mode}, workers={args.workers}"
    )

    threshold = args.threshold
    dpi = args.dpi
    use_oracle = args.oracle
    results: list[dict] = []

    if args.workers <= 1:
        for i, fname in enumerate(files, 1):
            row = process_file(
                fname, corpus, binary, dpi, threshold, use_oracle, reference_dir, i, total
            )
            results.append(row)
    else:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
            futures = {
                executor.submit(
                    process_file,
                    fname, corpus, binary, dpi, threshold, use_oracle, reference_dir, i, total,
                ): fname
                for i, fname in enumerate(files, 1)
            }
            for future in concurrent.futures.as_completed(futures):
                try:
                    results.append(future.result())
                except Exception as e:
                    fname = futures[future]
                    results.append({
                        "file": fname, "status": "crash", "ssim": None, "pass": False,
                        "error": str(e)[:300],
                    })

        # Restore file order.
        order = {fname: i for i, fname in enumerate(files)}
        results.sort(key=lambda r: order.get(r["file"], 999999))

    # ----------------------------------------------------------------
    # Summary
    # ----------------------------------------------------------------
    n_pass = sum(1 for r in results if r["pass"])
    n_fail = sum(1 for r in results if not r["pass"] and r["status"] in ("fail",))
    n_error = sum(1 for r in results if not r["pass"] and r["status"] not in ("fail",))
    ssim_scores = [r["ssim"] for r in results if r["ssim"] is not None]
    mean_ssim = round(float(np.mean(ssim_scores)), 4) if ssim_scores else None

    summary = {
        "total": total,
        "pass": n_pass,
        "fail": n_fail,
        "error": n_error,
        "mean_ssim": mean_ssim,
        "threshold": threshold,
    }

    payload = {"summary": summary, "results": results}
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, indent=2))

    print("\n=== XFA VISUAL REGRESSION SUMMARY ===")
    print(json.dumps(summary, indent=2))
    print(f"\n{n_pass} pass, {n_fail} fail, {n_error} error  |  mean SSIM: {mean_ssim}")
    print(f"Wrote {out_path}")

    return 1 if n_fail > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
