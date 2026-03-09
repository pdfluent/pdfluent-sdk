#!/usr/bin/env bash
# Build the pdf-java Rust library for Android targets using cargo-ndk.
#
# Prerequisites:
#   cargo install cargo-ndk
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   Set ANDROID_NDK_HOME to your NDK path (or let cargo-ndk auto-detect).
#
# Usage:
#   cd <repo-root>
#   bash bindings/android/scripts/build-android.sh
#
# Output:
#   bindings/android/xfapdf/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libpdf_java.so

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
JNI_LIBS="$REPO_ROOT/bindings/android/xfapdf/src/main/jniLibs"

echo "==> Building pdf-java for Android arm64-v8a..."
cargo ndk -t arm64-v8a -o "$JNI_LIBS" build -p pdf-java --release

echo "==> Building pdf-java for Android armeabi-v7a..."
cargo ndk -t armeabi-v7a -o "$JNI_LIBS" build -p pdf-java --release

echo "==> Building pdf-java for Android x86_64..."
cargo ndk -t x86_64 -o "$JNI_LIBS" build -p pdf-java --release

echo "==> Done. Native libraries:"
find "$JNI_LIBS" -name "*.so" -type f
