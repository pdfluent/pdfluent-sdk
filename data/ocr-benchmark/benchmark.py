#!/usr/bin/env python3
"""Benchmark OCR engines on the prepared corpus."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import subprocess
import sys
import time
import unicodedata
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path


BENCH_DIR = Path(__file__).resolve().parent
REPO_ROOT = BENCH_DIR.parent.parent
CORPUS_PATH = BENCH_DIR / "corpus.json"
RESULTS_PATH = BENCH_DIR / "results.json"
CARGO_TARGET_DIR = "/tmp/codex-ocr-corpus-target"
PADDLE_CARGO_TARGET_DIR = "/tmp/codex-ocr-paddle-target"
ORT_DYLIB_PATH = (
    "/tmp/onnxruntime-arm64/onnxruntime-osx-arm64-1.24.2/lib/libonnxruntime.dylib"
)


def load_dotenv() -> None:
    env_path = REPO_ROOT / ".env"
    if not env_path.exists():
        return

    for raw_line in env_path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip("'").strip('"')
        os.environ.setdefault(key, value)


def normalize_text(text: str) -> str:
    text = unicodedata.normalize("NFKC", text).lower()
    return " ".join(text.split())


def bag_overlap_similarity(a: str, b: str) -> float:
    ca, cb = Counter(a), Counter(b)
    overlap = sum((ca & cb).values())
    total = max(sum(ca.values()), sum(cb.values()))
    return overlap / total if total else 0.0


def levenshtein_similarity(a: str, b: str) -> float:
    """Normalized Levenshtein similarity (0-1)."""
    if not a and not b:
        return 1.0
    if not a or not b:
        return 0.0
    if max(len(a), len(b)) > 4000:
        return bag_overlap_similarity(a, b)

    if len(a) < len(b):
        a, b = b, a

    previous = list(range(len(b) + 1))
    for i, char_a in enumerate(a, start=1):
        current = [i]
        for j, char_b in enumerate(b, start=1):
            current.append(
                min(
                    previous[j] + 1,
                    current[j - 1] + 1,
                    previous[j - 1] + (0 if char_a == char_b else 1),
                )
            )
        previous = current

    distance = previous[-1]
    return 1.0 - distance / max(len(a), len(b))


def run_json_request(url: str, api_key: str, payload: dict, extra_headers: dict[str, str] | None = None) -> dict:
    body = json.dumps(payload).encode("utf-8")
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }
    if extra_headers:
        headers.update(extra_headers)

    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    with urllib.request.urlopen(request, timeout=90) as response:
        return json.loads(response.read().decode("utf-8"))


def image_to_data_url(image_path: str) -> str:
    path = Path(image_path)
    mime = "image/png" if path.suffix.lower() == ".png" else "image/jpeg"
    return f"data:{mime};base64,{base64.b64encode(path.read_bytes()).decode('ascii')}"


def strip_markdown_artifacts(text: str) -> str:
    text = re.sub(r"!\[[^\]]*\]\([^)]+\)", " ", text)
    return text.strip()


def ocr_tesseract(image_path: str) -> str:
    """OCR via Tesseract CLI."""
    result = subprocess.run(
        ["tesseract", image_path, "stdout", "-l", "eng"],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "tesseract failed")
    return result.stdout.strip()


def ocr_mistral(image_path: str, api_key: str) -> str:
    """OCR via Mistral's OCR endpoint."""
    payload = {
        "model": "mistral-ocr-latest",
        "document": {"type": "image_url", "image_url": image_to_data_url(image_path)},
    }
    response = run_json_request("https://api.mistral.ai/v1/ocr", api_key, payload)
    markdown_pages = [page.get("markdown", "") for page in response.get("pages", [])]
    return strip_markdown_artifacts("\n\n".join(page for page in markdown_pages if page))


def ocr_openrouter(image_path: str, api_key: str, model: str) -> str:
    """OCR via an OpenRouter vision-capable model."""
    payload = {
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": (
                            "Extract all visible text from this document image. "
                            "Return only the extracted text."
                        ),
                    },
                    {"type": "image_url", "image_url": {"url": image_to_data_url(image_path)}},
                ],
            }
        ],
    }
    response = run_json_request(
        "https://openrouter.ai/api/v1/chat/completions",
        api_key,
        payload,
        extra_headers={"HTTP-Referer": "https://github.com/jasperdew/xfa-native-rust"},
    )
    return response["choices"][0]["message"]["content"].strip()


def ocr_ocrs(image_path: str) -> str:
    """OCR via the repository's ocrs example binary."""
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "-p",
            "pdf-engine",
            "--features",
            "ocr",
            "--example",
            "ocr_single_image",
            "--",
            image_path,
        ],
        capture_output=True,
        text=True,
        timeout=240,
        check=False,
        cwd=REPO_ROOT,
        env={**os.environ, "CARGO_TARGET_DIR": CARGO_TARGET_DIR},
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(stderr or "cargo example failed")
    return result.stdout.strip()


def ocr_paddle_onnx(image_path: str) -> str:
    """OCR via the repository's PaddleOCR ONNX backend."""
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "--target",
            "aarch64-apple-darwin",
            "-p",
            "pdf-engine",
            "--features",
            "ocr-onnx",
            "--example",
            "ocr_single_image",
            "--",
            image_path,
        ],
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
        cwd=REPO_ROOT,
        env={
            **os.environ,
            "CARGO_TARGET_DIR": PADDLE_CARGO_TARGET_DIR,
            "ORT_DYLIB_PATH": ORT_DYLIB_PATH,
        },
    )
    if result.returncode != 0:
        stderr = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(stderr or "cargo example failed")
    return result.stdout.strip()


def summarize_scores(scores: list[dict]) -> tuple[float, int]:
    valid = [item["similarity"] for item in scores if "error" not in item]
    average = sum(valid) / len(valid) if valid else 0.0
    return average, len(valid)


def build_engines(requested: list[str]) -> dict[str, callable]:
    engines: dict[str, callable] = {}
    mistral_key = os.environ.get("MISTRAL_API_KEY")
    openrouter_key = os.environ.get("OPENROUTER_API_KEY")
    openrouter_model = os.environ.get("OPENROUTER_MODEL", "openai/gpt-4.1-mini")

    for name in requested:
        if name == "ocrs":
            engines[name] = ocr_ocrs
        elif name == "paddle-onnx":
            engines[name] = ocr_paddle_onnx
        elif name == "tesseract":
            engines[name] = ocr_tesseract
        elif name == "mistral":
            if not mistral_key:
                print("[warn] Skipping mistral: MISTRAL_API_KEY is not set", file=sys.stderr)
                continue
            engines[name] = lambda image_path, key=mistral_key: ocr_mistral(image_path, key)
        elif name == "openrouter":
            if not openrouter_key:
                print("[warn] Skipping openrouter: OPENROUTER_API_KEY is not set", file=sys.stderr)
                continue
            engines[name] = (
                lambda image_path, key=openrouter_key, model=openrouter_model: ocr_openrouter(image_path, key, model)
            )
        else:
            raise ValueError(f"Unsupported engine: {name}")

    if not engines:
        raise ValueError("No runnable engines selected")
    return engines


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--engines",
        default="ocrs,tesseract",
        help="Comma-separated engine list: ocrs,tesseract,paddle-onnx,mistral,openrouter",
    )
    parser.add_argument("--limit", type=int, default=20, help="Maximum number of corpus pairs to evaluate")
    parser.add_argument("--source", help="Optional corpus source filter, for example: funsd or sroie")
    return parser.parse_args()


def main() -> int:
    load_dotenv()
    args = parse_args()

    if not CORPUS_PATH.exists():
        print(f"[error] Missing corpus file: {CORPUS_PATH}. Run prepare_corpus.py first.", file=sys.stderr)
        return 1

    corpus = json.loads(CORPUS_PATH.read_text())
    if args.source:
        corpus = [pair for pair in corpus if pair.get("source") == args.source]
    sample = corpus[: args.limit]
    if not sample:
        print("[error] No corpus entries match the requested filters.", file=sys.stderr)
        return 1

    engines = build_engines([name.strip() for name in args.engines.split(",") if name.strip()])
    results = {
        "metadata": {
            "engines": list(engines),
            "limit": args.limit,
            "source": args.source,
            "sample_size": len(sample),
            "cargo_target_dir": CARGO_TARGET_DIR,
            "paddle_cargo_target_dir": PADDLE_CARGO_TARGET_DIR,
            "ort_dylib_path": ORT_DYLIB_PATH,
        },
        "results": {engine: [] for engine in engines},
    }

    for index, pair in enumerate(sample, start=1):
        print(f"[{index}/{len(sample)}] {pair['source']}:{pair['id']} ({Path(pair['image']).name})")
        gt = normalize_text(pair["text"])

        for name, fn in engines.items():
            started = time.perf_counter()
            try:
                raw_text = fn(pair["image"])
                normalized_text = normalize_text(raw_text)
                similarity = levenshtein_similarity(gt, normalized_text)
                elapsed = time.perf_counter() - started
                results["results"][name].append(
                    {
                        "id": pair["id"],
                        "source": pair["source"],
                        "image": pair["image"],
                        "similarity": similarity,
                        "seconds": round(elapsed, 3),
                        "ground_truth_chars": len(gt),
                        "ocr_chars": len(normalized_text),
                    }
                )
                print(f"  {name}: {similarity:.1%} in {elapsed:.1f}s")
            except (urllib.error.URLError, KeyError, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
                elapsed = time.perf_counter() - started
                results["results"][name].append(
                    {
                        "id": pair["id"],
                        "source": pair["source"],
                        "image": pair["image"],
                        "similarity": 0.0,
                        "seconds": round(elapsed, 3),
                        "error": str(exc),
                    }
                )
                print(f"  {name}: ERROR {exc}")

    print("\n=== BENCHMARK RESULTATEN ===")
    for name, scores in results["results"].items():
        average, success_count = summarize_scores(scores)
        print(f"{name}: {average:.1%} gemiddeld ({success_count}/{len(scores)} succesvol)")

    RESULTS_PATH.write_text(json.dumps(results, indent=2))
    print(f"\n[done] Results written to {RESULTS_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
