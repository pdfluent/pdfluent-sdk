#!/usr/bin/env python3
"""OCR benchmark wrapper for xfa-native-rust.

Routes engine labels to the existing Rust OCR accuracy harness so cloud
backends are exercised through `pdf_engine::OcrBackend` implementations
instead of bespoke direct API calls.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys


ENGINE_FEATURES = {
    "mistral-sdk": "ocr-mistral",
    "ocrs-sdk": "ocr",
    "paddle-onnx": "ocr-onnx",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run OCR benchmarks through the Rust OCR backend abstraction."
    )
    parser.add_argument(
        "--engine",
        choices=sorted(ENGINE_FEATURES),
        required=True,
        help="OCR engine label to benchmark.",
    )
    parser.add_argument(
        "--target-dir",
        default=os.environ.get("CARGO_TARGET_DIR", "/tmp/codex-cloud-ocr-target"),
        help="Cargo target directory to use.",
    )
    parser.add_argument(
        "--example",
        default="check_ocr_accuracy",
        help="xfa-test-runner example to execute.",
    )
    parser.add_argument(
        "example_args",
        nargs=argparse.REMAINDER,
        help="Extra arguments passed to the example after `--`.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    feature = ENGINE_FEATURES[args.engine]
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = args.target_dir

    command = [
        "cargo",
        "run",
        "-p",
        "xfa-test-runner",
        "--features",
        feature,
        "--example",
        args.example,
    ]
    if args.example_args:
        command.append("--")
        command.extend(args.example_args)

    print("Running:", " ".join(command))
    completed = subprocess.run(command, env=env)
    return completed.returncode


if __name__ == "__main__":
    sys.exit(main())
