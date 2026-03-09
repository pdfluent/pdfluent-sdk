#!/usr/bin/env bash
# Build the pdf-capi Rust library as an XCFramework for iOS and macOS.
#
# Prerequisites:
#   rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin
#
# Usage:
#   cd <repo-root>
#   bash bindings/ios/scripts/build-xcframework.sh
#
# Output:
#   bindings/ios/PdfCApiFFI.xcframework/

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT_DIR="$REPO_ROOT/bindings/ios"

echo "==> Building pdf-capi for iOS (aarch64)..."
cargo build -p pdf-capi --release --target aarch64-apple-ios

echo "==> Building pdf-capi for iOS Simulator (x86_64)..."
cargo build -p pdf-capi --release --target x86_64-apple-ios

echo "==> Building pdf-capi for macOS (aarch64)..."
cargo build -p pdf-capi --release --target aarch64-apple-darwin

echo "==> Building pdf-capi for macOS (x86_64)..."
cargo build -p pdf-capi --release --target x86_64-apple-darwin

# Create universal macOS binary
echo "==> Creating universal macOS binary..."
mkdir -p "$REPO_ROOT/target/universal-macos/release"
lipo -create \
    "$REPO_ROOT/target/aarch64-apple-darwin/release/libpdf_capi.a" \
    "$REPO_ROOT/target/x86_64-apple-darwin/release/libpdf_capi.a" \
    -output "$REPO_ROOT/target/universal-macos/release/libpdf_capi.a"

# Create universal iOS Simulator binary (if both arm64 and x86_64 sim targets exist)
echo "==> Preparing simulator binary..."
SIM_LIB="$REPO_ROOT/target/x86_64-apple-ios/release/libpdf_capi.a"

# Remove old xcframework
rm -rf "$OUT_DIR/PdfCApiFFI.xcframework"

echo "==> Creating XCFramework..."
xcodebuild -create-xcframework \
    -library "$REPO_ROOT/target/aarch64-apple-ios/release/libpdf_capi.a" \
    -headers "$OUT_DIR/Sources/XfaPdf/include" \
    -library "$SIM_LIB" \
    -headers "$OUT_DIR/Sources/XfaPdf/include" \
    -library "$REPO_ROOT/target/universal-macos/release/libpdf_capi.a" \
    -headers "$OUT_DIR/Sources/XfaPdf/include" \
    -output "$OUT_DIR/PdfCApiFFI.xcframework"

echo "==> Done: $OUT_DIR/PdfCApiFFI.xcframework"
