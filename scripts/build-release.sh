#!/bin/bash
set -euo pipefail

TARGET_DIR="${CARGO_TARGET_DIR:-target}"
NATIVE_DIR="${TARGET_DIR}/release-speed"
WASM_DIR="${TARGET_DIR}/wasm32-unknown-unknown/release"

echo "Building release binaries..."

# Native binaries: optimize for speed.
cargo build --profile release-speed -p xfa-cli -p xfa-test-runner

# WASM: optimize for size.
cargo build --target wasm32-unknown-unknown --release -p pdf-engine

# Extra stripping for native binaries if the tool is available.
strip "${NATIVE_DIR}/xfa-cli" 2>/dev/null || true
strip "${NATIVE_DIR}/xfa-test-runner" 2>/dev/null || true

if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --strip-debug --strip-producers \
    "${WASM_DIR}/pdf_engine.wasm" \
    -o "${WASM_DIR}/pdf_engine.opt.wasm"
  echo "WASM optimized:"
  ls -lh "${WASM_DIR}/pdf_engine.opt.wasm"
fi

echo "Release binaries ready:"
ls -lh "${NATIVE_DIR}/xfa-cli" "${NATIVE_DIR}/xfa-test-runner" 2>/dev/null || true
ls -lh "${WASM_DIR}"/pdf_engine*.wasm 2>/dev/null || true
